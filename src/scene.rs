use std::path::Path;
use crate::error::{WatchError, Result};

pub fn detect_scene_changes(video_path: &Path) -> Result<Vec<f64>> {
    // Use ffmpeg's scdet filter via subprocess
    // This is more reliable than av-scenechange for FFmpeg 6.x
    let output = std::process::Command::new("ffmpeg")
        .args([
            "-i", video_path.to_str().unwrap(),
            "-vf", "select='gt(scene,0.20)',showinfo",
            "-vsync", "vfr",
            "-f", "null",
            "-",
        ])
        .output();
    
    match output {
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            let mut timestamps = Vec::new();
            for line in stderr.lines() {
                if let Some(pos) = line.find("pts_time:") {
                    let rest = &line[pos + 10..];
                    if let Some(end) = rest.find(' ') {
                        if let Ok(ts) = rest[..end].parse::<f64>() {
                            timestamps.push(ts);
                        }
                    }
                }
            }
            Ok(timestamps)
        }
        Err(e) => Err(WatchError::Ffmpeg(format!("Scene detection failed: {}", e))),
    }
}
