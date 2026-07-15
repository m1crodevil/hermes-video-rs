use std::path::Path;
use crate::error::{WatchError, Result};
use super::VideoMetadata;

pub fn get_metadata(video_path: &Path) -> Result<VideoMetadata> {
    let video_str = video_path.to_str().unwrap_or("<invalid path>");
    let output = std::process::Command::new("ffprobe")
        .args(["-v", "quiet", "-print_format", "json", "-show_format", "-show_streams",
               video_str])
        .output()
        .map_err(|e| WatchError::Ffmpeg(format!("ffprobe not found or failed to execute for '{}': {}", video_str, e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(WatchError::Ffmpeg(format!(
            "ffprobe failed for '{}': exit code {:?}, stderr: {}",
            video_str, output.status.code(), stderr.trim())));
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| {
            let preview = String::from_utf8_lossy(&output.stdout);
            let snippet = if preview.len() > 200 { &preview[..200] } else { &preview };
            WatchError::Ffmpeg(format!(
                "ffprobe returned invalid JSON for '{}': {} (stdout preview: {:?})",
                video_str, e, snippet))
        })?;

    let empty_streams: Vec<serde_json::Value> = vec![];
    let streams = json["streams"].as_array().unwrap_or(&empty_streams);
    let fmt = &json["format"];
    let video_stream = streams.iter().find(|s| s["codec_type"].as_str() == Some("video"));

    let duration = fmt["duration"].as_f64().unwrap_or(
        video_stream.and_then(|s| s["duration"].as_f64()).unwrap_or(0.0));

    if duration <= 0.0 {
        return Err(WatchError::Ffmpeg(format!(
            "Video has zero or negative duration ({:.2}s) — file may be corrupt or not a valid video: {}",
            duration, video_str)));
    }
    if duration < 1.0 {
        eprintln!("[watch2] warning: very short video ({:.2}s), frame extraction may produce few or no frames", duration);
    }

    Ok(VideoMetadata {
        duration,
        width: video_stream.and_then(|s| s["width"].as_u64()).unwrap_or(0) as u32,
        height: video_stream.and_then(|s| s["height"].as_u64()).unwrap_or(0) as u32,
        codec: video_stream.and_then(|s| s["codec_name"].as_str()).unwrap_or("unknown").to_string(),
    })
}
