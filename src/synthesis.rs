//! Synthesis module for combining transcript and visual verification data
//! into a grounded, accurate summary.

use crate::output::TranscriptSegment;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

/// Result of synthesizing multiple analysis sources.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthesisResult {
    pub summary: String,
    #[serde(default)]
    pub key_corrections: Vec<serde_json::Value>,
    #[serde(default)]
    pub speaker_identification: Vec<serde_json::Value>,
    #[serde(default)]
    pub visual_evidence: Vec<serde_json::Value>,
    #[serde(default)]
    pub uncertainties: Vec<String>,
}

/// Prompt template for LLM-based synthesis.
const SYNTHESIS_PROMPT: &str = r#"You are synthesizing a video analysis from multiple sources to produce a grounded, accurate summary.

Video Metadata:
{metadata}

Transcript (timestamped):
{transcript}

Visual Verifications:
{verifications}

Your task: Produce a grounded, accurate summary that:

1. **Uses corrected transcript** — Apply any corrections from visual verifications
2. **Identifies speakers** — Who said what, based on visual cues
3. **Cites timestamps** — Every claim must reference when it was said/shown
4. **Notes uncertainties** — Be explicit about what you're unsure about

## Output Format
Return a JSON object with:
{{
  "summary": "Comprehensive summary of the video content",
  "key_corrections": [
    {{"original": "Raknarok", "corrected": "Ragnarok", "timestamp": "0:54", "source": "visual verification"}}
  ],
  "speaker_identification": [
    {{"speaker": "George", "evidence": "Discord UI shows name at 0:26", "quotes": ["quote 1"]}}
  ],
  "visual_evidence": [
    {{"timestamp": "0:54", "finding": "Game title screen shows 'Ragnarok Online'", "corrects_transcript": true}}
  ],
  "uncertainties": ["Uncertain about speaker attribution at 2:09-2:30"]
}}

Return ONLY valid JSON. No markdown, no explanation."#;

/// Regex to strip markdown code blocks from LLM responses.
static CODE_BLOCK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"```(?:json)?\s*\n?(?s)(.*?)\n?\s*```").unwrap());

/// Regex to extract the outermost JSON object as a fallback.
static JSON_OBJECT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)\{.*\}").unwrap());

/// Generate the synthesis prompt by filling in transcript, verifications, and metadata.
pub fn generate_synthesis_prompt(
    transcript_text: &str,
    verified_moments: &[serde_json::Value],
    metadata: &serde_json::Value,
) -> String {
    let verifications_str = serde_json::to_string_pretty(verified_moments)
        .unwrap_or_else(|_| "[]".to_string());
    let metadata_str = serde_json::to_string_pretty(metadata)
        .unwrap_or_else(|_| "{}".to_string());

    SYNTHESIS_PROMPT
        .replace("{transcript}", transcript_text)
        .replace("{verifications}", &verifications_str)
        .replace("{metadata}", &metadata_str)
}

/// Parse a JSON response from the LLM into a SynthesisResult.
///
/// Handles common LLM output quirks: markdown code blocks, extra whitespace,
/// and non-JSON preamble/postfix text. Falls back to a regex extraction of
/// the outermost `{...}` block. Returns an empty result on total failure.
pub fn parse_synthesis_response(response: &str) -> SynthesisResult {
    // First try: direct parse
    if let Ok(result) = serde_json::from_str::<SynthesisResult>(response) {
        return result;
    }

    // Second try: strip markdown code blocks
    if let Some(caps) = CODE_BLOCK_RE.captures(response) {
        if let Some(inner) = caps.get(1) {
            if let Ok(result) = serde_json::from_str::<SynthesisResult>(inner.as_str()) {
                return result;
            }
        }
    }

    // Third try: regex fallback to extract outermost JSON object
    if let Some(m) = JSON_OBJECT_RE.find(response) {
        if let Ok(result) = serde_json::from_str::<SynthesisResult>(m.as_str()) {
            return result;
        }
    }

    // Total failure: return empty result
    SynthesisResult {
        summary: String::new(),
        key_corrections: Vec::new(),
        speaker_identification: Vec::new(),
        visual_evidence: Vec::new(),
        uncertainties: Vec::new(),
    }
}

/// Apply corrections from synthesis output to transcript segments.
///
/// Performs case-insensitive matching with case preservation on the first letter.
/// Punctuation is stripped for matching only; the corrected text preserves the
/// original punctuation and surrounding characters.
pub fn apply_corrections_to_transcript(
    segments: &[TranscriptSegment],
    corrections: &[serde_json::Value],
) -> Vec<TranscriptSegment> {
    if corrections.is_empty() {
        return segments.to_vec();
    }

    // Build a map: lowercase(original) -> corrected
    let correction_map: std::collections::HashMap<String, String> = corrections
        .iter()
        .filter_map(|c| {
            let original = c.get("original")?.as_str()?;
            let corrected = c.get("corrected")?.as_str()?;
            Some((original.to_lowercase(), corrected.to_string()))
        })
        .collect();

    if correction_map.is_empty() {
        return segments.to_vec();
    }

    segments
        .iter()
        .map(|seg| {
            let corrected_text = apply_word_corrections(&seg.text, &correction_map);
            TranscriptSegment {
                start: seg.start,
                end: seg.end,
                text: corrected_text,
                words: seg.words.clone(),
            }
        })
        .collect()
}

/// Strip punctuation from a word for matching purposes (alphanumeric, apostrophes, hyphens only).
fn strip_punctuation(word: &str) -> String {
    word.chars()
        .filter(|c| c.is_alphanumeric() || *c == '\'' || *c == '-')
        .collect()
}

/// Apply word-level corrections to a text string.
///
/// Walks through the text character by character, accumulating words. When a
/// non-word character is encountered, the accumulated word is checked against
/// the correction map and replaced if found (with case preservation).
fn apply_word_corrections(text: &str, corrections: &std::collections::HashMap<String, String>) -> String {
    let mut result = String::with_capacity(text.len());
    let mut current_word = String::new();

    for ch in text.chars() {
        if ch.is_alphanumeric() || ch == '\'' || ch == '-' {
            current_word.push(ch);
        } else {
            if !current_word.is_empty() {
                let corrected = correct_word(&current_word, corrections);
                result.push_str(&corrected);
                current_word.clear();
            }
            result.push(ch);
        }
    }

    // Flush trailing word
    if !current_word.is_empty() {
        result.push_str(&correct_word(&current_word, corrections));
    }

    result
}

/// Apply a correction to a single word, preserving case of the first letter.
fn correct_word(
    word: &str,
    corrections: &std::collections::HashMap<String, String>,
) -> String {
    let clean = strip_punctuation(word);

    if let Some(corrected) = corrections.get(&clean.to_lowercase()) {
        // Preserve case: if original word started with uppercase, capitalize first letter
        let fixed = if clean.chars().next().map_or(false, |c| c.is_uppercase()) {
            let mut chars = corrected.chars();
            if let Some(first) = chars.next() {
                let mut s: String = first.to_uppercase().collect();
                s.extend(chars);
                s
            } else {
                corrected.clone()
            }
        } else {
            corrected.clone()
        };
        fixed
    } else {
        word.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(start: f64, end: f64, text: &str) -> TranscriptSegment {
        TranscriptSegment {
            start,
            end,
            text: text.to_string(),
            words: None,
        }
    }

    #[test]
    fn test_parse_synthesis_response_valid_json() {
        let json = r#"{
            "summary": "Test summary",
            "key_corrections": [],
            "speaker_identification": [],
            "visual_evidence": [],
            "uncertainties": ["test uncertainty"]
        }"#;
        let result = parse_synthesis_response(json);
        assert_eq!(result.summary, "Test summary");
        assert_eq!(result.uncertainties, vec!["test uncertainty"]);
    }

    #[test]
    fn test_parse_synthesis_response_in_code_block() {
        let response = r#"Here's the analysis:

```json
{
    "summary": "Wrapped in code block",
    "key_corrections": [],
    "speaker_identification": [],
    "visual_evidence": [],
    "uncertainties": []
}
```

Hope this helps!"#;
        let result = parse_synthesis_response(response);
        assert_eq!(result.summary, "Wrapped in code block");
    }

    #[test]
    fn test_parse_synthesis_response_code_block_no_json_label() {
        let response = "```\n{\"summary\": \"No label\",\"key_corrections\":[],\"speaker_identification\":[],\"visual_evidence\":[],\"uncertainties\":[]}\n```";
        let result = parse_synthesis_response(response);
        assert_eq!(result.summary, "No label");
    }

    #[test]
    fn test_parse_synthesis_response_with_preamble() {
        let response = "Sure, here is the result:\n{\"summary\": \"Preamble test\",\"key_corrections\":[],\"speaker_identification\":[],\"visual_evidence\":[],\"uncertainties\":[]}\nDone.";
        let result = parse_synthesis_response(response);
        assert_eq!(result.summary, "Preamble test");
    }

    #[test]
    fn test_parse_synthesis_response_empty() {
        let result = parse_synthesis_response("not json at all");
        assert!(result.summary.is_empty());
        assert!(result.key_corrections.is_empty());
    }

    #[test]
    fn test_parse_synthesis_response_defaults() {
        let json = r#"{"summary": "minimal"}"#;
        let result = parse_synthesis_response(json);
        assert_eq!(result.summary, "minimal");
        assert!(result.key_corrections.is_empty());
        assert!(result.uncertainties.is_empty());
    }

    #[test]
    fn test_generate_synthesis_prompt_fills_template() {
        let transcript = "[00:00] Hello world";
        let verifications = vec![
            serde_json::json!({"timestamp": "0:00", "finding": "Clear audio"}),
        ];
        let metadata = serde_json::json!({"title": "Test Video", "duration": 60});

        let prompt = generate_synthesis_prompt(transcript, &verifications, &metadata);
        assert!(prompt.contains("[00:00] Hello world"));
        assert!(prompt.contains("Test Video"));
        assert!(prompt.contains("Clear audio"));
        assert!(prompt.contains("synthesizing"));
    }

    #[test]
    fn test_apply_corrections_to_transcript_basic() {
        let segments = vec![seg(0.0, 5.0, "The Raknarok game is fun")];
        let corrections = vec![serde_json::json!({
            "original": "Raknarok",
            "corrected": "Ragnarok",
            "timestamp": "0:54",
            "source": "visual verification"
        })];

        let result = apply_corrections_to_transcript(&segments, &corrections);
        assert_eq!(result[0].text, "The Ragnarok game is fun");
    }

    #[test]
    fn test_apply_corrections_preserves_case() {
        let segments = vec![seg(0.0, 5.0, "Raknarok is great")];
        let corrections = vec![serde_json::json!({
            "original": "raknarok",
            "corrected": "ragnarok"
        })];

        let result = apply_corrections_to_transcript(&segments, &corrections);
        assert_eq!(result[0].text, "Ragnarok is great");
    }

    #[test]
    fn test_apply_corrections_preserves_punctuation() {
        let segments = vec![seg(0.0, 5.0, "Hello, Raknarok! How are you?")];
        let corrections = vec![serde_json::json!({
            "original": "Raknarok",
            "corrected": "Ragnarok"
        })];

        let result = apply_corrections_to_transcript(&segments, &corrections);
        assert_eq!(result[0].text, "Hello, Ragnarok! How are you?");
    }

    #[test]
    fn test_apply_corrections_empty_corrections() {
        let segments = vec![seg(0.0, 5.0, "No changes")];
        let result = apply_corrections_to_transcript(&segments, &[]);
        assert_eq!(result[0].text, "No changes");
    }

    #[test]
    fn test_apply_corrections_empty_segments() {
        let corrections = vec![serde_json::json!({"original": "x", "corrected": "y"})];
        let result = apply_corrections_to_transcript(&[], &corrections);
        assert!(result.is_empty());
    }

    #[test]
    fn test_apply_corrections_preserves_timing() {
        let segments = vec![seg(1.5, 3.0, "test")];
        let corrections = vec![serde_json::json!({"original": "test", "corrected": "best"})];
        let result = apply_corrections_to_transcript(&segments, &corrections);
        assert_eq!(result[0].start, 1.5);
        assert_eq!(result[0].end, 3.0);
    }

    #[test]
    fn test_apply_corrections_multiple_words() {
        let segments = vec![seg(0.0, 5.0, "Raknarok and Mesage")];
        let corrections = vec![
            serde_json::json!({"original": "Raknarok", "corrected": "Ragnarok"}),
            serde_json::json!({"original": "Mesage", "corrected": "Message"}),
        ];
        let result = apply_corrections_to_transcript(&segments, &corrections);
        assert_eq!(result[0].text, "Ragnarok and Message");
    }

    #[test]
    fn test_apply_corrections_skips_invalid_json() {
        let segments = vec![seg(0.0, 5.0, "Hello world")];
        let corrections = vec![
            serde_json::json!({"original": "x"}), // missing "corrected"
            serde_json::json!({"corrected": "y"}), // missing "original"
            serde_json::json!({"original": "Hello", "corrected": "Hi"}),
        ];
        let result = apply_corrections_to_transcript(&segments, &corrections);
        assert_eq!(result[0].text, "Hi world");
    }

    #[test]
    fn test_synthesis_result_serialization_roundtrip() {
        let result = SynthesisResult {
            summary: "Test".to_string(),
            key_corrections: vec![serde_json::json!({"original": "a", "corrected": "b"})],
            speaker_identification: vec![],
            visual_evidence: vec![],
            uncertainties: vec!["maybe".to_string()],
        };
        let json = serde_json::to_string(&result).unwrap();
        let parsed: SynthesisResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.summary, "Test");
        assert_eq!(parsed.uncertainties, vec!["maybe"]);
        assert_eq!(parsed.key_corrections.len(), 1);
    }
}
