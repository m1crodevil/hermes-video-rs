use std::path::Path;
use crate::error::{WatchError, Result};
use crate::output::FrameInfo;

const MAX_FPS: f32 = 2.0;
const MAX_READ_DIMENSION: u32 = 1998;

pub struct VideoMetadata {
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    pub codec: String,
}

pub fn get_metadata(video_path: &Path) -> Result<VideoMetadata> {
    // Use ffprobe via subprocess (reliable, matches Python version)
    let output = std::process::Command::new("ffprobe")
        .args(["-v", "quiet", "-print_format", "json", "-show_format", "-show_streams",
               video_path.to_str().unwrap()])
        .output()
        .map_err(|e| WatchError::Ffmpeg(format!("ffprobe not found: {}", e)))?;
    
    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| WatchError::Ffmpeg(format!("ffprobe parse error: {}", e)))?;
    
    let empty_streams: Vec<serde_json::Value> = vec![];
    let streams = json["streams"].as_array().unwrap_or(&empty_streams);
    let fmt = &json["format"];
    let video_stream = streams.iter().find(|s| s["codec_type"].as_str() == Some("video"));
    
    let duration = fmt["duration"].as_f64().unwrap_or(
        video_stream.and_then(|s| s["duration"].as_f64()).unwrap_or(0.0));
    
    Ok(VideoMetadata {
        duration,
        width: video_stream.and_then(|s| s["width"].as_u64()).unwrap_or(0) as u32,
        height: video_stream.and_then(|s| s["height"].as_u64()).unwrap_or(0) as u32,
        codec: video_stream.and_then(|s| s["codec_name"].as_str()).unwrap_or("unknown").to_string(),
    })
}

pub fn auto_fps(duration: f64, max_frames: u32) -> f32 {
    let raw_fps = max_frames as f32 / duration as f32;
    raw_fps.min(MAX_FPS)
}

pub fn auto_fps_focus(duration: f64, max_frames: u32) -> f32 {
    let raw_fps = max_frames as f32 / duration as f32;
    raw_fps.min(MAX_FPS)
}

pub fn extract_frames (
    video_path: &Path,
    out_dir: &Path,
    fps: f32,
    resolution: u32,
    max_frames: u32,
) -> Result<Vec<FrameInfo>> {
    std::fs::create_dir_all(out_dir)?;
    let filter = format!("fps={},scale=w='min({},iw)':h='min({},ih)':force_original_aspect_ratio=decrease:force_divisible_by=2",
        fps, resolution, MAX_READ_DIMENSION);
    let output_pattern = out_dir.join("frame_%04d.jpg").to_string_lossy().to_string();
    
    let status = std::process::Command::new("ffmpeg")
        .args([
            "-i", video_path.to_str().unwrap(),
            "-vf", &filter,
            "-q:v", "2",
            "-frames:v", &max_frames.to_string(),
            "-y",
            &output_pattern,
        ])
        .status()
        .map_err(|e| WatchError::Ffmpeg(format!("ffmpeg not found: {}", e)))?;
    
    if !status.success() {
        return Err(WatchError::Ffmpeg("ffmpeg frame extraction failed".into()));
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
