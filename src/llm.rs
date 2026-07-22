/// LLM utilities for video analysis: language detection and moment selection.
///
/// Fallback chain for all LLM calls:
///   1. Groq (llama-3.1-8b-instant) — fast, free tier
///   2. OpenAI (gpt-4o-mini) — paid fallback

use crate::config::WatchConfig;
use crate::moments::KeyMoment;

/// Detect content language from video title + description using an LLM.
///
/// Returns `Some(lang_code)` on success, `None` if all LLM backends fail.
pub async fn detect_language_llm(
    title: &str,
    description: Option<&str>,
    config: &WatchConfig,
) -> Option<String> {
    let desc = description.unwrap_or("(no description)");
    let prompt = format!(
        "Based on this video title and description, what is the primary language of the content? \
         Return ONLY the ISO 639-1 language code (e.g. \"en\", \"id\", \"ja\", \"ko\", \"es\"). \
         Nothing else.\n\n\
         Title: {}\n\
         Description: {}",
        title,
        // Truncate long descriptions to save tokens (char-safe for CJK)
        if desc.chars().count() > 500 { desc.chars().take(500).collect::<String>() } else { desc.to_string() }
    );

    let system = "You are a language detector. Return ONLY a 2-letter ISO 639-1 language code. No explanation.";

    // Try Groq first (fast, free tier)
    if let Some(ref key) = config.groq_api_key {
        if let Some(raw) = call_llm(
            key,
            "https://api.groq.com/openai/v1/chat/completions",
            "llama-3.1-8b-instant",
            &prompt,
            system,
            10,
            0.0,
        )
        .await
        {
            let lang = extract_lang_code(&raw.to_lowercase());
            if is_valid_lang_code(&lang) {
                return Some(lang);
            }
        }
    }

    // Fallback to OpenAI
    if let Some(ref key) = config.openai_api_key {
        if let Some(raw) = call_llm(
            key,
            "https://api.openai.com/v1/chat/completions",
            "gpt-4o-mini",
            &prompt,
            system,
            10,
            0.0,
        )
        .await
        {
            let lang = extract_lang_code(&raw.to_lowercase());
            if is_valid_lang_code(&lang) {
                return Some(lang);
            }
        }
    }

    None
}

/// Select key moments from a video transcript using an LLM.
///
/// Builds the moment detection prompt from transcript + metadata, calls
/// Groq (llama-3.1-8b-instant) first, falls back to OpenAI (gpt-4o-mini).
///
/// Returns parsed `KeyMoment` entries sorted by priority, or `None` if
/// all backends fail or the response can't be parsed.
pub async fn select_moments(
    transcript_text: &str,
    title: &str,
    uploader: &str,
    duration: f64,
    scene_text: &str,
    config: &WatchConfig,
) -> Option<Vec<KeyMoment>> {
    // Enrich transcript text with scene boundary context when available
    let full_text = if scene_text.is_empty() || scene_text == "No scene changes detected." {
        transcript_text.to_string()
    } else {
        format!("{}\n\nScene boundaries:\n{}", transcript_text, scene_text)
    };

    let prompt = crate::moments::generate_prompt(
        &full_text,
        title,
        uploader,
        duration,
        50,  // max_moments
        None, // min_moments (auto-calculate from duration)
    );

    let system = "You are a video analyst identifying key moments that need visual verification. \
                   Return ONLY a valid JSON array. No markdown fences, no explanation.";

    // Try Groq first
    if let Some(ref key) = config.groq_api_key {
        if let Some(response) = call_llm(
            key,
            "https://api.groq.com/openai/v1/chat/completions",
            "llama-3.1-8b-instant",
            &prompt,
            system,
            4096,
            0.0,
        )
        .await
        {
            let segments = Vec::new();
            let moments = crate::moments::parse_moments_response(&response, &segments);
            if !moments.is_empty() {
                eprintln!("[watch2] Groq returned {} moments", moments.len());
                return Some(moments);
            }
        }
    }

    // Fallback to OpenAI
    if let Some(ref key) = config.openai_api_key {
        if let Some(response) = call_llm(
            key,
            "https://api.openai.com/v1/chat/completions",
            "gpt-4o-mini",
            &prompt,
            system,
            4096,
            0.0,
        )
        .await
        {
            let segments = Vec::new();
            let moments = crate::moments::parse_moments_response(&response, &segments);
            if !moments.is_empty() {
                eprintln!("[watch2] OpenAI returned {} moments", moments.len());
                return Some(moments);
            }
        }
    }

    None
}

/// Call an OpenAI-compatible chat completions endpoint.
///
/// Returns the raw model response text on success, or `None` on any failure.
/// The caller is responsible for interpreting the content (language code, JSON, etc.).
pub async fn call_llm(
    api_key: &str,
    url: &str,
    model: &str,
    prompt: &str,
    system_prompt: &str,
    max_tokens: u32,
    temperature: f64,
) -> Option<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .ok()?;

    let body = serde_json::json!({
        "model": model,
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": prompt}
        ],
        "max_tokens": max_tokens,
        "temperature": temperature,
    });

    let resp = client
        .post(url)
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() {
        eprintln!("[watch2] LLM {} returned {}", model, resp.status());
        return None;
    }

    let json: serde_json::Value = resp.json().await.ok()?;
    let content = json["choices"][0]["message"]["content"]
        .as_str()?
        .trim()
        .to_string();

    Some(content)
}

// ─── Language code helpers ────────────────────────────────────────────────

/// Extract a 2-letter ISO 639-1 code from LLM output.
///
/// The LLM might return:
/// - "id" (clean)
/// - "The language is Indonesian (id)" (verbose)
/// - "id." (with punctuation)
fn extract_lang_code(text: &str) -> String {
    let cleaned = text.trim().trim_end_matches('.').trim().to_lowercase();

    // Direct 2-letter match
    if cleaned.len() == 2 && cleaned.chars().all(|c| c.is_ascii_alphabetic()) {
        return cleaned;
    }

    // Look for pattern like "(id)" or "code: id"
    if let Some(start) = cleaned.find('(') {
        if let Some(end) = cleaned.find(')') {
            let inside = cleaned[start + 1..end].trim();
            if inside.len() == 2 && inside.chars().all(|c| c.is_ascii_alphabetic()) {
                return inside.to_string();
            }
        }
    }

    // Look for last word that's a valid 2-letter code
    for word in cleaned.split_whitespace().rev() {
        let word = word.trim_end_matches('.').trim_end_matches(',');
        if word.len() == 2 && word.chars().all(|c| c.is_ascii_alphabetic()) {
            return word.to_string();
        }
    }

    cleaned
}

/// Check if a string is a plausible ISO 639-1 code.
fn is_valid_lang_code(code: &str) -> bool {
    code.len() == 2 && code.chars().all(|c| c.is_ascii_alphabetic())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_lang_code_clean() {
        assert_eq!(extract_lang_code("id"), "id");
        assert_eq!(extract_lang_code("en"), "en");
        assert_eq!(extract_lang_code("ja"), "ja");
    }

    #[test]
    fn test_extract_lang_code_verbose() {
        assert_eq!(extract_lang_code("The language is Indonesian (id)"), "id");
        assert_eq!(extract_lang_code("Language: Japanese (ja)"), "ja");
    }

    #[test]
    fn test_extract_lang_code_with_punctuation() {
        assert_eq!(extract_lang_code("id."), "id");
        assert_eq!(extract_lang_code("ID,"), "id");
    }

    #[test]
    fn test_is_valid_lang_code() {
        assert!(is_valid_lang_code("id"));
        assert!(is_valid_lang_code("en"));
        assert!(is_valid_lang_code("ja"));
        assert!(!is_valid_lang_code("indonesian"));
        assert!(!is_valid_lang_code("i"));
        assert!(!is_valid_lang_code("12"));
    }
}
