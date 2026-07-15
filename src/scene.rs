use crate::error::{WatchError, Result};
use std::path::Path;

/// Duration-based adaptive scene detection threshold.
/// Longer videos → lower threshold (more sensitive to scene changes).
pub fn adaptive_threshold(duration_secs: f64) -> f64 {
    if duration_secs <= 60.0 { 0.25 }
    else if duration_secs <= 300.0 { 0.22 }
    else if duration_secs <= 600.0 { 0.20 }
    else if duration_secs <= 1800.0 { 0.17 }
    else if duration_secs <= 3600.0 { 0.15 }
    else { 0.12 }
}

/// Detect scene changes using ffmpeg's scene detection filter.
/// Returns timestamps (in seconds) of detected scene changes.
/// Always includes frame 0 (first frame).
pub fn detect_scene_changes(video_path: &Path, threshold: f64) -> Result<Vec<f64>> {
    let filter = format!("select='eq(n\\,0)+gt(scene\\,{threshold})',showinfo");

    let output = std::process::Command::new("ffmpeg")
        .args([
            "-hide_banner", "-loglevel", "info",
            "-i", video_path.to_str().unwrap_or(""),
            "-vf", &filter,
            "-vsync", "vfr",
            "-f", "null",
            "-",
        ])
        .output()
        .map_err(|e| WatchError::Ffmpeg(format!("Failed to run ffmpeg: {e}")))?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut timestamps = Vec::new();

    for line in stderr.lines() {
        if let Some(pos) = line.find("pts_time:") {
            let rest = &line[pos + 10..];
            if let Some(end) = rest.find(|c: char| !c.is_ascii_digit() && c != '.') {
                if let Ok(ts) = rest[..end].parse::<f64>() {
                    timestamps.push(ts);
                }
            }
        }
    }

    Ok(timestamps)
}
