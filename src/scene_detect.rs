use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

use crate::error::{Result, WatchError};

/// A single scene segment with frame-accurate boundaries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneBoundary {
    pub start_sec: f64,
    pub end_sec: f64,
    pub duration_sec: f64,
    pub frame_start: u64,
    pub frame_end: u64,
}

impl SceneBoundary {
    pub fn new(start_sec: f64, end_sec: f64, _fps: f64, frame_start: u64, frame_end: u64) -> Self {
        Self {
            start_sec,
            end_sec,
            duration_sec: end_sec - start_sec,
            frame_start,
            frame_end,
        }
    }
}

/// Result of scene detection with metadata.
#[derive(Debug, Clone, Serialize)]
pub struct SceneDetectionResult {
    pub boundaries: Vec<SceneBoundary>,
    pub fps: f64,
    pub detection_time_ms: u64,
}

impl SceneDetectionResult {
    pub fn total_scenes(&self) -> usize {
        self.boundaries.len()
    }
}

/// Parse av-scenechange JSON output (format: {"scene_changes":[0,24,50,...]})
/// into SceneBoundary list.
pub fn parse_av_scenechange_output(json: &str, fps: f64) -> Vec<SceneBoundary> {
    let data: serde_json::Value = serde_json::from_str(json).unwrap_or_default();
    let frames = data["scene_changes"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_u64()).collect::<Vec<_>>())
        .unwrap_or_default();

    let mut boundaries = Vec::with_capacity(frames.len());
    for (i, &frame) in frames.iter().enumerate() {
        let start_sec = frame as f64 / fps;
        let end_sec = frames.get(i + 1).map_or(f64::INFINITY, |&f| f as f64 / fps);
        let frame_end = frames.get(i + 1).copied().unwrap_or(0);
        boundaries.push(SceneBoundary::new(
            start_sec, end_sec, fps, frame, frame_end,
        ));
    }
    boundaries
}

/// Convert existing timestamp list (from ffmpeg scene filter) to SceneBoundary list.
pub fn timestamps_to_boundaries(timestamps: &[f64], fps: f64, duration: f64) -> Vec<SceneBoundary> {
    let mut boundaries = Vec::with_capacity(timestamps.len());
    for (i, &ts) in timestamps.iter().enumerate() {
        let end_ts = timestamps.get(i + 1).copied().unwrap_or(duration);
        let frame_start = (ts * fps) as u64;
        let frame_end = (end_ts * fps) as u64;
        boundaries.push(SceneBoundary::new(ts, end_ts, fps, frame_start, frame_end));
    }
    boundaries
}

/// Check if av-scenechange binary is available.
pub fn is_available() -> bool {
    which::which("av-scenechange").is_ok()
}

/// Detect scenes using av-scenechange (mandatory).
/// Returns error if av-scenechange is not installed.
pub fn detect(video_path: &Path, fps: f64, _duration: f64) -> Result<SceneDetectionResult> {
    if !is_available() {
        return Err(WatchError::Ffmpeg(
            "av-scenechange is required but not found. Install: cargo install av-scenechange --features ffmpeg".to_string()
        ));
    }

    let start = std::time::Instant::now();
    let result = detect_with_av_scenechange(video_path, fps)?;
    let mut result = result;
    result.detection_time_ms = start.elapsed().as_millis() as u64;
    Ok(result)
}

fn detect_with_av_scenechange(video_path: &Path, fps: f64) -> Result<SceneDetectionResult> {
    let output = Command::new("av-scenechange")
        .args(["--min-scenecut", "24", video_path.to_str().unwrap_or("")])
        .output()
        .map_err(|e| WatchError::Ffmpeg(format!("av-scenechange failed to run: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(WatchError::Ffmpeg(format!(
            "av-scenechange exited with {}: {}",
            output.status, stderr
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let boundaries = parse_av_scenechange_output(&stdout, fps);

    Ok(SceneDetectionResult {
        boundaries,
        fps,
        detection_time_ms: 0,
    })
}

/// Convert timestamp list to SceneBoundary list (kept for external callers).
pub fn detect_from_timestamps(timestamps: &[f64], fps: f64, duration: f64) -> SceneDetectionResult {
    let boundaries = timestamps_to_boundaries(timestamps, fps, duration);
    SceneDetectionResult {
        boundaries,
        fps,
        detection_time_ms: 0,
    }
}
