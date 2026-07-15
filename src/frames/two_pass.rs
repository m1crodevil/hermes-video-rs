use std::path::Path;
use crate::error::Result;
use crate::output::FrameInfo;
use super::{FrameMeta, get_metadata, extract_frames, extract_at_timestamps};

pub fn extract_two_pass(
    video_path: &Path,
    out_dir: &Path,
    fps: f32,
    target_frames: u32,
    resolution: u32,
    start_seconds: Option<f64>,
    end_seconds: Option<f64>,
    dedup: bool,
) -> Result<(Vec<FrameInfo>, FrameMeta)> {
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

    // PASS 1: Scene detection (uncapped)
    let metadata = get_metadata(video_path)?;
    let threshold = crate::scene::adaptive_threshold(metadata.duration);
    let scene_timestamps = crate::scene::detect_scene_changes(video_path, threshold)?;

    let mut scene_frames: Vec<FrameInfo> = Vec::new();
    if !scene_timestamps.is_empty() {
        let (frames, _) = extract_at_timestamps(
            video_path, out_dir, &scene_timestamps, resolution, None,
            start_seconds, end_seconds,
        )?;
        scene_frames = frames;
    }

    // PASS 2: Uniform gap-filling at 50% density
    let fill_fps = fps * 0.5;
    let fill_target = (target_frames / 2).max(1);
    let uniform_frames = extract_frames(
        video_path, out_dir, fill_fps, resolution, fill_target,
    )?;

    // Merge by timestamp
    let mut all: Vec<FrameInfo> = scene_frames.into_iter()
        .chain(uniform_frames.into_iter())
        .collect();
    all.sort_by(|a, b| a.timestamp.partial_cmp(&b.timestamp).unwrap());

    // Dedup
    let mut deduped_count = 0u32;
    if dedup {
        deduped_count = crate::dedup::dedup_frames(&mut all);
    }

    let selected = all.len();
    let meta = FrameMeta {
        engine: "two-pass".to_string(),
        candidate_count: selected,
        selected_count: selected,
        deduped_count,
        fallback: false,
        dropped_out_of_window: 0,
    };

    Ok((all, meta))
}
