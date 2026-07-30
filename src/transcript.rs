use crate::error::{Result, WatchError};
use crate::output::TranscriptSegment;
use std::path::Path;

pub fn parse_json3(content: &str) -> Result<Vec<TranscriptSegment>> {
    let data: serde_json::Value = serde_json::from_str(content)?;
    let empty_vec = vec![];
    let events = data["events"].as_array().unwrap_or(&empty_vec);
    let mut segments = Vec::new();
    for event in events {
        let empty_segs = vec![];
        let segs = event["segs"].as_array().unwrap_or(&empty_segs);
        let text: String = segs
            .iter()
            .filter_map(|s| s["utf8"].as_str())
            .collect::<Vec<_>>()
            .join("")
            .trim()
            .to_string();
        if text.is_empty() || text == "\n" {
            continue;
        }
        let start_ms = event["tStartMs"].as_f64().unwrap_or(0.0);
        let dur_ms = event["dDurationMs"].as_f64().unwrap_or(0.0);

        // Extract word-level timing from segs
        let words: Vec<crate::output::WordTiming> = segs
            .iter()
            .filter_map(|s| {
                let utf8 = s["utf8"].as_str()?.trim();
                if utf8.is_empty() {
                    return None;
                }
                let offset_ms = s["tOffsetMs"].as_f64().unwrap_or(0.0);
                let confidence = s["acAsrConf"].as_i64().unwrap_or(0) as i32;
                Some(crate::output::WordTiming {
                    word: utf8.to_string(),
                    start: ((start_ms + offset_ms) / 1000.0 * 1000.0).round() / 1000.0,
                    confidence,
                })
            })
            .collect();

        segments.push(TranscriptSegment {
            start: start_ms / 1000.0,
            end: (start_ms + dur_ms) / 1000.0,
            text,
            words: if words.is_empty() { None } else { Some(words) },
        });
    }
    Ok(dedupe(segments))
}

pub fn parse_vtt(content: &str) -> Result<Vec<TranscriptSegment>> {
    let mut segments = Vec::new();
    let mut lines = content.lines().peekable();
    while let Some(line) = lines.peek() {
        if line.starts_with("WEBVTT") || line.trim().is_empty() {
            lines.next();
        } else {
            break;
        }
    }
    while let Some(line) = lines.next() {
        if line.contains("-->") {
            let parts: Vec<&str> = line.split("-->").collect();
            if parts.len() == 2 {
                let start = parse_vtt_time(parts[0].trim());
                let end = parse_vtt_time(parts[1].trim());
                let mut text = String::new();
                while let Some(next) = lines.next() {
                    if next.trim().is_empty() {
                        break;
                    }
                    if !text.is_empty() {
                        text.push(' ');
                    }
                    text.push_str(next.trim());
                }
                if !text.is_empty() {
                    segments.push(TranscriptSegment {
                        start,
                        end,
                        text,
                        words: None,
                    });
                }
            }
        }
    }
    Ok(dedupe(segments))
}

fn parse_vtt_time(s: &str) -> f64 {
    let parts: Vec<&str> = s.split(':').collect();
    match parts.len() {
        3 => {
            let h: f64 = parts[0].parse().unwrap_or(0.0);
            let m: f64 = parts[1].parse().unwrap_or(0.0);
            let sec: f64 = parts[2].replace(',', ".").parse().unwrap_or(0.0);
            h * 3600.0 + m * 60.0 + sec
        }
        2 => {
            let m: f64 = parts[0].parse().unwrap_or(0.0);
            let sec: f64 = parts[1].replace(',', ".").parse().unwrap_or(0.0);
            m * 60.0 + sec
        }
        _ => 0.0,
    }
}

fn dedupe(segments: Vec<TranscriptSegment>) -> Vec<TranscriptSegment> {
    let mut out = Vec::new();
    for seg in segments {
        if out
            .last()
            .map_or(false, |s: &TranscriptSegment| s.text == seg.text)
        {
            continue;
        }
        out.push(seg);
    }
    out
}

/// Filter segments to only those overlapping [lo, hi].
/// If both bounds are None, returns segments unchanged.
pub fn filter_by_range(
    segments: &[TranscriptSegment],
    start: Option<f64>,
    end: Option<f64>,
) -> Vec<TranscriptSegment> {
    let lo = start.unwrap_or(0.0);
    let hi = end.unwrap_or(f64::INFINITY);
    segments
        .iter()
        .filter(|s| s.end >= lo && s.start <= hi)
        .cloned()
        .collect()
}

pub fn parse_subtitle_file(path: &Path) -> Result<Vec<TranscriptSegment>> {
    let content = std::fs::read_to_string(path)?;
    match path.extension().and_then(|e| e.to_str()) {
        Some("json3") => parse_json3(&content),
        Some("vtt") => parse_vtt(&content),
        _ => Err(WatchError::Ffmpeg(format!(
            "Unsupported subtitle format: {:?}",
            path.extension()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_json3 tests ────────────────────────────────────────────

    #[test]
    fn test_parse_json3_word_level_timing() {
        let json = serde_json::json!({
            "events": [{
                "tStartMs": 1000,
                "dDurationMs": 2000,
                "segs": [
                    { "utf8": "Hello", "tOffsetMs": 0, "acAsrConf": 95 },
                    { "utf8": "world", "tOffsetMs": 400, "acAsrConf": 90 }
                ]
            }]
        });
        let segs = parse_json3(&json.to_string()).unwrap();
        assert_eq!(segs.len(), 1);
        // No space seg in data, so text is concatenated without separator
        assert_eq!(segs[0].text, "Helloworld");
        assert!((segs[0].start - 1.0).abs() < 0.001);
        assert!((segs[0].end - 3.0).abs() < 0.001);

        let words = segs[0].words.as_ref().unwrap();
        // Whitespace-only segs are trimmed and filtered out
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].word, "Hello");
        assert_eq!(words[0].confidence, 95);
        assert!((words[0].start - 1.0).abs() < 0.001);
        assert_eq!(words[1].word, "world");
        assert_eq!(words[1].confidence, 90);
        // word start = (tStartMs + tOffsetMs) / 1000
        assert!((words[1].start - 1.4).abs() < 0.001);
    }

    #[test]
    fn test_parse_json3_events_with_no_segs_skipped() {
        let json = serde_json::json!({
            "events": [
                { "tStartMs": 0, "dDurationMs": 1000 },
                { "tStartMs": 1000, "dDurationMs": 1000, "segs": [
                    { "utf8": "only event" }
                ]}
            ]
        });
        let segs = parse_json3(&json.to_string()).unwrap();
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].text, "only event");
    }

    #[test]
    fn test_parse_json3_whitespace_only_filtered() {
        let json = serde_json::json!({
            "events": [
                { "tStartMs": 0, "dDurationMs": 500, "segs": [{ "utf8": "   " }] },
                { "tStartMs": 500, "dDurationMs": 500, "segs": [{ "utf8": "\n" }] },
                { "tStartMs": 1000, "dDurationMs": 500, "segs": [{ "utf8": "real text" }] }
            ]
        });
        let segs = parse_json3(&json.to_string()).unwrap();
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].text, "real text");
    }

    // ── parse_vtt tests ──────────────────────────────────────────────

    #[test]
    fn test_parse_vtt_comma_separator() {
        let vtt = "\
WEBVTT

00:00:01,500 --> 00:00:03,000
Hello world

00:00:04,000 --> 00:00:06,000
Second cue
";
        let segs = parse_vtt(vtt).unwrap();
        assert_eq!(segs.len(), 2);
        assert!((segs[0].start - 1.5).abs() < 0.001);
        assert!((segs[0].end - 3.0).abs() < 0.001);
        assert_eq!(segs[0].text, "Hello world");
        assert_eq!(segs[1].text, "Second cue");
    }

    #[test]
    fn test_parse_vtt_no_header() {
        // No WEBVTT header — should still parse
        let vtt = "\
00:00:00.000 --> 00:00:02.000
No header here
";
        let segs = parse_vtt(vtt).unwrap();
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].text, "No header here");
    }

    #[test]
    fn test_parse_vtt_empty_cue_skipped() {
        let vtt = "\
WEBVTT

00:00:00.000 --> 00:00:02.000
Text here

00:00:03.000 --> 00:00:05.000

00:00:06.000 --> 00:00:08.000
Also here
";
        let segs = parse_vtt(vtt).unwrap();
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0].text, "Text here");
        assert_eq!(segs[1].text, "Also here");
    }

    // ── filter_by_range tests ────────────────────────────────────────

    #[test]
    fn test_filter_by_range_overlapping_filtered() {
        // Segments: [1.0-2.0], [3.0-4.0], [5.0-6.0]
        // Filter [2.5-4.5] — only [3.0-4.0] overlaps
        let segs = vec![
            TranscriptSegment {
                start: 1.0,
                end: 2.0,
                text: "a".into(),
                words: None,
            },
            TranscriptSegment {
                start: 3.0,
                end: 4.0,
                text: "b".into(),
                words: None,
            },
            TranscriptSegment {
                start: 5.0,
                end: 6.0,
                text: "c".into(),
                words: None,
            },
        ];
        let result = filter_by_range(&segs, Some(2.5), Some(4.5));
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].text, "b");
    }

    #[test]
    fn test_filter_by_range_exact_boundary_included() {
        // Segments touching exact boundaries of [2.0, 3.0] should be included
        let segs = vec![
            TranscriptSegment {
                start: 0.5,
                end: 1.5,
                text: "before".into(),
                words: None,
            },
            TranscriptSegment {
                start: 2.0,
                end: 2.5,
                text: "at-start".into(),
                words: None,
            },
            TranscriptSegment {
                start: 2.5,
                end: 3.0,
                text: "at-end".into(),
                words: None,
            },
            TranscriptSegment {
                start: 3.5,
                end: 4.0,
                text: "after".into(),
                words: None,
            },
        ];
        let result = filter_by_range(&segs, Some(2.0), Some(3.0));
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].text, "at-start");
        assert_eq!(result[1].text, "at-end");
    }

    #[test]
    fn test_filter_by_range_both_none_returns_all() {
        let segs = vec![
            TranscriptSegment {
                start: 0.0,
                end: 1.0,
                text: "a".into(),
                words: None,
            },
            TranscriptSegment {
                start: 2.0,
                end: 3.0,
                text: "b".into(),
                words: None,
            },
        ];
        let result = filter_by_range(&segs, None, None);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_filter_by_range_no_segments_match() {
        let segs = vec![TranscriptSegment {
            start: 1.0,
            end: 2.0,
            text: "a".into(),
            words: None,
        }];
        let result = filter_by_range(&segs, Some(10.0), Some(20.0));
        assert!(result.is_empty());
    }

    // ── parse_subtitle_file tests ────────────────────────────────────

    #[test]
    fn test_parse_subtitle_file_json3_dispatch() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("subs.json3");
        let json = serde_json::json!({
            "events": [{ "tStartMs": 0, "dDurationMs": 1000, "segs": [{ "utf8": "hi" }] }]
        });
        std::fs::write(&path, json.to_string()).unwrap();

        let result = parse_subtitle_file(&path).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].text, "hi");
    }

    #[test]
    fn test_parse_subtitle_file_vtt_dispatch() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("subs.vtt");
        let vtt = "\
WEBVTT

00:00:00.000 --> 00:00:01.000
Hello
";
        std::fs::write(&path, vtt).unwrap();

        let result = parse_subtitle_file(&path).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].text, "Hello");
    }

    #[test]
    fn test_parse_subtitle_file_srt_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("subs.srt");
        std::fs::write(&path, "1\n00:00:00,000 --> 00:00:01,000\nHello\n").unwrap();
        let result = parse_subtitle_file(&path);
        assert!(result.is_err());
    }

    // ── Unicode/encoding edge cases ─────────────────────────────────

    #[test]
    fn test_parse_json3_cjk_text() {
        let json = serde_json::json!({
            "events": [{
                "tStartMs": 0,
                "dDurationMs": 3000,
                "segs": [
                    { "utf8": "日本語テスト" },
                    { "utf8": " " },
                    { "utf8": "中文测试" },
                    { "utf8": " " },
                    { "utf8": "한국어테스트" }
                ]
            }]
        });
        let segs = parse_json3(&json.to_string()).unwrap();
        assert_eq!(segs.len(), 1);
        assert!(segs[0].text.contains("日本語テスト"));
        assert!(segs[0].text.contains("中文测试"));
        assert!(segs[0].text.contains("한국어테스트"));
    }

    #[test]
    fn test_parse_json3_emoji_in_text() {
        let json = serde_json::json!({
            "events": [{
                "tStartMs": 0,
                "dDurationMs": 2000,
                "segs": [
                    { "utf8": "Hello 🌍 World" },
                    { "utf8": " " },
                    { "utf8": "🎉🎈🎊" }
                ]
            }]
        });
        let segs = parse_json3(&json.to_string()).unwrap();
        assert_eq!(segs.len(), 1);
        assert!(segs[0].text.contains("🌍"));
        assert!(segs[0].text.contains("🎉🎈🎊"));
    }

    #[test]
    fn test_parse_vtt_with_bom() {
        let vtt = "\u{FEFF}WEBVTT\n\n00:00:00.000 --> 00:00:02.000\nBOM text here\n";
        let segs = parse_vtt(vtt).unwrap();
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].text, "BOM text here");
    }

    // ── Large input stress tests ────────────────────────────────────

    #[test]
    fn test_parse_json3_1000_events() {
        let mut events = Vec::new();
        for i in 0..1000 {
            events.push(serde_json::json!({
                "tStartMs": i * 1000,
                "dDurationMs": 800,
                "segs": [{ "utf8": format!("event {}", i) }]
            }));
        }
        let json = serde_json::json!({ "events": events });
        let segs = parse_json3(&json.to_string()).unwrap();
        assert_eq!(segs.len(), 1000);
        assert_eq!(segs[0].text, "event 0");
        assert_eq!(segs[999].text, "event 999");
    }

    #[test]
    fn test_filter_by_range_500_segments() {
        let segs: Vec<TranscriptSegment> = (0..500)
            .map(|i| TranscriptSegment {
                start: i as f64,
                end: (i + 1) as f64,
                text: format!("seg{}", i),
                words: None,
            })
            .collect();
        let result = filter_by_range(&segs, Some(100.0), Some(200.0));
        // Segments [100,200] overlap [100.0,200.0] → segments 99–200 inclusive (102 segments)
        assert!(result.len() >= 100);
        assert!(result.len() <= 102);
        // First result should have start near 99
        assert!(result[0].start >= 99.0);
        // Last result should have end near 201
        assert!(result.last().unwrap().end <= 201.0);
    }
}
