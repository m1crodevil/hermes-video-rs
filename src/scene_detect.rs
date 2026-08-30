use serde::Serialize;
use std::path::Path;

use crate::error::{Result, WatchError};

/// A scene segment with frame-accurate boundaries for agent selection.
#[derive(Debug, Clone, Serialize)]
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

/// Check whether the av-scenechange executable is available.
pub fn is_available() -> bool {
    which::which("av-scenechange").is_ok()
}

/// Detect scenes using av-scenechange.
pub fn detect(video_path: &Path, fps: f64, _duration: f64) -> Result<SceneDetectionResult> {
    if !is_available() {
        return Err(WatchError::Ffmpeg(
            "av-scenechange is required but not found. Install: cargo install av-scenechange --features ffmpeg".to_string(),
        ));
    }

    let start = std::time::Instant::now();
    let mut result = detect_with_av_scenechange(video_path, fps)?;
    result.detection_time_ms = start.elapsed().as_millis() as u64;
    Ok(result)
}

fn detect_with_av_scenechange(video_path: &Path, fps: f64) -> Result<SceneDetectionResult> {
    use av_scenechange::{Decoder, DetectionOptions, SceneDetectionSpeed};

    let mut decoder = Decoder::from_file(video_path)
        .map_err(|e| WatchError::Ffmpeg(format!("av-scenechange decoder init failed: {e}")))?;
    let opts = DetectionOptions {
        analysis_speed: SceneDetectionSpeed::Fast,
        detect_flashes: false,
        min_scenecut_distance: Some(24),
        max_scenecut_distance: Some(250),
        ..DetectionOptions::default()
    };
    let results = av_scenechange::detect_scene_changes::<u8>(&mut decoder, opts, None, None)
        .map_err(|e| WatchError::Ffmpeg(format!("av-scenechange detection failed: {e}")))?;

    let boundaries = results
        .scene_changes
        .iter()
        .enumerate()
        .map(|(i, &frame)| {
            let end_frame = results.scene_changes.get(i + 1).copied().unwrap_or(0);
            let end_sec = results
                .scene_changes
                .get(i + 1)
                .map_or(f64::INFINITY, |&next| next as f64 / fps);
            SceneBoundary::new(
                frame as f64 / fps,
                end_sec,
                fps,
                frame as u64,
                end_frame as u64,
            )
        })
        .collect();

    Ok(SceneDetectionResult {
        boundaries,
        fps,
        detection_time_ms: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_boundary_has_expected_duration() {
        let boundary = SceneBoundary::new(5.0, 20.0, 24.0, 120, 480);
        assert_eq!(boundary.duration_sec, 15.0);
    }

    #[test]
    fn detection_result_counts_boundaries() {
        let result = SceneDetectionResult {
            boundaries: vec![SceneBoundary::new(0.0, 5.0, 24.0, 0, 120)],
            fps: 24.0,
            detection_time_ms: 0,
        };
        assert_eq!(result.total_scenes(), 1);
    }
}
