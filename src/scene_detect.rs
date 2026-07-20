use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

use crate::error::{Result, WatchError};

/// A single scene segment with frame-accurate boundaries and scoring data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneBoundary {
    pub start_sec: f64,
    pub end_sec: f64,
    pub duration_sec: f64,
    pub frame_start: u64,
    pub frame_end: u64,
    // Scoring data from av-scenechange library
    pub inter_cost: Option<f64>,
    pub imp_block_cost: Option<f64>,
    pub backward_adjusted_cost: Option<f64>,
    pub forward_adjusted_cost: Option<f64>,
    pub threshold: Option<f64>,
}

impl SceneBoundary {
    pub fn new(start_sec: f64, end_sec: f64, _fps: f64, frame_start: u64, frame_end: u64) -> Self {
        Self {
            start_sec,
            end_sec,
            duration_sec: end_sec - start_sec,
            frame_start,
            frame_end,
            inter_cost: None,
            imp_block_cost: None,
            backward_adjusted_cost: None,
            forward_adjusted_cost: None,
            threshold: None,
        }
    }

    /// Create with scoring data from av-scenechange library.
    pub fn with_score(
        start_sec: f64, end_sec: f64, _fps: f64,
        frame_start: u64, frame_end: u64,
        inter_cost: f64, imp_block_cost: f64,
        backward_adjusted_cost: f64, forward_adjusted_cost: f64,
        threshold: f64,
    ) -> Self {
        Self {
            start_sec, end_sec,
            duration_sec: end_sec - start_sec,
            frame_start, frame_end,
            inter_cost: Some(inter_cost),
            imp_block_cost: Some(imp_block_cost),
            backward_adjusted_cost: Some(backward_adjusted_cost),
            forward_adjusted_cost: Some(forward_adjusted_cost),
            threshold: Some(threshold),
        }
    }

    /// Composite significance score (higher = more significant scene change).
    pub fn significance(&self) -> f64 {
        self.inter_cost.unwrap_or(0.0)
            + self.imp_block_cost.unwrap_or(0.0) * 10.0
            + self.backward_adjusted_cost.unwrap_or(0.0)
            + self.forward_adjusted_cost.unwrap_or(0.0)
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

/// Convert timestamp list to SceneBoundary list (kept for external callers).
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

/// Detect scenes using av-scenechange.
/// Uses --speed 1 for faster detection (acceptable for balanced mode).
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
    use av_scenechange::{Decoder, DetectionOptions, SceneDetectionSpeed};

    // Create decoder from video file (auto-selects backend: ffmpeg/ffms2/y4m)
    let mut decoder = match Decoder::from_file(video_path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("[watch2] av-scenechange decoder init failed: {e}");
            eprintln!("[watch2] Falling back to ffmpeg scene detection");
            return detect_with_ffmpeg(video_path, fps);
        }
    };

    // Configure detection options — use Fast for balanced mode speed
    let opts = DetectionOptions {
        analysis_speed: SceneDetectionSpeed::Fast,
        detect_flashes: false,
        min_scenecut_distance: Some(24),
        max_scenecut_distance: Some(250),
        ..DetectionOptions::default()
    };

    // Run scene detection
    let results = match av_scenechange::detect_scene_changes::<u8>(&mut decoder, opts, None, None) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[watch2] av-scenechange detection failed: {e}");
            eprintln!("[watch2] Falling back to ffmpeg scene detection");
            return detect_with_ffmpeg(video_path, fps);
        }
    };

    eprintln!(
        "[watch2] av-scenechange: {} scenes detected ({} fps)",
        results.scene_changes.len(),
        results.speed
    );

    // Convert DetectionResults → Vec<SceneBoundary> WITH scores
    let boundaries: Vec<SceneBoundary> = results
        .scene_changes
        .iter()
        .enumerate()
        .map(|(i, &frame)| {
            let start_sec = frame as f64 / fps;
            let end_sec = results
                .scene_changes
                .get(i + 1)
                .map_or(f64::INFINITY, |&next| next as f64 / fps);
            let frame_end = results.scene_changes.get(i + 1).copied().unwrap_or(0);

            // Look up scores for this frame
            if let Some(score) = results.scores.get(&frame) {
                SceneBoundary::with_score(
                    start_sec,
                    end_sec,
                    fps,
                    frame as u64,
                    frame_end as u64,
                    score.inter_cost,
                    score.imp_block_cost,
                    score.backward_adjusted_cost,
                    score.forward_adjusted_cost,
                    score.threshold,
                )
            } else {
                SceneBoundary::new(start_sec, end_sec, fps, frame as u64, frame_end as u64)
            }
        })
        .collect();

    Ok(SceneDetectionResult {
        boundaries,
        fps,
        detection_time_ms: 0,
    })
}

/// Fallback: ffmpeg scene detection (used when av-scenechange fails)
fn detect_with_ffmpeg(video_path: &Path, fps: f64) -> Result<SceneDetectionResult> {
    let meta = crate::frames::get_metadata(video_path)?;
    let threshold = crate::scene::adaptive_threshold(meta.duration);
    let timestamps = crate::scene::detect_scene_changes(video_path, threshold)?;
    let boundaries = timestamps_to_boundaries(&timestamps, fps, meta.duration);
    Ok(SceneDetectionResult {
        boundaries,
        fps,
        detection_time_ms: 0,
    })
}
