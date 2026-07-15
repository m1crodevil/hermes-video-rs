use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::output::TranscriptSegment;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionFinding {
    pub moment_index: usize,
    pub timestamp: String,
    pub word: String,
    pub actual: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correction: Option<String>,
    pub confidence: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

#[derive(Debug, Clone)]
pub struct VisionMoment {
    pub index: usize,
    pub timestamp: String,
    pub frame_path: Option<String>,
    pub word: String,
    pub question: String,
    pub priority: u8,
}

const BATCH_VISION_PROMPT: &str = r#"You are analyzing multiple video frames to verify transcript accuracy.

For each frame below, examine the image and answer the verification question.

## Frames to Analyze

{frames}

## Response Format

Return a JSON array of findings, one per frame:
[
  {{
    "index": 0,
    "timestamp": "0:26",
    "word": "George andika erison",
    "actual": "What name is actually displayed on screen",
    "correction": "Corrected name if different, or null",
    "confidence": 0.95,
    "notes": "Any additional observations"
  }},
  ...
]

## Guidelines

- **Be precise.** Only state what you can actually see in the frame.
- **Check spelling.** Compare the transcript word with what's shown on screen.
- **Note corrections.** If the transcript has a misspelling, provide the correction.
- **Estimate confidence.** How confident are you in your reading? (0.0-1.0)
- **Add notes.** Any additional observations about the frame.

Return ONLY valid JSON array. No markdown, no explanation."#;

/// Generate a batch prompt for vision analysis of multiple frames.
///
/// Filters moments by priority, skips those without a frame_path, and
/// formats each frame entry into the BATCH_VISION_PROMPT template.
pub fn generate_batch_prompt(moments: &[VisionMoment], max_priority: u8) -> String {
    let frames_text: String = moments
        .iter()
        .filter(|m| m.priority <= max_priority)
        .filter(|m| m.frame_path.is_some())
        .map(|m| {
            format!(
                "### Frame {} (timestamp: {})\\n\\
                 Image: {}\\n\\
                 Word from transcript: \"{}\"\\n\\
                 Question: {}\\n",
                m.index,
                m.timestamp,
                m.frame_path.as_ref().unwrap(),
                m.word,
                m.question,
            )
        })
        .collect::<Vec<_>>()
        .join("\\n");

    BATCH_VISION_PROMPT.replace("{frames}", &frames_text)
}

/// Process batch results returned from the vision model into VisionFinding objects.
pub fn process_batch_results(results: Vec<serde_json::Value>) -> Vec<VisionFinding> {
    results
        .into_iter()
        .filter_map(|v| {
            let moment_index = v.get("index")?.as_u64()? as usize;
            let timestamp = v
                .get("timestamp")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string();
            let word = v
                .get("word")
                .and_then(|w| w.as_str())
                .unwrap_or("")
                .to_string();
            let actual = v
                .get("actual")
                .and_then(|a| a.as_str())
                .unwrap_or("")
                .to_string();
            let correction = v.get("correction").and_then(|c| {
                if c.is_null() {
                    None
                } else {
                    c.as_str().map(|s| s.to_string())
                }
            });
            let confidence = v.get("confidence").and_then(|c| c.as_f64()).unwrap_or(0.0);
            let notes = v
                .get("notes")
                .and_then(|n| n.as_str())
                .map(|s| s.to_string());

            Some(VisionFinding {
                moment_index,
                timestamp,
                word,
                actual,
                correction,
                confidence,
                notes,
            })
        })
        .collect()
}

/// Apply vision findings to a list of moments (serde_json::Value objects).
///
/// Sets `vision_result`, `correction`, and `verified` fields on each moment
/// based on matching findings.
pub fn apply_corrections_to_moments(
    moments: &[serde_json::Value],
    findings: &[VisionFinding],
) -> Vec<serde_json::Value> {
    // Index findings by moment_index
    let findings_map: HashMap<usize, &VisionFinding> =
        findings.iter().map(|f| (f.moment_index, f)).collect();

    moments
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let mut moment = m.clone();
            if let Some(finding) = findings_map.get(&i) {
                moment["vision_result"] =
                    serde_json::to_value(&finding.actual).unwrap_or(serde_json::Value::Null);
                moment["correction"] = serde_json::to_value(finding.correction.as_deref())
                    .unwrap_or(serde_json::Value::Null);
                moment["verified"] = serde_json::Value::Bool(true);
            }
            moment
        })
        .collect()
}

/// Extract corrections from findings: maps original word → corrected word.
///
/// Only includes entries where the correction differs from the word.
pub fn extract_corrections_for_transcript(findings: &[VisionFinding]) -> HashMap<String, String> {
    findings
        .iter()
        .filter_map(|f| {
            let correction = f.correction.as_deref()?;
            if correction != f.word && !correction.is_empty() {
                Some((f.word.clone(), correction.to_string()))
            } else {
                None
            }
        })
        .collect()
}

/// Strip punctuation from a word for matching purposes.
fn strip_punctuation(word: &str) -> String {
    word.chars()
        .filter(|c| c.is_alphanumeric() || *c == '\'' || *c == '-')
        .collect()
}

/// Apply word-level corrections to transcript segments.
///
/// For each segment, splits text into words and applies corrections found
/// from vision analysis. Preserves surrounding punctuation and case.
pub fn apply_corrections_to_transcript(
    segments: &[TranscriptSegment],
    corrections: &HashMap<String, String>,
) -> Vec<TranscriptSegment> {
    if corrections.is_empty() {
        return segments.to_vec();
    }

    segments
        .iter()
        .map(|seg| {
            let corrected_text = apply_word_corrections(&seg.text, corrections);
            TranscriptSegment {
                start: seg.start,
                end: seg.end,
                text: corrected_text,
                words: seg.words.clone(),
            }
        })
        .collect()
}

/// Apply word corrections to a single text string.
fn apply_word_corrections(text: &str, corrections: &HashMap<String, String>) -> String {
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut current_word = String::new();

    while let Some(&ch) = chars.peek() {
        if ch.is_alphanumeric() || ch == '\'' || ch == '-' {
            current_word.push(ch);
            chars.next();
        } else {
            // Flush the accumulated word
            if !current_word.is_empty() {
                let corrected = correct_word(&current_word, corrections);
                result.push_str(&corrected);
                current_word.clear();
            }
            result.push(ch);
            chars.next();
        }
    }

    // Flush any trailing word
    if !current_word.is_empty() {
        let corrected = correct_word(&current_word, corrections);
        result.push_str(&corrected);
    }

    result
}

/// Apply a correction to a single word, preserving case.
fn correct_word(word: &str, corrections: &HashMap<String, String>) -> String {
    let clean = strip_punctuation(word);

    // Find the matching correction (case-insensitive lookup)
    let correction = corrections
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(&clean));

    if let Some((_, corrected)) = correction {
        // Preserve case: if the original word started with uppercase,
        // capitalize the first letter of the correction
        if clean.chars().next().map_or(false, |c| c.is_uppercase()) {
            let mut chars = corrected.chars();
            if let Some(first) = chars.next() {
                let mut fixed: String = first.to_uppercase().collect();
                fixed.extend(chars);
                return fixed;
            }
        }
        corrected.clone()
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
    fn test_process_batch_results_valid() {
        let json = serde_json::json!([
            {
                "index": 0,
                "timestamp": "0:10",
                "word": "hello",
                "actual": "Hello World",
                "correction": "Hello World",
                "confidence": 0.95,
                "notes": "clear text"
            },
            {
                "index": 1,
                "timestamp": "0:20",
                "word": "frend",
                "actual": "Friend",
                "correction": "Friend",
                "confidence": 0.88,
                "notes": null
            }
        ]);
        let findings = process_batch_results(serde_json::from_value(json).unwrap());
        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0].moment_index, 0);
        assert_eq!(findings[0].word, "hello");
        assert_eq!(findings[0].confidence, 0.95);
        assert_eq!(findings[1].correction, Some("Friend".to_string()));
    }

    #[test]
    fn test_process_batch_results_null_correction() {
        let json = serde_json::json!([
            {
                "index": 0,
                "timestamp": "0:05",
                "word": "test",
                "actual": "test",
                "correction": null,
                "confidence": 1.0,
                "notes": null
            }
        ]);
        let findings = process_batch_results(serde_json::from_value(json).unwrap());
        assert_eq!(findings[0].correction, None);
    }

    #[test]
    fn test_extract_corrections_for_transcript() {
        let findings = vec![
            VisionFinding {
                moment_index: 0,
                timestamp: "0:10".into(),
                word: "frend".into(),
                actual: "Friend".into(),
                correction: Some("Friend".into()),
                confidence: 0.9,
                notes: None,
            },
            VisionFinding {
                moment_index: 1,
                timestamp: "0:20".into(),
                word: "hello".into(),
                actual: "hello".into(),
                correction: None,
                confidence: 1.0,
                notes: None,
            },
            VisionFinding {
                moment_index: 2,
                timestamp: "0:30".into(),
                word: "recieve".into(),
                actual: "receive".into(),
                correction: Some("receive".into()),
                confidence: 0.85,
                notes: None,
            },
        ];
        let corrections = extract_corrections_for_transcript(&findings);
        assert_eq!(corrections.len(), 2);
        assert_eq!(corrections.get("frend").unwrap(), "Friend");
        assert_eq!(corrections.get("recieve").unwrap(), "receive");
        assert!(corrections.get("hello").is_none());
    }

    #[test]
    fn test_extract_corrections_same_word() {
        // Corrections that are same as word should be excluded
        let findings = vec![VisionFinding {
            moment_index: 0,
            timestamp: "0:05".into(),
            word: "hello".into(),
            actual: "hello".into(),
            correction: Some("hello".into()),
            confidence: 1.0,
            notes: None,
        }];
        let corrections = extract_corrections_for_transcript(&findings);
        assert!(corrections.is_empty());
    }

    #[test]
    fn test_apply_corrections_to_transcript_basic() {
        let segments = vec![seg(0.0, 1.0, "The frend arrived")];
        let mut corrections = HashMap::new();
        corrections.insert("frend".to_string(), "friend".to_string());
        let result = apply_corrections_to_transcript(&segments, &corrections);
        assert_eq!(result[0].text, "The friend arrived");
    }

    #[test]
    fn test_apply_corrections_preserves_case() {
        let segments = vec![seg(0.0, 1.0, "Frend is here")];
        let mut corrections = HashMap::new();
        corrections.insert("frend".to_string(), "friend".to_string());
        let result = apply_corrections_to_transcript(&segments, &corrections);
        assert_eq!(result[0].text, "Friend is here");
    }

    #[test]
    fn test_apply_corrections_preserves_punctuation() {
        let segments = vec![seg(0.0, 1.0, "Hello, frend! How are you?")];
        let mut corrections = HashMap::new();
        corrections.insert("frend".to_string(), "friend".to_string());
        let result = apply_corrections_to_transcript(&segments, &corrections);
        assert_eq!(result[0].text, "Hello, friend! How are you?");
    }

    #[test]
    fn test_apply_corrections_no_corrections() {
        let segments = vec![seg(0.0, 1.0, "No changes here")];
        let corrections = HashMap::new();
        let result = apply_corrections_to_transcript(&segments, &corrections);
        assert_eq!(result[0].text, "No changes here");
    }

    #[test]
    fn test_apply_corrections_multiple_words() {
        let segments = vec![seg(0.0, 1.0, "Frend recieve the mesage")];
        let mut corrections = HashMap::new();
        corrections.insert("frend".to_string(), "friend".to_string());
        corrections.insert("recieve".to_string(), "receive".to_string());
        corrections.insert("mesage".to_string(), "message".to_string());
        let result = apply_corrections_to_transcript(&segments, &corrections);
        assert_eq!(result[0].text, "Friend receive the message");
    }

    #[test]
    fn test_apply_corrections_preserves_timing() {
        let segments = vec![seg(1.5, 3.0, "frend")];
        let mut corrections = HashMap::new();
        corrections.insert("frend".to_string(), "friend".to_string());
        let result = apply_corrections_to_transcript(&segments, &corrections);
        assert_eq!(result[0].start, 1.5);
        assert_eq!(result[0].end, 3.0);
    }

    #[test]
    fn test_generate_batch_prompt_filters_priority() {
        let moments = vec![
            VisionMoment {
                index: 0,
                timestamp: "0:10".into(),
                frame_path: Some("/tmp/frame0.jpg".into()),
                word: "hello".into(),
                question: "Is this correct?".into(),
                priority: 1,
            },
            VisionMoment {
                index: 1,
                timestamp: "0:20".into(),
                frame_path: Some("/tmp/frame1.jpg".into()),
                word: "world".into(),
                question: "Is this correct?".into(),
                priority: 5,
            },
        ];
        let prompt = generate_batch_prompt(&moments, 3);
        assert!(prompt.contains("hello"));
        assert!(!prompt.contains("world"));
    }

    #[test]
    fn test_generate_batch_prompt_filters_no_frame() {
        let moments = vec![
            VisionMoment {
                index: 0,
                timestamp: "0:10".into(),
                frame_path: Some("/tmp/frame0.jpg".into()),
                word: "hello".into(),
                question: "Verify?".into(),
                priority: 1,
            },
            VisionMoment {
                index: 1,
                timestamp: "0:20".into(),
                frame_path: None,
                word: "missing".into(),
                question: "Verify?".into(),
                priority: 1,
            },
        ];
        let prompt = generate_batch_prompt(&moments, 5);
        assert!(prompt.contains("hello"));
        assert!(!prompt.contains("missing"));
    }

    #[test]
    fn test_generate_batch_prompt_empty() {
        let moments: Vec<VisionMoment> = vec![];
        let prompt = generate_batch_prompt(&moments, 5);
        assert!(prompt.contains("Frames to Analyze"));
    }

    #[test]
    fn test_apply_corrections_to_moments() {
        let moments = vec![
            serde_json::json!({"word": "frend"}),
            serde_json::json!({"word": "hello"}),
        ];
        let findings = vec![VisionFinding {
            moment_index: 0,
            timestamp: "0:10".into(),
            word: "frend".into(),
            actual: "Friend".into(),
            correction: Some("friend".into()),
            confidence: 0.9,
            notes: Some("clear".into()),
        }];
        let result = apply_corrections_to_moments(&moments, &findings);
        assert_eq!(result[0]["verified"], true);
        assert_eq!(result[0]["correction"], "friend");
        assert_eq!(result[0]["vision_result"], "Friend");
        assert!(result[1].get("verified").is_none());
    }
}
