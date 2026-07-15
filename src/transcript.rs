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
                if utf8.is_empty() { return None; }
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
                    segments.push(TranscriptSegment { start, end, text, words: None });
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
    segments.iter()
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
