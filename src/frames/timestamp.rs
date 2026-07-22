use std::path::Path;
use crate::error::{WatchError, Result};
use crate::output::FrameInfo;
use super::{FrameMeta, even_indices, scale_filter};

pub fn extract_at_timestamps(
    video_path: &Path,
    out_dir: &Path,
    timestamps: &[f64],
    resolution: u32,
    max_frames: Option<u32>,
    start_seconds: Option<f64>,
    end_seconds: Option<f64>,
) -> Result<(Vec<FrameInfo>, FrameMeta)> {
    let video_str = video_path.to_str().unwrap_or("<invalid path>");

    std::fs::create_dir_all(out_dir)?;
    // Clean previous cue files
    if out_dir.exists() {
        for entry in std::fs::read_dir(out_dir)? {
            let entry = entry?;
            let p = entry.path();
            if p.file_stem().map_or(false, |s| {
                s.to_string_lossy().starts_with("cue_")
            }) {
                let _ = std::fs::remove_file(&p);
            }
        }
    }

    let lo = start_seconds.unwrap_or(0.0);
    let hi = end_seconds.unwrap_or(f64::INFINITY);

    // Sort, dedup, and round timestamps to 2 decimal places
    let mut requested: Vec<f64> = timestamps.iter().map(|t| (t * 100.0).round() / 100.0).collect();
    requested.sort_by(|a, b| a.partial_cmp(b).unwrap());
    requested.dedup_by(|a, b| (*a - *b).abs() < f64::EPSILON);

    let candidate_count = requested.len();

    // Filter to focus window
    let in_window: Vec<f64> = requested.iter().copied().filter(|t| *t >= lo && *t <= hi).collect();
    let dropped_out_of_window = candidate_count - in_window.len();

    // Even-sample if over max_frames cap
    let points = if let Some(cap) = max_frames {
        let cap = cap as usize;
        if cap > 0 && in_window.len() > cap {
            let indices = even_indices(in_window.len(), cap);
            indices.into_iter().map(|i| in_window[i]).collect()
        } else {
            in_window
        }
    } else {
        in_window
    };

    let vf = scale_filter(resolution);
    let mut frames: Vec<FrameInfo> = Vec::new();

    for (i, &t) in points.iter().enumerate() {
        let frame_path = out_dir.join(format!("cue_{:04}.jpg", i));
        let frame_str = frame_path.to_string_lossy().to_string();

        let mut cmd = std::process::Command::new("ffmpeg");
        cmd.args(["-hide_banner", "-loglevel", "error", "-y"]);
        cmd.args(["-ss", &format!("{t:.3}")]);
        cmd.args(["-i", video_str]);
        cmd.args(["-frames:v", "1"]);
        cmd.args(["-vf", &vf]);
        cmd.args(["-q:v", "4"]);
        cmd.arg(&frame_str);

        let status = cmd.status().map_err(|e| {
            WatchError::Ffmpeg(format!(
                "ffmpeg cue frame extraction failed for '{}' at ts={:.3}: {}",
                video_str, t, e
            ))
        })?;

        if status.success() && frame_path.exists() {
            frames.push(FrameInfo {
                path: frame_str,
                timestamp: t,
                reason: "transcript-cue".to_string(),
                scene_score: None,
            });
        }
    }

    let meta = FrameMeta {
        engine: "timestamps".to_string(),
        candidate_count,
        selected_count: frames.len(),
        deduped_count: 0,
        fallback: false,
        dropped_out_of_window,
    };

    Ok((frames, meta))
}
