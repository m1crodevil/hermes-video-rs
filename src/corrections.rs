//! Transcript correction application.
//!
//! Extracts word-level corrections from verified moments and applies them
//! to transcript segments while preserving punctuation, case, and timing.

use crate::output::TranscriptSegment;
use crate::timestamp::format_time;
use std::collections::HashMap;

/// A verified moment from vision analysis that may contain a correction.
#[derive(Debug, Clone)]
pub struct VerifiedMoment {
    pub timestamp: f64,
    pub word: String,
    pub correction: Option<String>,
}

/// A single correction applied to a transcript segment.
#[derive(Debug, Clone)]
pub struct CorrectionChange {
    pub segment_start: f64,
    pub original: String,
    pub corrected: String,
}

/// Strip punctuation characters from a word for matching purposes.
fn strip_punctuation(word: &str) -> String {
    word.chars()
        .filter(|c| c.is_alphanumeric() || *c == '\'' || *c == '-')
        .collect()
}

/// Extract corrections from verified moments.
///
/// Returns a HashMap mapping original words (lowercase) to their corrected forms.
/// Only includes moments where a correction was provided and differs from the original.
pub fn extract_corrections_from_moments(moments: &[VerifiedMoment]) -> HashMap<String, String> {
    let mut corrections = HashMap::new();

    for moment in moments {
        if let Some(ref correction) = moment.correction {
            let clean = strip_punctuation(&moment.word);
            if !clean.is_empty() && !correction.is_empty() && clean != *correction {
                // Use lowercase for case-insensitive matching
                corrections.insert(clean.to_lowercase(), correction.clone());
            }
        }
    }

    corrections
}

/// Apply corrections to transcript segments, tracking changes made.
///
/// Returns a tuple of (corrected segments, list of changes).
/// Preserves surrounding punctuation and letter case from the original text.
pub fn apply_corrections_to_segments(
    segments: &[TranscriptSegment],
    corrections: &HashMap<String, String>,
) -> (Vec<TranscriptSegment>, Vec<CorrectionChange>) {
    if corrections.is_empty() {
        return (segments.to_vec(), Vec::new());
    }

    let mut corrected_segments = Vec::with_capacity(segments.len());
    let mut changes = Vec::new();

    for seg in segments {
        let (corrected_text, seg_changes) =
            apply_word_corrections_with_changes(seg.start, &seg.text, corrections);

        corrected_segments.push(TranscriptSegment {
            start: seg.start,
            end: seg.end,
            text: corrected_text,
            words: seg.words.clone(),
        });

        changes.extend(seg_changes);
    }

    (corrected_segments, changes)
}

/// Apply word corrections to a text string, returning both the corrected text
/// and a list of changes made.
fn apply_word_corrections_with_changes(
    segment_start: f64,
    text: &str,
    corrections: &HashMap<String, String>,
) -> (String, Vec<CorrectionChange>) {
    let mut result = String::with_capacity(text.len());
    let mut changes = Vec::new();
    let mut current_word = String::new();

    for ch in text.chars() {
        if ch.is_alphanumeric() || ch == '\'' || ch == '-' {
            current_word.push(ch);
        } else {
            // Flush the accumulated word
            if !current_word.is_empty() {
                let (corrected, change) =
                    correct_word_tracked(segment_start, &current_word, corrections);
                result.push_str(&corrected);
                if let Some(c) = change {
                    changes.push(c);
                }
                current_word.clear();
            }
            result.push(ch);
        }
    }

    // Flush any trailing word
    if !current_word.is_empty() {
        let (corrected, change) = correct_word_tracked(segment_start, &current_word, corrections);
        result.push_str(&corrected);
        if let Some(c) = change {
            changes.push(c);
        }
    }

    (result, changes)
}

/// Apply a correction to a single word, preserving case, and track the change.
fn correct_word_tracked(
    segment_start: f64,
    word: &str,
    corrections: &HashMap<String, String>,
) -> (String, Option<CorrectionChange>) {
    let clean = strip_punctuation(word);

    // Find the matching correction (case-insensitive lookup)
    let correction = corrections
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(&clean));

    if let Some((_, corrected)) = correction {
        // Preserve case: if the original word started with uppercase,
        // capitalize the first letter of the correction
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

        let change = CorrectionChange {
            segment_start,
            original: word.to_string(),
            corrected: fixed.clone(),
        };

        (fixed, Some(change))
    } else {
        (word.to_string(), None)
    }
}

/// Generate corrected transcript text with timestamps.
///
/// Format: `[MM:SS] text` per segment.
pub fn generate_corrected_transcript_text(segments: &[TranscriptSegment]) -> String {
    segments
        .iter()
        .map(|seg| format!("[{}] {}", format_time(seg.start), seg.text))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Generate a human-readable diff summary of applied corrections.
///
/// Format: "Applied N corrections:\n  [MM:SS] original → corrected"
pub fn generate_diff(changes: &[CorrectionChange]) -> String {
    if changes.is_empty() {
        return "No corrections applied.".to_string();
    }

    let mut output = format!(
        "Applied {} correction{}:\n",
        changes.len(),
        if changes.len() == 1 { "" } else { "s" }
    );

    for change in changes {
        output.push_str(&format!(
            "  [{}] {} → {}\n",
            format_time(change.segment_start),
            change.original,
            change.corrected,
        ));
    }

    output
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

    fn moment(timestamp: f64, word: &str, correction: Option<&str>) -> VerifiedMoment {
        VerifiedMoment {
            timestamp,
            word: word.to_string(),
            correction: correction.map(|s| s.to_string()),
        }
    }

    #[test]
    fn test_extract_corrections_basic() {
        let moments = vec![
            moment(10.0, "frend", Some("friend")),
            moment(20.0, "hello", None),
            moment(30.0, "recieve", Some("receive")),
        ];
        let corrections = extract_corrections_from_moments(&moments);
        assert_eq!(corrections.len(), 2);
        assert_eq!(corrections.get("frend").unwrap(), "friend");
        assert_eq!(corrections.get("recieve").unwrap(), "receive");
        assert!(corrections.get("hello").is_none());
    }

    #[test]
    fn test_extract_corrections_same_word_excluded() {
        let moments = vec![moment(5.0, "hello", Some("hello"))];
        let corrections = extract_corrections_from_moments(&moments);
        assert!(corrections.is_empty());
    }

    #[test]
    fn test_extract_corrections_empty_correction_excluded() {
        let moments = vec![moment(5.0, "word", Some(""))];
        let corrections = extract_corrections_from_moments(&moments);
        assert!(corrections.is_empty());
    }

    #[test]
    fn test_apply_corrections_basic() {
        let segments = vec![seg(0.0, 5.0, "The frend arrived")];
        let mut corrections = HashMap::new();
        corrections.insert("frend".to_string(), "friend".to_string());
        let (result, changes) = apply_corrections_to_segments(&segments, &corrections);
        assert_eq!(result[0].text, "The friend arrived");
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].original, "frend");
        assert_eq!(changes[0].corrected, "friend");
    }

    #[test]
    fn test_apply_corrections_preserves_case() {
        let segments = vec![seg(0.0, 5.0, "Frend is here")];
        let mut corrections = HashMap::new();
        corrections.insert("frend".to_string(), "friend".to_string());
        let (result, _) = apply_corrections_to_segments(&segments, &corrections);
        assert_eq!(result[0].text, "Friend is here");
    }

    #[test]
    fn test_apply_corrections_preserves_punctuation() {
        let segments = vec![seg(0.0, 5.0, "Hello, frend! How are you?")];
        let mut corrections = HashMap::new();
        corrections.insert("frend".to_string(), "friend".to_string());
        let (result, _) = apply_corrections_to_segments(&segments, &corrections);
        assert_eq!(result[0].text, "Hello, friend! How are you?");
    }

    #[test]
    fn test_apply_corrections_empty_map() {
        let segments = vec![seg(0.0, 5.0, "No changes here")];
        let corrections = HashMap::new();
        let (result, changes) = apply_corrections_to_segments(&segments, &corrections);
        assert_eq!(result[0].text, "No changes here");
        assert!(changes.is_empty());
    }

    #[test]
    fn test_apply_corrections_multiple_words() {
        let segments = vec![seg(0.0, 5.0, "Frend recieve the mesage")];
        let mut corrections = HashMap::new();
        corrections.insert("frend".to_string(), "friend".to_string());
        corrections.insert("recieve".to_string(), "receive".to_string());
        corrections.insert("mesage".to_string(), "message".to_string());
        let (result, changes) = apply_corrections_to_segments(&segments, &corrections);
        assert_eq!(result[0].text, "Friend receive the message");
        assert_eq!(changes.len(), 3);
    }

    #[test]
    fn test_apply_corrections_preserves_timing() {
        let segments = vec![seg(1.5, 3.0, "frend")];
        let mut corrections = HashMap::new();
        corrections.insert("frend".to_string(), "friend".to_string());
        let (result, _) = apply_corrections_to_segments(&segments, &corrections);
        assert_eq!(result[0].start, 1.5);
        assert_eq!(result[0].end, 3.0);
    }

    #[test]
    fn test_generate_corrected_transcript_text() {
        let segments = vec![seg(0.0, 5.0, "Hello world"), seg(65.0, 70.0, "Goodbye")];
        let text = generate_corrected_transcript_text(&segments);
        assert_eq!(text, "[00:00] Hello world\n[01:05] Goodbye");
    }

    #[test]
    fn test_generate_diff_with_changes() {
        let changes = vec![
            CorrectionChange {
                segment_start: 10.0,
                original: "frend".to_string(),
                corrected: "friend".to_string(),
            },
            CorrectionChange {
                segment_start: 30.0,
                original: "recieve".to_string(),
                corrected: "receive".to_string(),
            },
        ];
        let diff = generate_diff(&changes);
        assert!(diff.contains("Applied 2 corrections"));
        assert!(diff.contains("[00:10] frend → friend"));
        assert!(diff.contains("[00:30] recieve → receive"));
    }

    #[test]
    fn test_generate_diff_no_changes() {
        let diff = generate_diff(&[]);
        assert_eq!(diff, "No corrections applied.");
    }

    #[test]
    fn test_generate_diff_single_change() {
        let changes = vec![CorrectionChange {
            segment_start: 5.0,
            original: "test".to_string(),
            corrected: "best".to_string(),
        }];
        let diff = generate_diff(&changes);
        assert!(diff.contains("Applied 1 correction"));
        assert!(!diff.contains("corrections"));
    }

    #[test]
    fn test_apply_corrections_empty_segments() {
        let segments: Vec<TranscriptSegment> = vec![];
        let mut corrections = HashMap::new();
        corrections.insert("test".to_string(), "best".to_string());
        let (result, changes) = apply_corrections_to_segments(&segments, &corrections);
        assert!(result.is_empty());
        assert!(changes.is_empty());
    }
}
