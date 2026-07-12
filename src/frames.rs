use std::path::Path;
use crate::error::{WatchError, Result};
use crate::output::FrameInfo;

const MAX_FPS: f32 = 2.0;
const MAX_READ_DIMENSION: u32 = 1998;
pub const SCENE_MIN_FRAMES: usize = 8;
pub const KEYFRAME_MIN: usize = 4;

pub struct VideoMetadata {
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    pub codec: String,
}

pub struct FrameMeta {
    pub engine: String,
    pub candidate_count: usize,
    pub selected_count: usize,
    pub deduped_count: u32,
    pub fallback: bool,
    pub dropped_out_of_window: usize,
}

pub fn get_metadata(video_path: &Path) -> Result<VideoMetadata> {
    // Use ffprobe via subprocess (reliable, matches Python version)
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

    // Validate duration
    if duration <= 0.0 {
        return Err(WatchError::Ffmpeg(format!(
            "Video has zero or negative duration ({:.2}s) — file may be corrupt or not a valid video: {}",
            duration, video_str)));
    }
    if duration < 1.0 {
        eprintln!("[watch-rs] warning: very short video ({:.2}s), frame extraction may produce few or no frames", duration);
    }
    
    Ok(VideoMetadata {
        duration,
        width: video_stream.and_then(|s| s["width"].as_u64()).unwrap_or(0) as u32,
        height: video_stream.and_then(|s| s["height"].as_u64()).unwrap_or(0) as u32,
        codec: video_stream.and_then(|s| s["codec_name"].as_str()).unwrap_or("unknown").to_string(),
    })
}

pub fn auto_fps(duration: f64, max_frames: u32) -> f32 {
    if duration <= 0.0 {
        return MAX_FPS;
    }
    let raw_fps = max_frames as f32 / duration as f32;
    raw_fps.min(MAX_FPS)
}

pub fn auto_fps_focus(duration: f64, max_frames: u32) -> f32 {
    if duration <= 0.0 {
        return MAX_FPS;
    }
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
    let video_str = video_path.to_str().unwrap_or("<invalid path>");
    let filter = format!("fps={},scale=w='min({},iw)':h='min({},ih)':force_original_aspect_ratio=decrease:force_divisible_by=2",
        fps, resolution, MAX_READ_DIMENSION);
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

/// Pick `n` evenly-spaced items from a slice (always first + last).
fn even_indices(count: usize, n: usize) -> Vec<usize> {
    if n >= count {
        return (0..count).collect();
    }
    if n <= 1 {
        return vec![0];
    }
    (0..n).map(|i| (i * (count - 1) / (n - 1)) as usize).collect()
}

fn even_sample(frames: &mut Vec<FrameInfo>, cap: usize) {
    let count = frames.len();
    if cap >= count || count == 0 {
        return;
    }
    let indices: std::collections::HashSet<usize> = even_indices(count, cap).into_iter().collect();
    let mut removed_paths = Vec::new();
    let mut i = 0;
    frames.retain(|f| {
        let keep = indices.contains(&i);
        if !keep {
            removed_paths.push(f.path.clone());
        }
        i += 1;
        keep
    });
    // Delete dropped JPEGs from disk
    for p in &removed_paths {
        let _ = std::fs::remove_file(p);
    }
}

/// Extract only keyframes (I-frames) using ffmpeg's `-skip_frame nokey`.
///
/// Falls back to uniform `extract_frames` if fewer than `KEYFRAME_MIN`
/// keyframes are found.
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

    let scale_filter = format!(
        "scale=w='min({resolution},iw)':h='min({MAX_READ_DIMENSION},ih)':force_original_aspect_ratio=decrease:force_divisible_by=2"
    );
    let vf = format!("{scale_filter},showinfo");
    let output_pattern = out_dir.join("frame_%04d.jpg").to_string_lossy().to_string();

    // Build ffmpeg command: -skip_frame nokey extracts only I-frames
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

    // Parse timestamps from showinfo stderr (same pattern as scene.rs)
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
        // Clean up keyframe files
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

fn scale_filter(resolution: u32) -> String {
    format!(
        "scale=w='min({resolution},iw)':h='min({MAX_READ_DIMENSION},ih)':force_original_aspect_ratio=decrease:force_divisible_by=2"
    )
}

/// Extract exactly one frame at each requested timestamp (transcript cues).
///
/// Timestamps are absolute source seconds. Any falling outside an active
/// `[start, end]` focus window are dropped. Files use a `cue_*.jpg` prefix
/// so they sit alongside detail-engine `frame_*.jpg` output without clobbering.
/// When more cues than `max_frames` survive, they are even-sampled (first + last
/// kept) before extraction.
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

/// Extract frames at detected scene-change timestamps.
///
/// Calls `detect_scene_changes()` to find shot boundaries, then extracts one
/// frame per boundary.  Falls back to uniform `extract_frames()` when the
/// video has fewer than `SCENE_MIN_FRAMES` detected scenes (effectively static).
pub fn extract_scene_or_uniform(
    video_path: &Path,
    out_dir: &Path,
    fps: f32,
    target_frames: u32,
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

    // Detect scene changes (uncapped — covers entire clip)
    let timestamps = crate::scene::detect_scene_changes(video_path)?;

    if timestamps.len() >= SCENE_MIN_FRAMES {
        // --- Scene path: extract one frame per detected cut ---
        let scale_filter = format!(
            "scale=w='min({resolution},iw)':h='min({MAX_READ_DIMENSION},ih)':force_original_aspect_ratio=decrease:force_divisible_by=2"
        );

        let mut candidates: Vec<FrameInfo> = Vec::new();
        for (i, &ts) in timestamps.iter().enumerate() {
            let frame_path = out_dir.join(format!("frame_{:04}.jpg", i + 1));
            let frame_str = frame_path.to_string_lossy().to_string();

            let mut cmd = std::process::Command::new("ffmpeg");
            cmd.args(["-hide_banner", "-loglevel", "error", "-y"]);
            cmd.args(["-ss", &format!("{ts:.3}")]);
            cmd.args(["-i", video_str]);
            cmd.args(["-frames:v", "1"]);
            cmd.args(["-vf", &scale_filter]);
            cmd.args(["-q:v", "4"]);
            cmd.arg(&frame_str);

            let status = cmd.status().map_err(|e| {
                WatchError::Ffmpeg(format!(
                    "ffmpeg scene frame extraction failed for '{}' at ts={:.3}: {}",
                    video_str, ts, e
                ))
            })?;

            if !status.success() {
                // Skip individual failures — don't abort the whole extraction
                eprintln!(
                    "[watch-rs] warning: ffmpeg failed to extract scene frame at ts={:.3}",
                    ts
                );
                let _ = std::fs::remove_file(&frame_path);
                continue;
            }

            candidates.push(FrameInfo {
                path: frame_str,
                timestamp: ts,
                reason: if i == 0 {
                    "first-frame".to_string()
                } else {
                    "scene-change".to_string()
                },
            });
        }

        let candidate_count = candidates.len();
        let mut deduped_count = 0u32;

        if dedup {
            deduped_count = crate::dedup::dedup_frames(&mut candidates);
        }

        // Even-sample down to cap
        let cap = max_frames as usize;
        even_sample(&mut candidates, cap);

        let meta = FrameMeta {
            engine: "scene".to_string(),
            candidate_count,
            selected_count: candidates.len(),
            deduped_count,
            fallback: false,
            dropped_out_of_window: 0,
        };
        return Ok((candidates, meta));
    }

    // --- Fallback: uniform FPS extraction ---
    let meta_video = get_metadata(video_path)?;
    let eff_start = start_seconds.unwrap_or(0.0);
    let eff_end = end_seconds.unwrap_or(meta_video.duration);
    let eff_duration = (eff_end - eff_start).max(0.0);
    let budget = target_frames.min(max_frames);
    let effective_fps = if eff_duration > 0.0 {
        let raw = budget as f32 / eff_duration as f32;
        raw.min(MAX_FPS)
    } else {
        fps
    };

    let mut frames_out = extract_frames(video_path, out_dir, effective_fps, resolution, budget)?;
    let mut deduped_count = 0u32;
    if dedup {
        deduped_count = crate::dedup::dedup_frames(&mut frames_out);
    }

    let meta = FrameMeta {
        engine: "uniform".to_string(),
        candidate_count: timestamps.len(),
        selected_count: frames_out.len(),
        deduped_count,
        fallback: true,
        dropped_out_of_window: 0,
    };
    Ok((frames_out, meta))
}
