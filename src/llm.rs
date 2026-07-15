/// LLM-based language detection for video content.
///
/// Uses the video title + description to ask an LLM what language the content is in.
/// Returns an ISO 639-1 language code (e.g. "id", "en", "ja").
///
/// Fallback chain:
///   1. LLM detection (Groq → OpenAI)
///   2. `info.language` from yt-dlp metadata
///   3. "en" (English)

use crate::config::WatchConfig;

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
        // Truncate long descriptions to save tokens
        if desc.len() > 500 { &desc[..500] } else { desc }
    );

    // Try Groq first (fast, free tier)
    if let Some(ref key) = config.groq_api_key {
        if let Some(lang) = call_llm(
            key,
            "https://api.groq.com/openai/v1/chat/completions",
            "llama-3.1-8b-instant",
            &prompt,
        )
        .await
        {
            return Some(lang);
        }
    }

    // Fallback to OpenAI
    if let Some(ref key) = config.openai_api_key {
        if let Some(lang) = call_llm(
            key,
            "https://api.openai.com/v1/chat/completions",
            "gpt-4o-mini",
            &prompt,
        )
        .await
        {
            return Some(lang);
        }
    }

    None
}

/// Call an OpenAI-compatible chat completions endpoint.
async fn call_llm(api_key: &str, url: &str, model: &str, prompt: &str) -> Option<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .ok()?;

    let body = serde_json::json!({
        "model": model,
        "messages": [
            {"role": "system", "content": "You are a language detector. Return ONLY a 2-letter ISO 639-1 language code. No explanation."},
            {"role": "user", "content": prompt}
        ],
        "max_tokens": 10,
        "temperature": 0.0,
    });

    let resp = client
        .post(url)
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() {
        return None;
    }

    let json: serde_json::Value = resp.json().await.ok()?;
    let content = json["choices"][0]["message"]["content"]
        .as_str()?
        .trim()
        .to_lowercase();

    // Extract just the language code (LLM might return extra text)
    let lang = extract_lang_code(&content);
    if is_valid_lang_code(&lang) {
        Some(lang)
    } else {
        None
    }
}

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
