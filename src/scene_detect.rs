use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::error::{Result, WatchError};

// ─── Scene Scores Output ────────────────────────────────────────────────────

/// Top-level structure for scene_scores.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneScoresOutput {
    pub video_duration: f64,
    pub fps: f64,
    pub total_scenes: usize,
    pub detection_time_ms: u64,
    pub scenes: Vec<SceneScoreEntry>,
    pub frame_scores: Vec<FrameScoreEntry>,
}

/// Per-scene entry in scene_scores.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneScoreEntry {
    pub index: usize,
    pub start_sec: f64,
    pub end_sec: f64,
    pub duration_sec: f64,
    pub frame_start: u64,
    pub frame_end: u64,
    pub significance: f64,
    pub scores: ScoreDetails,
}

/// Raw scoring data from av-scenechange
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreDetails {
    pub inter_cost: f64,
    pub imp_block_cost: f64,
    pub backward_adjusted_cost: f64,
    pub forward_adjusted_cost: f64,
    pub threshold: f64,
}

/// Per-frame score entry mapping extracted frames to scene significance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameScoreEntry {
    pub timestamp: f64,
    pub scene_index: usize,
    pub significance: f64,
    pub is_scene_cut: bool,
    pub scene_position: String,
}

/// Classify where a timestamp falls within a scene.
fn classify_position_str(start_sec: f64, end_sec: f64, timestamp: f64) -> String {
    let since_start = timestamp - start_sec;
    let until_end = end_sec - timestamp;
    if since_start.abs() < 1.0 {
        "AtCut".to_string()
    } else if since_start < 3.0 {
        "EarlyScene".to_string()
    } else if until_end < 3.0 {
        "LateScene".to_string()
    } else {
        "MidScene".to_string()
    }
}

/// Write scene_scores.json to disk.
///
/// Called after scene detection + frame extraction. Produces a JSON file with:
/// - All scene boundaries with significance scores
/// - Per-frame scores mapping each extracted frame to its scene's significance
pub fn write_scene_scores(
    boundaries: &[SceneBoundary],
    frame_timestamps: &[(String, f64)], // (path, timestamp)
    duration: f64,
    fps: f64,
    detection_time_ms: u64,
    path: &Path,
) -> Result<()> {
    let scenes: Vec<SceneScoreEntry> = boundaries
        .iter()
        .enumerate()
        .map(|(i, b)| SceneScoreEntry {
            index: i,
            start_sec: b.start_sec,
            end_sec: b.end_sec,
            duration_sec: b.duration_sec,
            frame_start: b.frame_start,
            frame_end: b.frame_end,
            significance: b.significance(),
            scores: ScoreDetails {
                inter_cost: b.inter_cost.unwrap_or(0.0),
                imp_block_cost: b.imp_block_cost.unwrap_or(0.0),
                backward_adjusted_cost: b.backward_adjusted_cost.unwrap_or(0.0),
                forward_adjusted_cost: b.forward_adjusted_cost.unwrap_or(0.0),
                threshold: b.threshold.unwrap_or(0.0),
            },
        })
        .collect();

    let frame_scores: Vec<FrameScoreEntry> = frame_timestamps
        .iter()
        .map(|(_path, ts)| {
            let scene_idx = boundaries
                .iter()
                .position(|b| *ts >= b.start_sec && *ts < b.end_sec);
            let (significance, position) = scene_idx
                .map(|i| {
                    let b = &boundaries[i];
                    (
                        b.significance(),
                        classify_position_str(b.start_sec, b.end_sec, *ts),
                    )
                })
                .unwrap_or((0.0, "Unknown".to_string()));
            FrameScoreEntry {
                timestamp: *ts,
                scene_index: scene_idx.unwrap_or(0),
                significance,
                is_scene_cut: position == "AtCut",
                scene_position: position,
            }
        })
        .collect();

    let output = SceneScoresOutput {
        video_duration: duration,
        fps,
        total_scenes: boundaries.len(),
        detection_time_ms,
        scenes,
        frame_scores,
    };

    let json = serde_json::to_string_pretty(&output)
        .map_err(|e| WatchError::Ffmpeg(format!("Failed to serialize scene scores: {}", e)))?;
    std::fs::write(path, json)?;
    Ok(())
}

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
        start_sec: f64,
        end_sec: f64,
        _fps: f64,
        frame_start: u64,
        frame_end: u64,
        inter_cost: f64,
        imp_block_cost: f64,
        backward_adjusted_cost: f64,
        forward_adjusted_cost: f64,
        threshold: f64,
    ) -> Self {
        Self {
            start_sec,
            end_sec,
            duration_sec: end_sec - start_sec,
            frame_start,
            frame_end,
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
            return Err(WatchError::Ffmpeg(format!(
                "av-scenechange decoder init failed: {e}"
            )));
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
            return Err(WatchError::Ffmpeg(format!(
                "av-scenechange detection failed: {e}"
            )));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_boundary(start: f64, end: f64, significance: f64) -> SceneBoundary {
        let inter = significance * 0.25;
        let imp = significance * 0.07;
        let back = significance * 0.25;
        let fwd = significance * 0.25;
        SceneBoundary::with_score(
            start,
            end,
            24.0,
            (start * 24.0) as u64,
            (end * 24.0) as u64,
            inter,
            imp,
            back,
            fwd,
            0.15,
        )
    }

    #[test]
    fn test_write_scene_scores_basic() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scene_scores.json");

        let boundaries = vec![
            make_boundary(0.0, 10.0, 0.5),
            make_boundary(10.0, 25.0, 0.8),
            make_boundary(25.0, 40.0, 0.3),
        ];

        let frame_timestamps = vec![
            ("frame_0001.jpg".to_string(), 5.0),
            ("frame_0002.jpg".to_string(), 12.0),
            ("frame_0003.jpg".to_string(), 30.0),
        ];

        write_scene_scores(&boundaries, &frame_timestamps, 40.0, 24.0, 150, &path).unwrap();

        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        let output: SceneScoresOutput = serde_json::from_str(&content).unwrap();

        assert_eq!(output.total_scenes, 3);
        assert_eq!(output.frame_scores.len(), 3);
        assert_eq!(output.video_duration, 40.0);
        assert_eq!(output.detection_time_ms, 150);

        // First frame at t=5.0 should be in scene 0
        assert_eq!(output.frame_scores[0].scene_index, 0);
        assert!(output.frame_scores[0].significance > 0.0);

        // Second frame at t=12.0 should be in scene 1
        assert_eq!(output.frame_scores[1].scene_index, 1);

        // Third frame at t=30.0 should be in scene 2
        assert_eq!(output.frame_scores[2].scene_index, 2);
    }

    #[test]
    fn test_write_scene_scores_scene_positions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scene_scores.json");

        let boundaries = vec![make_boundary(0.0, 20.0, 0.6)];

        // Frame at start of scene (AtCut)
        // Frame in middle (MidScene)
        // Frame near end (LateScene)
        let frame_timestamps = vec![
            ("f1.jpg".to_string(), 0.5),  // AtCut
            ("f2.jpg".to_string(), 10.0), // MidScene
            ("f3.jpg".to_string(), 18.5), // LateScene
        ];

        write_scene_scores(&boundaries, &frame_timestamps, 20.0, 24.0, 0, &path).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let output: SceneScoresOutput = serde_json::from_str(&content).unwrap();

        assert_eq!(output.frame_scores[0].scene_position, "AtCut");
        assert!(output.frame_scores[0].is_scene_cut);

        assert_eq!(output.frame_scores[1].scene_position, "MidScene");
        assert!(!output.frame_scores[1].is_scene_cut);

        assert_eq!(output.frame_scores[2].scene_position, "LateScene");
        assert!(!output.frame_scores[2].is_scene_cut);
    }

    #[test]
    fn test_write_scene_scores_empty_frames() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scene_scores.json");

        let boundaries = vec![make_boundary(0.0, 10.0, 0.5)];

        let frame_timestamps: Vec<(String, f64)> = vec![];

        write_scene_scores(&boundaries, &frame_timestamps, 10.0, 24.0, 0, &path).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let output: SceneScoresOutput = serde_json::from_str(&content).unwrap();

        assert_eq!(output.total_scenes, 1);
        assert_eq!(output.frame_scores.len(), 0);
    }

    #[test]
    fn test_significance_calculation() {
        let b = SceneBoundary::with_score(0.0, 10.0, 24.0, 0, 240, 0.1, 0.05, 0.08, 0.12, 0.15);
        let sig = b.significance();
        // significance = inter_cost + imp_block_cost * 10 + backward + forward
        // = 0.1 + 0.05*10 + 0.08 + 0.12 = 0.1 + 0.5 + 0.08 + 0.12 = 0.8
        assert!((sig - 0.8).abs() < 0.001);
    }

    #[test]
    fn test_classify_position_str() {
        assert_eq!(classify_position_str(10.0, 30.0, 10.5), "AtCut");
        assert_eq!(classify_position_str(10.0, 30.0, 9.5), "AtCut");
        assert_eq!(classify_position_str(10.0, 30.0, 12.0), "EarlyScene");
        assert_eq!(classify_position_str(10.0, 30.0, 20.0), "MidScene");
        assert_eq!(classify_position_str(10.0, 30.0, 28.0), "LateScene");
    }

    #[test]
    fn classify_position_zero_duration() {
        // start == end == timestamp -> since_start = 0.0, |0.0| < 1.0 -> AtCut
        assert_eq!(classify_position_str(5.0, 5.0, 5.0), "AtCut");
    }

    #[test]
    fn significance_all_none() {
        let b = SceneBoundary::new(0.0, 10.0, 24.0, 0, 240);
        assert!((b.significance() - 0.0).abs() < 0.001);
    }

    #[test]
    fn significance_only_inter_cost() {
        let mut b = SceneBoundary::new(0.0, 10.0, 24.0, 0, 240);
        b.inter_cost = Some(0.42);
        assert!((b.significance() - 0.42).abs() < 0.001);
    }

    #[test]
    fn significance_all_scores_weighted_sum() {
        let b = SceneBoundary::with_score(
            0.0, 10.0, 24.0, 0, 240, 0.1,  // inter_cost
            0.05, // imp_block_cost
            0.08, // backward_adjusted_cost
            0.12, // forward_adjusted_cost
            0.15, // threshold (not in formula)
        );
        // inter + imp*10 + backward + forward = 0.1 + 0.5 + 0.08 + 0.12 = 0.8
        let sig = b.significance();
        assert!((sig - 0.8).abs() < 0.001);
    }

    #[test]
    fn timestamps_to_boundaries_empty() {
        let bounds = timestamps_to_boundaries(&[], 24.0, 60.0);
        assert!(bounds.is_empty());
    }

    #[test]
    fn timestamps_to_boundaries_single() {
        let bounds = timestamps_to_boundaries(&[5.0], 24.0, 10.0);
        assert_eq!(bounds.len(), 1);
        assert!((bounds[0].start_sec - 5.0).abs() < 0.001);
        assert!((bounds[0].end_sec - 10.0).abs() < 0.001);
        assert_eq!(bounds[0].frame_start, (5.0 * 24.0) as u64);
        assert_eq!(bounds[0].frame_end, (10.0 * 24.0) as u64);
    }

    #[test]
    fn timestamps_to_boundaries_multiple_sorted() {
        let bounds = timestamps_to_boundaries(&[2.0, 5.0, 8.0], 24.0, 10.0);
        assert_eq!(bounds.len(), 3);
        // First boundary: 2.0 -> 5.0
        assert!((bounds[0].start_sec - 2.0).abs() < 0.001);
        assert!((bounds[0].end_sec - 5.0).abs() < 0.001);
        // Second boundary: 5.0 -> 8.0
        assert!((bounds[1].start_sec - 5.0).abs() < 0.001);
        assert!((bounds[1].end_sec - 8.0).abs() < 0.001);
        // Third boundary: 8.0 -> duration (10.0)
        assert!((bounds[2].start_sec - 8.0).abs() < 0.001);
        assert!((bounds[2].end_sec - 10.0).abs() < 0.001);
        // Verify sorted ascending
        for i in 1..bounds.len() {
            assert!(bounds[i].start_sec >= bounds[i - 1].start_sec);
        }
    }

    // ─── T2: Filesystem-based tests for write_scene_scores ──────────────────

    #[test]
    fn test_write_scene_scores_valid_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scene_scores.json");

        let boundaries = vec![
            make_boundary(0.0, 15.0, 0.6),
            make_boundary(15.0, 35.0, 0.9),
        ];

        let frame_timestamps = vec![
            ("frame_0001.jpg".to_string(), 7.5),
            ("frame_0002.jpg".to_string(), 25.0),
        ];

        write_scene_scores(&boundaries, &frame_timestamps, 35.0, 24.0, 200, &path).unwrap();

        // Verify file exists on disk
        assert!(path.exists(), "JSON file should be created on disk");

        // Parse it back and verify structure
        let content = std::fs::read_to_string(&path).unwrap();
        let output: SceneScoresOutput = serde_json::from_str(&content).unwrap();

        assert_eq!(output.video_duration, 35.0);
        assert_eq!(output.fps, 24.0);
        assert_eq!(output.total_scenes, 2);
        assert_eq!(output.detection_time_ms, 200);
        assert_eq!(output.scenes.len(), 2);
        assert_eq!(output.frame_scores.len(), 2);

        // Verify scene content round-trips correctly
        assert!((output.scenes[0].start_sec - 0.0).abs() < 0.001);
        assert!((output.scenes[0].end_sec - 15.0).abs() < 0.001);
        assert!((output.scenes[1].start_sec - 15.0).abs() < 0.001);
        assert!((output.scenes[1].end_sec - 35.0).abs() < 0.001);

        // Verify frame_scores point to correct scenes
        assert_eq!(output.frame_scores[0].scene_index, 0);
        assert_eq!(output.frame_scores[1].scene_index, 1);
    }

    #[test]
    fn test_write_scene_scores_empty_boundaries() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scene_scores.json");

        let boundaries: Vec<SceneBoundary> = vec![];
        let frame_timestamps: Vec<(String, f64)> = vec![("orphan.jpg".to_string(), 5.0)];

        write_scene_scores(&boundaries, &frame_timestamps, 60.0, 30.0, 0, &path).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let output: SceneScoresOutput = serde_json::from_str(&content).unwrap();

        // Empty boundaries → empty scenes array
        assert!(
            output.scenes.is_empty(),
            "scenes array should be empty when no boundaries"
        );
        assert_eq!(output.total_scenes, 0);
        // Frame with no matching boundary gets Unknown position
        assert_eq!(output.frame_scores.len(), 1);
        assert_eq!(output.frame_scores[0].scene_position, "Unknown");
    }

    #[test]
    fn test_write_scene_scores_json_has_required_keys() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scene_scores.json");

        let boundaries = vec![make_boundary(0.0, 10.0, 0.5)];
        let frame_timestamps = vec![("f.jpg".to_string(), 5.0)];

        write_scene_scores(&boundaries, &frame_timestamps, 10.0, 24.0, 50, &path).unwrap();

        // Read raw JSON to verify top-level keys are present
        let raw: serde_json::Value = {
            let content = std::fs::read_to_string(&path).unwrap();
            serde_json::from_str(&content).unwrap()
        };

        assert!(
            raw.get("video_duration").is_some(),
            "JSON must contain 'video_duration'"
        );
        assert!(raw.get("fps").is_some(), "JSON must contain 'fps'");
        assert!(raw.get("scenes").is_some(), "JSON must contain 'scenes'");
        assert!(raw["scenes"].is_array(), "'scenes' must be an array");
        assert!(
            raw.get("frame_scores").is_some(),
            "JSON must contain 'frame_scores'"
        );
        assert!(
            raw["frame_scores"].is_array(),
            "'frame_scores' must be an array"
        );
    }

    // ─── T2: Tests for SceneBoundary ────────────────────────────────────────

    #[test]
    fn test_boundary_new_duration_calculation() {
        let b = SceneBoundary::new(5.0, 20.0, 24.0, 120, 480);
        assert!(
            (b.duration_sec - 15.0).abs() < 0.001,
            "duration_sec should equal end_sec - start_sec"
        );

        // All score fields should be None
        assert!(b.inter_cost.is_none());
        assert!(b.imp_block_cost.is_none());
        assert!(b.backward_adjusted_cost.is_none());
        assert!(b.forward_adjusted_cost.is_none());
        assert!(b.threshold.is_none());

        // Boundary values
        assert!((b.start_sec - 5.0).abs() < 0.001);
        assert!((b.end_sec - 20.0).abs() < 0.001);
        assert_eq!(b.frame_start, 120);
        assert_eq!(b.frame_end, 480);
    }

    #[test]
    fn test_boundary_with_score_populates_all_fields() {
        let b = SceneBoundary::with_score(
            10.0, 30.0, 24.0, 240, 720, 0.1,  // inter_cost
            0.05, // imp_block_cost
            0.08, // backward_adjusted_cost
            0.12, // forward_adjusted_cost
            0.15, // threshold
        );

        assert!((b.duration_sec - 20.0).abs() < 0.001);
        assert!((b.inter_cost.unwrap() - 0.1).abs() < 0.001);
        assert!((b.imp_block_cost.unwrap() - 0.05).abs() < 0.001);
        assert!((b.backward_adjusted_cost.unwrap() - 0.08).abs() < 0.001);
        assert!((b.forward_adjusted_cost.unwrap() - 0.12).abs() < 0.001);
        assert!((b.threshold.unwrap() - 0.15).abs() < 0.001);
    }

    #[test]
    fn test_total_scenes_on_detection_result() {
        // Empty boundaries → 0
        let result_empty = SceneDetectionResult {
            boundaries: vec![],
            fps: 24.0,
            detection_time_ms: 0,
        };
        assert_eq!(result_empty.total_scenes(), 0);

        // Multiple boundaries → correct count
        let result_multi = SceneDetectionResult {
            boundaries: vec![
                make_boundary(0.0, 5.0, 0.3),
                make_boundary(5.0, 12.0, 0.7),
                make_boundary(12.0, 20.0, 0.5),
            ],
            fps: 24.0,
            detection_time_ms: 100,
        };
        assert_eq!(result_multi.total_scenes(), 3);
    }
}
