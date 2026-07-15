use serde::{Deserialize, Serialize};
use std::path::Path;

/// Token estimation: frames use linear interpolation between buckets.
const TOKENS_PER_FRAME: &[(u32, usize)] = &[
    (384, 450),
    (448, 600),
    (512, 800),
    (640, 1250),
    (768, 1800),
    (1024, 3200),
    (1280, 5000),
    (1920, 11000),
];
const CHARS_PER_TOKEN: usize = 4;
const VISION_TOKEN_PER_VERIFICATION: usize = 1000;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AnalysisStats {
    pub processing_time: f64,
    pub video_duration: f64,
    pub video_duration_fmt: String,
    pub video_file_size: u64,
    pub video_resolution: String,
    pub frames_extracted: usize,
    pub frames_resolution: u32,
    pub frames_engine: String,
    pub transcript_segments: usize,
    pub transcript_language: String,
    pub transcript_source: String,
    pub key_moments_detected: usize,
    pub key_moments_priority_1: usize,
    pub vision_verifications: usize,
    pub vision_corrections: usize,
    pub tokens: usize,
}

/// Collect analysis statistics from working directory artifacts.
pub fn collect_stats(work_dir: &Path, processing_time: f64) -> AnalysisStats {
    let mut stats = AnalysisStats {
        processing_time,
        ..Default::default()
    };

    // Read report.json
    let report_path = work_dir.join("report.json");
    if report_path.exists() {
        if let Ok(data) = std::fs::read_to_string(&report_path) {
            if let Ok(report) = serde_json::from_str::<serde_json::Value>(&data) {
                stats.video_duration = report
                    .get("duration")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                stats.video_duration_fmt = format_duration(stats.video_duration);

                if let Some(frames) = report.get("frames").and_then(|v| v.as_array()) {
                    stats.frames_extracted = frames.len();
                }
                if let Some(res) = report.get("resolution").and_then(|v| v.as_u64()) {
                    stats.frames_resolution = res as u32;
                }
                if let Some(engine) = report.get("engine").and_then(|v| v.as_str()) {
                    stats.frames_engine = engine.to_string();
                }

                if let Some(segs) = report.get("transcript").and_then(|v| v.as_array()) {
                    stats.transcript_segments = segs.len();
                }
                if let Some(lang) = report.get("language").and_then(|v| v.as_str()) {
                    stats.transcript_language = lang.to_string();
                }
                if let Some(src) = report.get("transcript_source").and_then(|v| v.as_str()) {
                    stats.transcript_source = src.to_string();
                }
                stats.video_resolution = report
                    .get("video_resolution")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                estimate_tokens(&mut stats, &report);
            }
        }
    }

    // Read key_moments.json
    let moments_path = work_dir.join("key_moments.json");
    if moments_path.exists() {
        if let Ok(data) = std::fs::read_to_string(&moments_path) {
            if let Ok(moments) = serde_json::from_str::<Vec<serde_json::Value>>(&data) {
                stats.key_moments_detected = moments.len();
                stats.key_moments_priority_1 = moments
                    .iter()
                    .filter(|m| m.get("priority").and_then(|v| v.as_u64()) == Some(1))
                    .count();
            }
        }
    }

    // Read vision_results.json
    let vision_path = work_dir.join("vision_results.json");
    if vision_path.exists() {
        if let Ok(data) = std::fs::read_to_string(&vision_path) {
            if let Ok(results) = serde_json::from_str::<Vec<serde_json::Value>>(&data) {
                stats.vision_verifications = results.len();
                stats.vision_corrections = results
                    .iter()
                    .filter(|r| {
                        r.get("correction")
                            .and_then(|v| v.as_str())
                            .map(|s| !s.is_empty())
                            .unwrap_or(false)
                    })
                    .count();
            }
        }
    }

    // Count frames in frames/ directory
    if stats.frames_extracted == 0 {
        stats.frames_extracted = count_jpg_files(&work_dir.join("frames"));
    }

    // Count moment frames
    let moment_frames = count_jpg_files(&work_dir.join("moment_frames"));
    if moment_frames > 0 {
        stats.frames_extracted += moment_frames;
    }

    // Get video file size
    let video_path = work_dir.join("download").join("video.mp4");
    if video_path.exists() {
        if let Ok(meta) = std::fs::metadata(&video_path) {
            stats.video_file_size = meta.len();
        }
    }

    // Get resolution from first frame if not set
    if stats.frames_resolution == 0 && stats.frames_extracted > 0 {
        stats.frames_resolution = 512; // default
    }

    stats
}

/// Estimate token usage using linear interpolation for frames and character
/// counting for text.
fn estimate_tokens(stats: &mut AnalysisStats, report: &serde_json::Value) {
    let resolution = stats.frames_resolution;

    // Frame tokens via linear interpolation between buckets
    let frame_tokens = if resolution == 0 {
        0
    } else {
        interpolate_tokens(resolution)
    };
    let frame_total = frame_tokens * stats.frames_extracted;

    // Text tokens from transcript characters
    let transcript_chars: usize = report
        .get("transcript")
        .and_then(|v| v.as_array())
        .map(|segs| {
            segs.iter()
                .filter_map(|s| s.get("text").and_then(|v| v.as_str()))
                .map(|t| t.len())
                .sum()
        })
        .unwrap_or(0);
    let text_tokens = transcript_chars / CHARS_PER_TOKEN;

    // Vision tokens
    let vision_tokens = stats.vision_verifications * VISION_TOKEN_PER_VERIFICATION;

    stats.tokens = frame_total + text_tokens + vision_tokens;
}

/// Linear interpolation between TOKENS_PER_FRAME buckets.
fn interpolate_tokens(resolution: u32) -> usize {
    if TOKENS_PER_FRAME.is_empty() {
        return 0;
    }
    // Below first bucket
    if resolution <= TOKENS_PER_FRAME[0].0 {
        return TOKENS_PER_FRAME[0].1;
    }
    // Above last bucket
    if resolution >= TOKENS_PER_FRAME[TOKENS_PER_FRAME.len() - 1].0 {
        return TOKENS_PER_FRAME[TOKENS_PER_FRAME.len() - 1].1;
    }
    // Find the two bounding buckets
    for w in TOKENS_PER_FRAME.windows(2) {
        if resolution >= w[0].0 && resolution <= w[1].0 {
            let t = (resolution - w[0].0) as f64 / (w[1].0 - w[0].0) as f64;
            return (w[0].1 as f64 + t * (w[1].1 as f64 - w[0].1 as f64)) as usize;
        }
    }
    TOKENS_PER_FRAME[0].1
}

/// Format stats for Telegram display (rich, multi-line).
pub fn format_stats_telegram(stats: &AnalysisStats) -> String {
    let mut lines: Vec<String> = Vec::new();

    if stats.processing_time > 0.0 {
        lines.push(format!(
            "⏱️ Processing: {}",
            format_duration(stats.processing_time)
        ));
    }
    if stats.video_duration > 0.0 {
        lines.push(format!(
            "🎬 Video: {} ({})",
            stats.video_duration_fmt,
            format_file_size(stats.video_file_size)
        ));
    }
    if !stats.video_resolution.is_empty() {
        lines.push(format!("📐 Resolution: {}", stats.video_resolution));
    }
    if stats.video_file_size > 0 {
        lines.push(format!("💾 File size: {}", format_file_size(stats.video_file_size)));
    }
    if stats.frames_extracted > 0 {
        lines.push(format!(
            "🖼️ Frames: {} ({}px)",
            stats.frames_extracted, stats.frames_resolution
        ));
    }
    if !stats.frames_engine.is_empty() {
        lines.push(format!("🖼️ Engine: {}", stats.frames_engine));
    }
    if stats.transcript_segments > 0 {
        let mut seg_line = format!("📝 Transcript: {} segments", stats.transcript_segments);
        if !stats.transcript_source.is_empty() {
            seg_line.push_str(&format!(" ({})", stats.transcript_source));
        }
        lines.push(seg_line);
    }
    if !stats.transcript_language.is_empty() {
        lines.push(format!("📝 Language: {}", stats.transcript_language));
    }
    if stats.key_moments_detected > 0 {
        lines.push(format!(
            "🎯 Moments: {} detected ({} priority-1)",
            stats.key_moments_detected, stats.key_moments_priority_1
        ));
    }
    if stats.vision_verifications > 0 {
        lines.push(format!(
            "🔍 Vision: {} verifications ({} corrections)",
            stats.vision_verifications, stats.vision_corrections
        ));
    }
    if stats.tokens > 0 {
        lines.push(format!("🪙 Est. tokens: {}", format_number(stats.tokens)));
    }

    if lines.is_empty() {
        "No stats available.".to_string()
    } else {
        lines.join("\n")
    }
}

/// Format stats as a single compact line.
pub fn format_stats_compact(stats: &AnalysisStats) -> String {
    let mut parts: Vec<String> = Vec::new();

    if stats.processing_time > 0.0 {
        parts.push(format!("⏱️ {:.1}s", stats.processing_time));
    }
    if stats.frames_extracted > 0 {
        parts.push(format!("🖼️ {} frames", stats.frames_extracted));
    }
    if stats.transcript_segments > 0 {
        parts.push(format!("📝 {} segs", stats.transcript_segments));
    }
    if stats.key_moments_detected > 0 {
        parts.push(format!("🎯 {} moments", stats.key_moments_detected));
    }
    if stats.vision_verifications > 0 {
        parts.push(format!("🔍 {} verif.", stats.vision_verifications));
    }
    if stats.tokens > 0 {
        parts.push(format!("🪙 {} tok", format_number(stats.tokens)));
    }

    if parts.is_empty() {
        "No stats".to_string()
    } else {
        parts.join(" · ")
    }
}

/// Format bytes as human-readable string (KB, MB, GB).
pub fn format_file_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Format seconds as MM:SS or HH:MM:SS.
pub fn format_duration(seconds: f64) -> String {
    let total = seconds.round() as u64;
    let hours = total / 3600;
    let mins = (total % 3600) / 60;
    let secs = total % 60;
    if hours > 0 {
        format!("{}:{:02}:{:02}", hours, mins, secs)
    } else {
        format!("{}:{:02}", mins, secs)
    }
}

/// Format a number with commas (e.g., 1234567 → "1,234,567").
fn format_number(n: usize) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, ch) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(ch);
    }
    result.chars().rev().collect()
}

/// Count .jpg files in a directory (non-recursive).
fn count_jpg_files(dir: &Path) -> usize {
    if !dir.exists() {
        return 0;
    }
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.path()
                        .extension()
                        .map_or(false, |ext| ext == "jpg" || ext == "jpeg")
                })
                .count()
        })
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_file_size() {
        assert_eq!(format_file_size(0), "0 B");
        assert_eq!(format_file_size(512), "512 B");
        assert_eq!(format_file_size(1024), "1.0 KB");
        assert_eq!(format_file_size(1536), "1.5 KB");
        assert_eq!(format_file_size(1_048_576), "1.0 MB");
        assert_eq!(format_file_size(1_073_741_824), "1.0 GB");
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
    fn test_format_number() {
        assert_eq!(format_number(0), "0");
        assert_eq!(format_number(999), "999");
        assert_eq!(format_number(1000), "1,000");
        assert_eq!(format_number(1234567), "1,234,567");
    }

    #[test]
    fn test_interpolate_tokens() {
        // Exact bucket
        assert_eq!(interpolate_tokens(384), 450);
        assert_eq!(interpolate_tokens(512), 800);
        assert_eq!(interpolate_tokens(1920), 11000);

        // Below first bucket
        assert_eq!(interpolate_tokens(200), 450);

        // Above last bucket
        assert_eq!(interpolate_tokens(3000), 11000);

        // Midpoint interpolation
        let mid = interpolate_tokens(480); // halfway between 448 (600) and 512 (800)
        assert!(mid >= 600 && mid <= 800);
    }

    #[test]
    fn test_format_stats_telegram() {
        let stats = AnalysisStats {
            processing_time: 5.2,
            video_duration: 120.0,
            video_duration_fmt: "2:00".into(),
            video_file_size: 5_242_880,
            frames_extracted: 42,
            frames_resolution: 512,
            frames_engine: "keyframe".into(),
            transcript_segments: 120,
            transcript_source: "captions".into(),
            ..Default::default()
        };
        let output = format_stats_telegram(&stats);
        assert!(output.contains("⏱️ Processing:"));
        assert!(output.contains("🖼️ Frames: 42 (512px)"));
        assert!(output.contains("📝 Transcript: 120 segments"));
    }

    #[test]
    fn test_format_stats_compact() {
        let stats = AnalysisStats {
            processing_time: 5.2,
            frames_extracted: 42,
            transcript_segments: 120,
            ..Default::default()
        };
        let output = format_stats_compact(&stats);
        assert!(output.contains("⏱️ 5.2s"));
        assert!(output.contains("🖼️ 42 frames"));
        assert!(output.contains("📝 120 segs"));
    }

    #[test]
    fn test_format_stats_empty() {
        let stats = AnalysisStats::default();
        assert_eq!(format_stats_telegram(&stats), "No stats available.");
        assert_eq!(format_stats_compact(&stats), "No stats");
    }

    #[test]
    fn test_default_struct() {
        let stats = AnalysisStats::default();
        assert_eq!(stats.processing_time, 0.0);
        assert_eq!(stats.video_duration, 0.0);
        assert_eq!(stats.frames_extracted, 0);
        assert_eq!(stats.tokens, 0);
    }
}
