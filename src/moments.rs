use crate::output::TranscriptSegment;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A key moment in a video transcript that needs visual verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyMoment {
    pub timestamp: f64,
    pub timestamp_fmt: String,
    pub word: String,
    pub context: String,
    pub reason: String,
    pub question: String,
    pub priority: u32,
    pub frame_path: Option<String>,
}

/// Prompt template for LLM-based moment detection.
const MOMENT_DETECTION_PROMPT: &str = r#"You are analyzing a video transcript to identify moments that need visual verification from video frames.

Video Title: {title}
Uploader: {uploader}
Duration: {duration}s ({duration_fmt})

Transcript (timestamped):
{transcript}

Your task: Identify AT LEAST {min_moments} key moments where visual verification would improve accuracy.

Coverage guidance:
- For a {duration}s video, aim for approximately one moment every {interval}s
- Spread moments evenly across the FULL duration — do not cluster in the first half
- Include moments from the beginning, middle, AND end of the video
- Lower-priority moments (4-5) are encouraged to ensure coverage
- It is BETTER to include too many moments than too few

Focus on moments where:
1. **Proper nouns** — names, brands, game titles, tool names that might be misspelled in auto-captions
2. **Claims/statistics** — numbers, prices, dates that need fact-checking
3. **Deictic references** — "this", "that", "here", "look at this" where speaker points at something
4. **Speaker identity** — moments where it's unclear who is speaking
5. **Visual context** — moments where understanding the visual context changes interpretation
6. **Entity validation** — game names, software names, product names that could be transcribed incorrectly
7. **Topic transitions** — moments where the conversation shifts to a new subject
8. **Key arguments** — important points, conclusions, or controversial statements

For EACH moment, provide:
- timestamp: MM:SS format (from the transcript timestamps)
- word: the specific word/phrase that triggered this
- context: 1-2 sentences around this moment
- reason: one of [proper_noun, claim, deictic, speaker_id, visual_context, entity, topic_transition, key_argument]
- question: specific question to ask a vision model about this frame
- priority: 1 (critical) to 5 (nice-to-have)

Return ONLY a valid JSON array. No markdown, no explanation.

Example:
[
  {{
    "timestamp": "0:54",
    "word": "Raknarok",
    "context": "Ya kan Ragnarok. Tahu Raknarok? Raknarok tahu tahu.",
    "reason": "proper_noun",
    "question": "What game name is displayed on screen? Correct any misspellings.",
    "priority": 1
  }},
  {{
    "timestamp": "9:28",
    "word": "1 juta dolar",
    "context": "1 juta dolar berarti kalau rupiah sekarang 18 M",
    "reason": "claim",
    "question": "What prize amount or monetary figure is mentioned or shown?",
    "priority": 1
  }}
]"#;

/// Format seconds as MM:SS or HH:MM:SS.
pub fn format_duration(secs: f64) -> String {
    let total_secs = secs.round() as u64;
    let hours = total_secs / 3600;
    let mins = (total_secs % 3600) / 60;
    let secs = total_secs % 60;
    if hours > 0 {
        format!("{}:{:02}:{:02}", hours, mins, secs)
    } else {
        format!("{}:{:02}", mins, secs)
    }
}

/// Format a timestamp in seconds as MM:SS.
pub fn format_timestamp(secs: f64) -> String {
    format_duration(secs)
}

/// Format transcript segments for analysis.
pub fn format_transcript_for_analysis(segments: &[TranscriptSegment]) -> String {
    segments
        .iter()
        .map(|seg| format!("[{}] {}", format_timestamp(seg.start), seg.text))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Generate a moment detection prompt for the LLM.
pub fn generate_prompt(
    transcript_text: &str,
    title: &str,
    uploader: &str,
    duration: f64,
    max_moments: u32,
    min_moments: Option<u32>,
) -> String {
    let effective_min = min_moments.unwrap_or_else(|| {
        // Heuristic: ~1 moment per 30s of video, clamped to max_moments
        let suggested = ((duration / 30.0).ceil() as u32).max(5);
        suggested.min(max_moments)
    });
    let interval = if effective_min > 0 {
        duration / effective_min as f64
    } else {
        duration
    };
    let duration_fmt = format_duration(duration);

    MOMENT_DETECTION_PROMPT
        .replace("{title}", title)
        .replace("{uploader}", uploader)
        .replace("{duration}", &format!("{:.0}", duration))
        .replace("{duration_fmt}", &duration_fmt)
        .replace("{transcript}", transcript_text)
        .replace("{min_moments}", &effective_min.to_string())
        .replace("{interval}", &format!("{:.0}", interval))
}

/// Prompt template for fused moment detection (scene changes + ASR confidence).
const FUSED_MOMENT_DETECTION_PROMPT: &str = r#"You are analyzing a video transcript fused with scene change detection and ASR confidence data to identify moments that need visual verification.

Video Title: {title}
Uploader: {uploader}
Duration: {duration}s ({duration_fmt})

Scene Changes Detected:
{scene_text}

Transcript (timestamped):
{transcript}

Scene-Transcript Fusion (words near scene boundaries or with low ASR confidence):
{fusion_text}

Your task: Identify AT LEAST {min_moments} key moments where visual verification would improve accuracy.

Priority scoring:
- **P1 (critical)**: Scene cut/transition AND low ASR confidence — most likely to have transcription errors or visual context shifts
- **P2**: Low ASR confidence anywhere — uncertain transcription regardless of scene context
- **P3**: Scene cut/transition with normal confidence — visual context may clarify or contradict transcript
- **P4+ (nice-to-have)**: Normal confidence, no scene boundary — standard moment detection

Coverage guidance:
- For a {duration}s video, aim for approximately one moment every {interval}s
- Spread moments evenly across the FULL duration — do not cluster in the first half
- Include moments from the beginning, middle, AND end of the video
- Lower-priority moments (4-5) are encouraged to ensure coverage
- It is BETTER to include too many moments than too few

Focus on moments where:
1. **Proper nouns** — names, brands, game titles, tool names that might be misspelled in auto-captions
2. **Claims/statistics** — numbers, prices, dates that need fact-checking
3. **Deictic references** — "this", "that", "here", "look at this" where speaker points at something
4. **Speaker identity** — moments where it's unclear who is speaking
5. **Visual context** — moments where understanding the visual context changes interpretation
6. **Entity validation** — game names, software names, product names that could be transcribed incorrectly
7. **Topic transitions** — moments where the conversation shifts to a new subject
8. **Key arguments** — important points, conclusions, or controversial statements
9. **Scene boundaries** — moments at or near scene changes where context shifts
10. **Low confidence words** — words flagged as uncertain in ASR fusion data

For EACH moment, provide:
- timestamp: MM:SS format (from the transcript timestamps)
- word: the specific word/phrase that triggered this
- context: 1-2 sentences around this moment
- scene_info: scene boundary info if applicable (e.g., "Scene 1 → Scene 2 at 1:30" or "none")
- reason: one of [proper_noun, claim, deictic, speaker_id, visual_context, entity, topic_transition, key_argument, scene_boundary, low_confidence]
- question: specific question to ask a vision model about this frame
- priority: 1 (critical) to 5 (nice-to-have) — use the P1-P4+ scoring above

Return ONLY a valid JSON array. No markdown, no explanation.

Example:
[
  {{
    "timestamp": "0:54",
    "word": "Raknarok",
    "context": "Ya kan Ragnarok. Tahu Raknarok? Raknarok tahu tahu.",
    "scene_info": "Scene 1 → Scene 2 at 0:52",
    "reason": "scene_boundary",
    "question": "What game name is displayed on screen? Correct any misspellings.",
    "priority": 1
  }},
  {{
    "timestamp": "3:12",
    "word": "1 juta dolar",
    "context": "1 juta dolar berarti kalau rupiah sekarang 18 M",
    "scene_info": "none",
    "reason": "low_confidence",
    "question": "What prize amount or monetary figure is mentioned or shown?",
    "priority": 2
  }}
]"""#;

/// Generate a fused moment detection prompt that includes scene change context and ASR confidence data.
pub fn generate_fused_prompt(
    transcript_text: &str,
    fusion_text: &str,
    scene_text: &str,
    title: &str,
    uploader: &str,
    duration: f64,
    max_moments: u32,
    min_moments: Option<u32>,
) -> String {
    let effective_min = min_moments.unwrap_or_else(|| {
        let suggested = ((duration / 30.0).ceil() as u32).max(5);
        suggested.min(max_moments)
    });
    let interval = if effective_min > 0 {
        duration / effective_min as f64
    } else {
        duration
    };
    let duration_fmt = format_duration(duration);

    FUSED_MOMENT_DETECTION_PROMPT
        .replace("{title}", title)
        .replace("{uploader}", uploader)
        .replace("{duration}", &format!("{:.0}", duration))
        .replace("{duration_fmt}", &duration_fmt)
        .replace("{transcript}", transcript_text)
        .replace("{fusion_text}", fusion_text)
        .replace("{scene_text}", scene_text)
        .replace("{min_moments}", &effective_min.to_string())
        .replace("{interval}", &format!("{:.0}", interval))
}

/// Build a timestamp → text map from transcript segments.
pub fn build_timestamp_map(segments: &[TranscriptSegment]) -> HashMap<String, String> {
    segments
        .iter()
        .map(|seg| {
            let ts = format_timestamp(seg.start);
            let text = if seg.text.chars().count() > 100 {
                seg.text.chars().take(100).collect::<String>()
            } else {
                seg.text.clone()
            };
            (ts, text)
        })
        .collect()
}

/// Parse a timestamp string (MM:SS or HH:MM:SS) to seconds.
pub fn parse_timestamp(ts: &str) -> Option<f64> {
    let ts = ts.trim();
    let parts: Vec<&str> = ts.split(':').collect();
    match parts.len() {
        2 => {
            let mins: f64 = parts[0].parse().ok()?;
            let secs: f64 = parts[1].parse().ok()?;
            Some(mins * 60.0 + secs)
        }
        3 => {
            let hours: f64 = parts[0].parse().ok()?;
            let mins: f64 = parts[1].parse().ok()?;
            let secs: f64 = parts[2].parse().ok()?;
            Some(hours * 3600.0 + mins * 60.0 + secs)
        }
        _ => None,
    }
}

/// Parse the LLM response into KeyMoment entries.
///
/// Handles markdown code blocks and falls back to regex extraction of JSON arrays.
pub fn parse_moments_response(
    response: &str,
    transcript_segments: &[TranscriptSegment],
) -> Vec<KeyMoment> {
    let timestamp_map = build_timestamp_map(transcript_segments);

    // Strip markdown code blocks
    let cleaned = strip_code_blocks(response);
    // Try to parse as JSON array directly
    let entries: Vec<serde_json::Value> = serde_json::from_str(&cleaned).or_else(|_| {
        // Fallback: find JSON array in the response
        extract_json_array(&cleaned)
            .and_then(|s| serde_json::from_str(&s).ok())
            .ok_or_else(|| {
                use serde::de::Error;
                serde_json::Error::custom("no JSON array found")
            })
    })
    .unwrap_or_default();

    let mut moments: Vec<KeyMoment> = Vec::new();

    for entry in &entries {
        let timestamp_str = entry
            .get("timestamp")
            .and_then(|v| v.as_str())
            .unwrap_or("0:00");

        let timestamp = parse_timestamp(timestamp_str).unwrap_or(0.0);
        let timestamp_fmt = format_timestamp(timestamp);

        let word = entry
            .get("word")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let mut context = entry
            .get("context")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // Fill context from transcript if missing
        if context.is_empty() {
            if let Some(text) = timestamp_map.get(timestamp_str) {
                context = text.clone();
            }
        }

        let reason = entry
            .get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let question = entry
            .get("question")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let priority = entry
            .get("priority")
            .and_then(|v| v.as_u64())
            .unwrap_or(3) as u32;

        moments.push(KeyMoment {
            timestamp,
            timestamp_fmt,
            word,
            context,
            reason,
            question,
            priority,
            frame_path: None,
        });
    }

    // Sort by priority (ascending), then by timestamp
    moments.sort_by(|a, b| {
        a.priority
            .cmp(&b.priority)
            .then_with(|| a.timestamp.partial_cmp(&b.timestamp).unwrap())
    });

    // Deduplicate nearby moments
    deduplicate_moments(&moments, 2.0)
}

/// Remove markdown code block fences (```json ... ```).
fn strip_code_blocks(s: &str) -> String {
    let mut result = s.trim().to_string();

    // Remove opening fence (```json or ```)
    if let Some(start) = result.find("```") {
        let after_fence = start + 3;
        // Skip the language tag if present
        let rest = &result[after_fence..];
        let skip = rest.find('\n').unwrap_or(0) + 1;
        result = result[start + 3 + skip..].to_string();
    }

    // Remove closing fence
    if let Some(end) = result.rfind("```") {
        result = result[..end].to_string();
    }

    result.trim().to_string()
}

/// Extract the first JSON array from a string using simple bracket matching.
fn extract_json_array(s: &str) -> Option<String> {
    let start = s.find('[')?;
    let mut depth = 0;
    let mut in_string = false;
    let mut escape = false;

    for (i, ch) in s[start..].char_indices() {
        if escape {
            escape = false;
            continue;
        }
        if ch == '\\' && in_string {
            escape = true;
            continue;
        }
        if ch == '"' {
            in_string = !in_string;
            continue;
        }
        if in_string {
            continue;
        }
        match ch {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(s[start..=start + i].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

/// Remove duplicate moments that are within `min_gap_seconds` of each other.
///
/// Keeps the one with lower priority (more critical). If tied, keeps the earlier one.
pub fn deduplicate_moments(moments: &[KeyMoment], min_gap_seconds: f64) -> Vec<KeyMoment> {
    if moments.is_empty() {
        return Vec::new();
    }

    let mut kept: Vec<KeyMoment> = Vec::new();

    for moment in moments {
        let is_nearby = kept.iter().any(|k| {
            (k.timestamp - moment.timestamp).abs() < min_gap_seconds
        });

        if !is_nearby {
            kept.push(moment.clone());
        } else {
            // If this moment has higher priority, replace the nearby one
            if let Some(idx) = kept.iter().position(|k| {
                (k.timestamp - moment.timestamp).abs() < min_gap_seconds
            }) {
                if moment.priority < kept[idx].priority {
                    kept[idx] = moment.clone();
                }
            }
        }
    }

    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_segment(start: f64, text: &str) -> TranscriptSegment {
        TranscriptSegment {
            start,
            end: start + 5.0,
            text: text.to_string(),
            words: None,
        }
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(0.0), "0:00");
        assert_eq!(format_duration(59.0), "0:59");
        assert_eq!(format_duration(60.0), "1:00");
        assert_eq!(format_duration(65.0), "1:05");
        assert_eq!(format_duration(3600.0), "1:00:00");
        assert_eq!(format_duration(3661.0), "1:01:01");
    }

    #[test]
    fn test_parse_timestamp() {
        assert_eq!(parse_timestamp("0:00"), Some(0.0));
        assert_eq!(parse_timestamp("1:30"), Some(90.0));
        assert_eq!(parse_timestamp("10:05"), Some(605.0));
        assert_eq!(parse_timestamp("1:02:03"), Some(3723.0));
        assert_eq!(parse_timestamp("bad"), None);
        assert_eq!(parse_timestamp("1:30:00"), Some(5400.0));
    }

    #[test]
    fn test_format_transcript_for_analysis() {
        let segments = vec![make_segment(10.0, "Hello world"), make_segment(65.0, "Goodbye")];
        let result = format_transcript_for_analysis(&segments);
        assert_eq!(result, "[0:10] Hello world\n[1:05] Goodbye");
    }

    #[test]
    fn test_build_timestamp_map() {
        let segments = vec![make_segment(120.0, "This is a test with a very long text that should be truncated at some point because it is way too long to be included in full")];
        let map = build_timestamp_map(&segments);
        assert!(map.contains_key("2:00"));
        let text = map.get("2:00").unwrap();
        assert!(text.len() <= 100);
    }

    #[test]
    fn test_strip_code_blocks() {
        let input = "```json\n[{\"a\": 1}]\n```";
        let result = strip_code_blocks(input);
        assert_eq!(result, "[{\"a\": 1}]");

        let input2 = "```\n[{\"b\": 2}]\n```";
        let result2 = strip_code_blocks(input2);
        assert_eq!(result2, "[{\"b\": 2}]");
    }

    #[test]
    fn test_extract_json_array() {
        let s = "Here is the result: [{\"x\": 1}, {\"x\": 2}] and more text";
        let result = extract_json_array(s).unwrap();
        assert_eq!(result, "[{\"x\": 1}, {\"x\": 2}]");
    }

    #[test]
    fn test_parse_moments_response_valid() {
        let response = r#"[{"timestamp":"0:30","word":"Raknarok","context":"Playing Raknarok","reason":"proper_noun","question":"What game?","priority":1},{"timestamp":"1:00","word":"test","context":"test context","reason":"claim","question":"Is this true?","priority":3}]"#;
        let segments = vec![make_segment(30.0, "Playing Raknarok")];
        let moments = parse_moments_response(response, &segments);
        assert_eq!(moments.len(), 2);
        assert_eq!(moments[0].priority, 1);
        assert_eq!(moments[1].priority, 3);
        assert_eq!(moments[0].timestamp, 30.0);
    }

    #[test]
    fn test_parse_moments_response_with_code_block() {
        let response = "```json\n[{\"timestamp\":\"1:00\",\"word\":\"hello\",\"context\":\"\",\"reason\":\"deictic\",\"question\":\"What?\",\"priority\":2}]\n```";
        let segments = vec![];
        let moments = parse_moments_response(response, &segments);
        assert_eq!(moments.len(), 1);
        assert_eq!(moments[0].word, "hello");
    }

    #[test]
    fn test_parse_moments_response_fills_context_from_transcript() {
        let response = r#"[{"timestamp":"0:10","word":"test","context":"","reason":"claim","question":"?","priority":2}]"#;
        let segments = vec![make_segment(10.0, "This is the transcript text")];
        let moments = parse_moments_response(response, &segments);
        assert_eq!(moments[0].context, "This is the transcript text");
    }

    #[test]
    fn test_deduplicate_moments() {
        let moments = vec![
            KeyMoment { timestamp: 10.0, timestamp_fmt: "0:10".into(), word: "a".into(), context: "".into(), reason: "claim".into(), question: "".into(), priority: 3, frame_path: None },
            KeyMoment { timestamp: 10.5, timestamp_fmt: "0:10".into(), word: "b".into(), context: "".into(), reason: "claim".into(), question: "".into(), priority: 1, frame_path: None },
            KeyMoment { timestamp: 50.0, timestamp_fmt: "0:50".into(), word: "c".into(), context: "".into(), reason: "claim".into(), question: "".into(), priority: 2, frame_path: None },
        ];
        let deduped = deduplicate_moments(&moments, 2.0);
        assert_eq!(deduped.len(), 2);
        // The higher-priority moment (1) should replace the nearby lower-priority (3)
        assert_eq!(deduped[0].word, "b");
        assert_eq!(deduped[0].priority, 1);
        assert_eq!(deduped[1].word, "c");
    }

    #[test]
    fn test_generate_prompt() {
        let segments = vec![make_segment(0.0, "Hello")];
        let transcript = format_transcript_for_analysis(&segments);
        let prompt = generate_prompt(&transcript, "Test Video", "Uploader", 120.0, 50, None);
        assert!(prompt.contains("Test Video"));
        assert!(prompt.contains("Uploader"));
        assert!(prompt.contains("120s"));
        assert!(prompt.contains("[0:00] Hello"));
    }

    #[test]
    fn test_generate_fused_prompt_contains_scene_data() {
        let segments = vec![make_segment(0.0, "Hello")];
        let transcript = format_transcript_for_analysis(&segments);
        let fused_text =
            "[0:10] \"test\" — confidence: 65%, scene: scene 1 (10s), position: AtCut";
        let scene_text = "Scene 1: 0:00 - 0:10 (10.0s)";
        let prompt = generate_fused_prompt(
            &transcript,
            fused_text,
            scene_text,
            "Test Video",
            "Uploader",
            120.0,
            10,
            None,
        );
        assert!(prompt.contains("Scene Changes Detected"));
        assert!(prompt.contains("Scene 1: 0:00 - 0:10"));
    }
}
