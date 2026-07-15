use std::path::Path;
use crate::error::{WatchError, Result};
use crate::output::FrameInfo;
use super::{MAX_READ_DIMENSION};

pub fn extract_frames(
    video_path: &Path,
    out_dir: &Path,
    fps: f32,
    resolution: u32,
    max_frames: u32,
) -> Result<Vec<FrameInfo>> {
    std::fs::create_dir_all(out_dir)?;
    let video_str = video_path.to_str().unwrap_or("<invalid path>");
    let filter = format!(
        "fps={},scale=w='min({},iw)':h='min({},ih)':force_original_aspect_ratio=decrease:force_divisible_by=2",
        fps, resolution, MAX_READ_DIMENSION
    );
    let output_pattern = out_dir.join("frame_%04d.jpg").to_string_lossy().to_string();

    let status = std::process::Command::new("ffmpeg")
        .args([
            "-i", video_str,
            "-vf", &filter,
            "-q:v", "2",
            "-frames:v", &max_frames.to_string(),
            "-y",
            &output_pattern,
        ])
        .status()
        .map_err(|e| WatchError::Ffmpeg(format!("ffmpeg not found or failed to execute for '{}': {}", video_str, e)))?;

    if !status.success() {
        return Err(WatchError::Ffmpeg(format!(
            "ffmpeg frame extraction failed for '{}' (exit code: {:?}). Check that the file is a valid video and ffmpeg supports its codec.",
            video_str, status.code())));
    }

    let mut frames = Vec::new();
    for entry in std::fs::read_dir(out_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().map_or(false, |e| e == "jpg") {
            let filename = path.file_stem().unwrap().to_string_lossy();
            let idx: u32 = filename.replace("frame_", "").parse().unwrap_or(0);
            let timestamp = (idx - 1) as f64 / fps as f64;
            frames.push(FrameInfo {
                path: path.to_string_lossy().to_string(),
                timestamp,
                reason: "uniform".to_string(),
            });
        }
    }
    frames.sort_by(|a, b| a.timestamp.partial_cmp(&b.timestamp).unwrap());
    Ok(frames)
}
