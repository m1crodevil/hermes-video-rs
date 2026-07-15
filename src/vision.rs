use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

// ── Data Models ──────────────────────────────────────────────────────────

/// A request for vision analysis on a frame.
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

/// Result from vision analysis.
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

/// A moment with verification results.
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

// ── Frame Matching ───────────────────────────────────────────────────────

/// Find the closest frame file to the given timestamp within a tolerance.
///
/// Searches the frames directory for JPEG files (`frame_*.jpg`, `cue_*.jpg`,
/// `fill_*.jpg`) and tries to match by reading the parent `report.json` if
/// available, falling back to filename-index heuristics.
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

                    // Try to extract index from filename for rough heuristic
                    if let Some(idx_str) = name
                        .strip_prefix("frame_")
                        .or_else(|| name.strip_prefix("cue_"))
                        .or_else(|| name.strip_prefix("fill_"))
                        .and_then(|s| s.strip_suffix(".jpg"))
                    {
                        if let Ok(_idx) = idx_str.parse::<usize>() {
                            // Without fps metadata, we can't precisely map index→timestamp.
                            // Store as candidate with unknown timestamp.
                            // Only use if nothing better found.
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
///
/// Each element is a JSON object with `moment_index`, `timestamp`, `frame_path`,
/// `frame_exists`, and other metadata fields.
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

// ── Vision Questions ─────────────────────────────────────────────────────

/// Generate vision analysis requests for each moment.
///
/// Skips moments that already have a `vision_result`. Returns requests
/// sorted by priority (ascending = most critical first).
pub fn generate_vision_questions(moments: &[serde_json::Value]) -> Vec<VisionRequest> {
    let mut requests: Vec<VisionRequest> = Vec::new();

    for (i, moment) in moments.iter().enumerate() {
        // Skip if vision_result already exists
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

    // Sort by priority (ascending = most critical first)
    requests.sort_by(|a, b| a.priority.cmp(&b.priority));

    requests
}

// ── Result Processing ────────────────────────────────────────────────────

/// Process vision results and merge them into moments.
///
/// For each moment, looks up a matching result by `moment_index`. If found,
/// populates the `vision_result` and `correction` fields and marks `verified`.
pub fn process_vision_results(
    moments: &[serde_json::Value],
    results: &[serde_json::Value],
) -> Vec<VerifiedMoment> {
    // Index results by moment_index
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

// ── Correction Extraction ────────────────────────────────────────────────

/// Extract word corrections from verified moments.
///
/// Returns a map of `original_word → corrected_word` for all verified moments
/// that have a correction differing from the original word.
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

    #[test]
    fn test_generate_vision_questions_basic() {
        let moments = vec![
            make_moment_json(10.0, "Raknarok", "proper_noun", 1),
            make_moment_json(30.0, "hello", "deictic", 3),
        ];
        let requests = generate_vision_questions(&moments);
        assert_eq!(requests.len(), 2);
        // Sorted by priority — critical (1) first
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
        // First moment should be verified
        assert!(verified[0].verified);
        assert!(verified[0].vision_result.is_some());
        assert_eq!(verified[0].correction, Some("Ragnarok".to_string()));
        // Second moment should NOT be verified
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
    fn test_extract_corrections_empty_correction_excluded() {
        let verified = vec![VerifiedMoment {
            timestamp: 10.0,
            timestamp_fmt: "0:10".into(),
            word: "hello".into(),
            context: "".into(),
            reason: "deictic".into(),
            question: "?".into(),
            priority: 3,
            vision_result: None,
            correction: Some("".into()),
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

        // Use a non-existent dir — should still return entries with frame_exists=false
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

    #[test]
    fn test_find_frame_at_timestamp_with_report_json() {
        let tmp = tempfile::tempdir().unwrap();
        let frames_dir = tmp.path().join("frames");
        std::fs::create_dir_all(&frames_dir).unwrap();

        // Create a report.json with frame metadata
        let report = serde_json::json!({
            "frames": [
                {"timestamp": 9.5, "path": "/tmp/frames/frame_0001.jpg"},
                {"timestamp": 29.0, "path": "/tmp/frames/frame_0002.jpg"},
            ]
        });
        std::fs::write(
            tmp.path().join("report.json"),
            serde_json::to_string_pretty(&report).unwrap(),
        )
        .unwrap();

        // Should find closest frame within tolerance
        let result = find_frame_at_timestamp(10.0, &frames_dir, 2.0);
        assert_eq!(result, Some("/tmp/frames/frame_0001.jpg".to_string()));

        // Outside tolerance — should not match
        let result = find_frame_at_timestamp(50.0, &frames_dir, 2.0);
        assert!(result.is_none());
    }

    #[test]
    fn test_vision_request_serialization() {
        let req = VisionRequest {
            moment_index: 0,
            timestamp: 10.0,
            timestamp_fmt: "0:10".into(),
            frame_path: "/tmp/frame.jpg".into(),
            word: "hello".into(),
            question: "Is this correct?".into(),
            reason: "deictic".into(),
            priority: 1,
        };
        let json = serde_json::to_string(&req).unwrap();
        let deserialized: VisionRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.moment_index, 0);
        assert_eq!(deserialized.word, "hello");
    }

    #[test]
    fn test_verified_moment_serialization() {
        let vm = VerifiedMoment {
            timestamp: 10.0,
            timestamp_fmt: "0:10".into(),
            word: "test".into(),
            context: "ctx".into(),
            reason: "claim".into(),
            question: "Why?".into(),
            priority: 2,
            vision_result: None,
            correction: Some("fixed".into()),
            verified: true,
        };
        let json = serde_json::to_string(&vm).unwrap();
        let deserialized: VerifiedMoment = serde_json::from_str(&json).unwrap();
        assert!(deserialized.verified);
        assert_eq!(deserialized.correction, Some("fixed".into()));
    }
}
