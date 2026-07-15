use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use crate::output::TranscriptSegment;

// ── Data Models ──────────────────────────────────────────────────────────

/// A request for vision analysis on a frame (single-moment mode).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionRequest {
    pub moment_index: usize,
    pub timestamp: f64,
    pub timestamp_fmt: String,
    pub frame_path: String,
    pub word: String,
    pub question: String,
    pub reason: String,
    pub priority: u32,
}

/// Result from single-moment vision analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionResult {
    pub moment_index: usize,
    pub timestamp: f64,
    pub frame_path: String,
    pub raw_answer: String,
    pub correction: Option<String>,
    pub corrected_word: Option<String>,
    pub confidence: Option<String>,
}

/// A moment with verification results (single-moment mode).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifiedMoment {
    pub timestamp: f64,
    pub timestamp_fmt: String,
    pub word: String,
    pub context: String,
    pub reason: String,
    pub question: String,
    pub priority: u32,
    pub vision_result: Option<VisionResult>,
    pub correction: Option<String>,
    pub verified: bool,
}

/// A finding from batch vision analysis.
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

/// A moment prepared for batch vision analysis.
#[derive(Debug, Clone)]
pub struct VisionMoment {
    pub index: usize,
    pub timestamp: String,
    pub frame_path: Option<String>,
    pub word: String,
    pub question: String,
    pub priority: u8,
}

// ── Frame Matching ───────────────────────────────────────────────────────

/// Find the closest frame file to the given timestamp within a tolerance.
pub fn find_frame_at_timestamp(
    timestamp: f64,
    frames_dir: &Path,
    tolerance: f64,
) -> Option<String> {
    let mut best_match: Option<String> = None;
    let mut best_diff = f64::INFINITY;

    // Try report.json metadata first — most accurate
    let report_path = frames_dir.parent()?.join("report.json");
    if report_path.exists() {
        if let Ok(report) = std::fs::read_to_string(&report_path) {
            if let Ok(report_val) = serde_json::from_str::<serde_json::Value>(&report) {
                if let Some(frames) = report_val["frames"].as_array() {
                    for frame in frames {
                        if let (Some(frame_ts), Some(frame_path)) = (
                            frame.get("timestamp").and_then(|v| v.as_f64()),
                            frame.get("path").and_then(|v| v.as_str()),
                        ) {
                            let diff = (frame_ts - timestamp).abs();
                            if diff < best_diff && diff <= tolerance {
                                best_diff = diff;
                                best_match = Some(frame_path.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    // Fallback: scan frame files directly
    if best_match.is_none() {
        for pattern in &["frame_*.jpg", "cue_*.jpg", "fill_*.jpg"] {
            if let Ok(entries) = std::fs::read_dir(frames_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let name = path.file_name()?.to_string_lossy();
                    let matches = match *pattern {
                        "frame_*.jpg" => name.starts_with("frame_") && name.ends_with(".jpg"),
                        "cue_*.jpg" => name.starts_with("cue_") && name.ends_with(".jpg"),
                        "fill_*.jpg" => name.starts_with("fill_") && name.ends_with(".jpg"),
                        _ => false,
                    };
                    if !matches {
                        continue;
                    }

                    if let Some(idx_str) = name
                        .strip_prefix("frame_")
                        .or_else(|| name.strip_prefix("cue_"))
                        .or_else(|| name.strip_prefix("fill_"))
                        .and_then(|s| s.strip_suffix(".jpg"))
                    {
                        if let Ok(_idx) = idx_str.parse::<usize>() {
                            if best_match.is_none() {
                                best_match = Some(path.to_string_lossy().to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    best_match
}

// ── Frames Needed ────────────────────────────────────────────────────────

/// List frames needed for verification, checking which already exist.
pub fn list_frames_needed(
    moments: &[serde_json::Value],
    frames_dir: &Path,
) -> Vec<serde_json::Value> {
    let mut result = Vec::new();

    for (i, moment) in moments.iter().enumerate() {
        let timestamp = moment
            .get("timestamp")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        let frame_path = find_frame_at_timestamp(timestamp, frames_dir, 2.0);

        let entry = serde_json::json!({
            "moment_index": i,
            "timestamp": timestamp,
            "timestamp_fmt": moment.get("timestamp_fmt").and_then(|v| v.as_str()).unwrap_or(""),
            "word": moment.get("word").and_then(|v| v.as_str()).unwrap_or(""),
            "question": moment.get("question").and_then(|v| v.as_str()).unwrap_or(""),
            "frame_path": frame_path,
            "frame_exists": frame_path.is_some(),
            "priority": moment.get("priority").and_then(|v| v.as_u64()).unwrap_or(3) as u32,
        });

        result.push(entry);
    }

    result
}

// ── Single-Moment Vision Questions ──────────────────────────────────────

/// Generate vision analysis requests for each moment (single-moment mode).
pub fn generate_vision_questions(moments: &[serde_json::Value]) -> Vec<VisionRequest> {
    let mut requests: Vec<VisionRequest> = Vec::new();

    for (i, moment) in moments.iter().enumerate() {
        if moment.get("vision_result").is_some() {
            continue;
        }

        let timestamp = moment
            .get("timestamp")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        requests.push(VisionRequest {
            moment_index: i,
            timestamp,
            timestamp_fmt: moment
                .get("timestamp_fmt")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            frame_path: moment
                .get("frame_path")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            word: moment
                .get("word")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            question: moment
                .get("question")
                .and_then(|v| v.as_str())
                .unwrap_or("What is shown in this frame?")
                .to_string(),
            reason: moment
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string(),
            priority: moment.get("priority").and_then(|v| v.as_u64()).unwrap_or(3) as u32,
        });
    }

    requests.sort_by(|a, b| a.priority.cmp(&b.priority));
    requests
}

// ── Single-Moment Result Processing ─────────────────────────────────────

/// Process vision results and merge them into moments (single-moment mode).
pub fn process_vision_results(
    moments: &[serde_json::Value],
    results: &[serde_json::Value],
) -> Vec<VerifiedMoment> {
    let mut results_by_index: HashMap<usize, &serde_json::Value> = HashMap::new();
    for r in results {
        if let Some(idx) = r.get("moment_index").and_then(|v| v.as_u64()) {
            results_by_index.insert(idx as usize, r);
        }
    }

    let mut verified = Vec::new();

    for (i, moment) in moments.iter().enumerate() {
        let timestamp = moment
            .get("timestamp")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let timestamp_fmt = moment
            .get("timestamp_fmt")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let word = moment
            .get("word")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let context = moment
            .get("context")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let reason = moment
            .get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let question = moment
            .get("question")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let priority = moment.get("priority").and_then(|v| v.as_u64()).unwrap_or(3) as u32;

        if let Some(result_val) = results_by_index.get(&i) {
            let vision_result = Some(VisionResult {
                moment_index: i,
                timestamp,
                frame_path: result_val
                    .get("frame_path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                raw_answer: result_val
                    .get("raw_answer")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                correction: result_val
                    .get("correction")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                corrected_word: result_val
                    .get("corrected_word")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                confidence: result_val.get("confidence").and_then(|v| match v {
                    serde_json::Value::Number(n) => Some(n.to_string()),
                    serde_json::Value::String(s) => Some(s.clone()),
                    _ => None,
                }),
            });

            let correction = vision_result.as_ref().and_then(|vr| vr.correction.clone());

            verified.push(VerifiedMoment {
                timestamp,
                timestamp_fmt,
                word,
                context,
                reason,
                question,
                priority,
                vision_result,
                correction,
                verified: true,
            });
        } else {
            verified.push(VerifiedMoment {
                timestamp,
                timestamp_fmt,
                word,
                context,
                reason,
                question,
                priority,
                vision_result: None,
                correction: None,
                verified: false,
            });
        }
    }

    verified
}

// ── Batch Vision ─────────────────────────────────────────────────────────

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
pub fn generate_batch_prompt(moments: &[VisionMoment], max_priority: u8) -> String {
    let frames_text: String = moments
        .iter()
        .filter(|m| m.priority <= max_priority)
        .filter(|m| m.frame_path.is_some())
        .map(|m| {
            format!(
                "### Frame {} (timestamp: {})\\n\\n  Image: {}\\n\\n  Word from transcript: \"{}\"\\n\\n  Question: {}\\n",
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
pub fn apply_corrections_to_moments(
    moments: &[serde_json::Value],
    findings: &[VisionFinding],
) -> Vec<serde_json::Value> {
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

// ── Correction Extraction (unified) ─────────────────────────────────────

/// Extract word corrections from verified moments (single-moment mode).
pub fn extract_corrections(verified: &[VerifiedMoment]) -> HashMap<String, String> {
    verified
        .iter()
        .filter_map(|v| {
            let correction = v.correction.as_deref()?;
            if correction != v.word && !correction.is_empty() {
                Some((v.word.clone(), correction.to_string()))
            } else {
                None
            }
        })
        .collect()
}

/// Extract corrections from batch findings: maps original word → corrected word.
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

// ── Transcript Corrections ──────────────────────────────────────────────

/// Strip punctuation from a word for matching purposes.
fn strip_punctuation(word: &str) -> String {
    word.chars()
        .filter(|c| c.is_alphanumeric() || *c == '\'' || *c == '-')
        .collect()
}

/// Apply a correction to a single word, preserving case.
fn correct_word(word: &str, corrections: &HashMap<String, String>) -> String {
    let clean = strip_punctuation(word);

    let correction = corrections
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(&clean));

    if let Some((_, corrected)) = correction {
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
            if !current_word.is_empty() {
                let corrected = correct_word(&current_word, corrections);
                result.push_str(&corrected);
                current_word.clear();
            }
            result.push(ch);
            chars.next();
        }
    }

    if !current_word.is_empty() {
        let corrected = correct_word(&current_word, corrections);
        result.push_str(&corrected);
    }

    result
}

/// Apply word-level corrections to transcript segments.
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

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_moment_json(
        timestamp: f64,
        word: &str,
        reason: &str,
        priority: u32,
    ) -> serde_json::Value {
        serde_json::json!({
            "timestamp": timestamp,
            "timestamp_fmt": crate::moments::format_timestamp(timestamp),
            "word": word,
            "context": format!("Context for {}", word),
            "reason": reason,
            "question": format!("Is {} correct?", word),
            "priority": priority,
        })
    }

    fn seg(start: f64, end: f64, text: &str) -> TranscriptSegment {
        TranscriptSegment {
            start,
            end,
            text: text.to_string(),
            words: None,
        }
    }

    // ── Single-moment tests ─────────────────────────────────────────

    #[test]
    fn test_generate_vision_questions_basic() {
        let moments = vec![
            make_moment_json(10.0, "Raknarok", "proper_noun", 1),
            make_moment_json(30.0, "hello", "deictic", 3),
        ];
        let requests = generate_vision_questions(&moments);
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].word, "Raknarok");
        assert_eq!(requests[0].priority, 1);
        assert_eq!(requests[1].word, "hello");
        assert_eq!(requests[1].priority, 3);
    }

    #[test]
    fn test_generate_vision_questions_skips_existing() {
        let moments = vec![
            make_moment_json(10.0, "already_done", "claim", 1),
            make_moment_json(30.0, "needs_check", "proper_noun", 2),
        ];
        let mut moments_with_result = moments.clone();
        moments_with_result[0]["vision_result"] = serde_json::json!("already verified");

        let requests = generate_vision_questions(&moments_with_result);
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].word, "needs_check");
    }

    #[test]
    fn test_generate_vision_questions_empty() {
        let moments: Vec<serde_json::Value> = vec![];
        let requests = generate_vision_questions(&moments);
        assert!(requests.is_empty());
    }

    #[test]
    fn test_generate_vision_questions_default_question() {
        let moments = vec![serde_json::json!({
            "timestamp": 5.0,
            "word": "test",
            "reason": "claim",
            "priority": 2,
        })];
        let requests = generate_vision_questions(&moments);
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].question, "What is shown in this frame?");
    }

    #[test]
    fn test_process_vision_results_with_match() {
        let moments = vec![
            make_moment_json(10.0, "Raknarok", "proper_noun", 1),
            make_moment_json(30.0, "hello", "deictic", 3),
        ];
        let results = vec![serde_json::json!({
            "moment_index": 0,
            "raw_answer": "Ragnarok is displayed on screen",
            "correction": "Ragnarok",
            "confidence": 0.95,
        })];

        let verified = process_vision_results(&moments, &results);
        assert_eq!(verified.len(), 2);
        assert!(verified[0].verified);
        assert!(verified[0].vision_result.is_some());
        assert_eq!(verified[0].correction, Some("Ragnarok".to_string()));
        assert!(!verified[1].verified);
        assert!(verified[1].vision_result.is_none());
    }

    #[test]
    fn test_process_vision_results_no_results() {
        let moments = vec![make_moment_json(10.0, "test", "claim", 2)];
        let results: Vec<serde_json::Value> = vec![];

        let verified = process_vision_results(&moments, &results);
        assert_eq!(verified.len(), 1);
        assert!(!verified[0].verified);
        assert!(verified[0].vision_result.is_none());
    }

    #[test]
    fn test_extract_corrections_basic() {
        let verified = vec![
            VerifiedMoment {
                timestamp: 10.0,
                timestamp_fmt: "0:10".into(),
                word: "Raknarok".into(),
                context: "Playing Raknarok".into(),
                reason: "proper_noun".into(),
                question: "What game?".into(),
                priority: 1,
                vision_result: None,
                correction: Some("Ragnarok".into()),
                verified: true,
            },
            VerifiedMoment {
                timestamp: 30.0,
                timestamp_fmt: "0:30".into(),
                word: "hello".into(),
                context: "greeting".into(),
                reason: "deictic".into(),
                question: "What?".into(),
                priority: 3,
                vision_result: None,
                correction: None,
                verified: true,
            },
            VerifiedMoment {
                timestamp: 60.0,
                timestamp_fmt: "1:00".into(),
                word: "frend".into(),
                context: "my friend".into(),
                reason: "proper_noun".into(),
                question: "Who?".into(),
                priority: 2,
                vision_result: None,
                correction: Some("friend".into()),
                verified: true,
            },
        ];

        let corrections = extract_corrections(&verified);
        assert_eq!(corrections.len(), 2);
        assert_eq!(corrections.get("Raknarok").unwrap(), "Ragnarok");
        assert_eq!(corrections.get("frend").unwrap(), "friend");
        assert!(corrections.get("hello").is_none());
    }

    #[test]
    fn test_extract_corrections_same_word_excluded() {
        let verified = vec![VerifiedMoment {
            timestamp: 10.0,
            timestamp_fmt: "0:10".into(),
            word: "hello".into(),
            context: "".into(),
            reason: "deictic".into(),
            question: "?".into(),
            priority: 3,
            vision_result: None,
            correction: Some("hello".into()),
            verified: true,
        }];

        let corrections = extract_corrections(&verified);
        assert!(corrections.is_empty());
    }

    #[test]
    fn test_list_frames_needed_basic() {
        let moments = vec![
            make_moment_json(10.0, "Raknarok", "proper_noun", 1),
            make_moment_json(30.0, "hello", "deictic", 3),
        ];

        let tmp = tempfile::tempdir().unwrap();
        let frames_dir = tmp.path().join("frames");
        std::fs::create_dir_all(&frames_dir).unwrap();

        let result = list_frames_needed(&moments, &frames_dir);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0]["moment_index"], 0);
        assert_eq!(result[0]["word"], "Raknarok");
        assert_eq!(result[0]["frame_exists"], false);
        assert_eq!(result[1]["moment_index"], 1);
    }

    #[test]
    fn test_list_frames_needed_empty() {
        let moments: Vec<serde_json::Value> = vec![];
        let tmp = tempfile::tempdir().unwrap();
        let result = list_frames_needed(&moments, tmp.path());
        assert!(result.is_empty());
    }

    #[test]
    fn test_find_frame_at_timestamp_no_frames_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let result = find_frame_at_timestamp(10.0, tmp.path(), 2.0);
        assert!(result.is_none());
    }

    // ── Batch tests ─────────────────────────────────────────────────

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

    // ── Transcript correction tests ─────────────────────────────────

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
}
