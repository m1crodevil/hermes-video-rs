use std::path::Path;
use crate::error::{WatchError, Result};
use crate::output::FrameInfo;
use super::{FrameMeta, MAX_FPS, MAX_READ_DIMENSION, SCENE_MIN_FRAMES, even_sample, extract_frames, get_metadata, score_based_select};

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
    scene_boundaries: Option<&[crate::scene_detect::SceneBoundary]>,
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

    // Detect scene changes using adaptive threshold based on video duration
    let meta_video = get_metadata(video_path)?;
    let threshold = crate::scene::adaptive_threshold(meta_video.duration);
    let timestamps = crate::scene::detect_scene_changes(video_path, threshold)?;

    if timestamps.len() >= SCENE_MIN_FRAMES {
        // --- Scene path: extract one frame per detected cut ---
        let sf = format!(
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
            cmd.args(["-vf", &sf]);
            cmd.args(["-q:v", "4"]);
            cmd.arg(&frame_str);

            let status = cmd.status().map_err(|e| {
                WatchError::Ffmpeg(format!(
                    "ffmpeg scene frame extraction failed for '{}' at ts={:.3}: {}",
                    video_str, ts, e
                ))
            })?;

            if !status.success() {
                eprintln!(
                    "[watch2] warning: ffmpeg failed to extract scene frame at ts={:.3}",
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

        // Fill gaps between scene frames with uniform fill
        candidates = super::gap_fill::fill_gaps_with_uniform(
            &candidates, video_path, out_dir, resolution,
            meta_video.duration, target_frames as usize,
        )?;

        // Score-based selection if av-scenechange boundaries available, else even-sample
        let cap = max_frames as usize;
        if let Some(boundaries) = scene_boundaries {
            let scores: Vec<f64> = candidates.iter().map(|f| {
                boundaries.iter()
                    .find(|b| f.timestamp >= b.start_sec && f.timestamp < b.end_sec)
                    .map(|b| b.significance())
                    .unwrap_or(0.0)
            }).collect();
            super::score_based_select(&mut candidates, cap, &scores);
        } else {
            even_sample(&mut candidates, cap);
        }

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
