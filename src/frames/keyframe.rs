use std::path::Path;
use crate::error::{WatchError, Result};
use crate::output::FrameInfo;
use super::{FrameMeta, MAX_READ_DIMENSION, KEYFRAME_MIN, auto_fps, even_sample, get_metadata, extract_frames};

pub fn extract_keyframes(
    video_path: &Path,
    out_dir: &Path,
    resolution: u32,
    max_frames: u32,
    start_seconds: Option<f64>,
    end_seconds: Option<f64>,
    dedup: bool,
) -> Result<(Vec<FrameInfo>, FrameMeta)> {
    let video_str = video_path.to_str().unwrap_or("<invalid path>");

    std::fs::create_dir_all(out_dir)?;

    // Clean previous frames
    if out_dir.exists() {
        for entry in std::fs::read_dir(out_dir)? {
            let entry = entry?;
            let p = entry.path();
            if p.extension().map_or(false, |e| e == "jpg") {
                let _ = std::fs::remove_file(&p);
            }
        }
    }

    let max_dim = MAX_READ_DIMENSION;
    let scale_filter = format!(
        "scale=w='min({resolution},iw)':h='min({max_dim},ih)':force_original_aspect_ratio=decrease:force_divisible_by=2"
    );
    let vf = format!("{scale_filter},showinfo");
    let output_pattern = out_dir.join("frame_%04d.jpg").to_string_lossy().to_string();

    let mut cmd = std::process::Command::new("ffmpeg");
    cmd.args(["-hide_banner", "-loglevel", "info", "-y"]);
    if let Some(start) = start_seconds {
        cmd.args(["-ss", &format!("{start:.3}")]);
    }
    if let Some(end) = end_seconds {
        cmd.args(["-to", &format!("{end:.3}")]);
    }
    cmd.args(["-skip_frame", "nokey"]);
    cmd.args(["-i", video_str]);
    cmd.args(["-vf", &vf]);
    cmd.args(["-vsync", "vfr"]);
    cmd.args(["-q:v", "4"]);
    cmd.arg(&output_pattern);

    let output = cmd.output().map_err(|e| {
        WatchError::Ffmpeg(format!(
            "ffmpeg not found or failed to execute for '{}': {}",
            video_str, e
        ))
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(WatchError::Ffmpeg(format!(
            "ffmpeg keyframe extraction failed for '{}' (exit code {:?}): {}",
            video_str,
            output.status.code(),
            stderr.trim()
        )));
    }

    // Parse timestamps from showinfo stderr
    let stderr = String::from_utf8_lossy(&output.stderr);
    let offset = start_seconds.unwrap_or(0.0);
    let mut timestamps: Vec<f64> = Vec::new();
    for line in stderr.lines() {
        if let Some(pos) = line.find("pts_time:") {
            let rest = &line[pos + 10..];
            if let Some(end) = rest.find(' ') {
                if let Ok(ts) = rest[..end].parse::<f64>() {
                    timestamps.push(((offset + ts) * 100.0).round() / 100.0);
                }
            }
        }
    }

    // Collect extracted frame files
    let mut frame_files: Vec<std::path::PathBuf> = Vec::new();
    if out_dir.exists() {
        for entry in std::fs::read_dir(out_dir)? {
            let entry = entry?;
            let p = entry.path();
            if p.extension().map_or(false, |e| e == "jpg") {
                frame_files.push(p);
            }
        }
    }
    frame_files.sort();

    let mut candidates: Vec<FrameInfo> = Vec::new();
    for (i, path) in frame_files.iter().enumerate() {
        let ts = timestamps.get(i).copied().unwrap_or(offset);
        candidates.push(FrameInfo {
            path: path.to_string_lossy().to_string(),
            timestamp: ts,
            reason: "keyframe".to_string(),
        });
    }

    let candidate_count = candidates.len();

    // Too few keyframes → fall back to uniform extraction
    if candidate_count < KEYFRAME_MIN {
        for c in &candidates {
            let _ = std::fs::remove_file(&c.path);
        }

        let meta = get_metadata(video_path)?;
        let eff_start = start_seconds.unwrap_or(0.0);
        let eff_end = end_seconds.unwrap_or(meta.duration);
        let eff_duration = (eff_end - eff_start).max(0.0);
        let budget = max_frames;
        let fps = auto_fps(eff_duration, budget);

        let mut frames_out = extract_frames(video_path, out_dir, fps, resolution, budget)?;
        let mut deduped_count = 0u32;
        if dedup {
            deduped_count = crate::dedup::dedup_frames(&mut frames_out);
        }

        let meta = FrameMeta {
            engine: "uniform".to_string(),
            candidate_count,
            selected_count: frames_out.len(),
            deduped_count,
            fallback: true,
            dropped_out_of_window: 0,
        };
        return Ok((frames_out, meta));
    }

    // Dedup if requested
    let mut deduped_count = 0u32;
    if dedup {
        deduped_count = crate::dedup::dedup_frames(&mut candidates);
    }

    // Even-sample down to cap
    let cap = max_frames as usize;
    even_sample(&mut candidates, cap);

    let meta = FrameMeta {
        engine: "keyframe".to_string(),
        candidate_count,
        selected_count: candidates.len(),
        deduped_count,
        fallback: false,
        dropped_out_of_window: 0,
    };

    Ok((candidates, meta))
}
