pub mod metadata;
pub mod uniform;
pub mod keyframe;
pub mod timestamp;
pub mod scene;
pub mod two_pass;
pub mod gap_fill;

use crate::output::FrameInfo;

pub const MAX_FPS: f32 = 2.0;
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

pub use metadata::get_metadata;
pub use uniform::extract_frames;
pub use keyframe::extract_keyframes;
pub use timestamp::extract_at_timestamps;
pub use scene::extract_scene_or_uniform;
pub use two_pass::extract_two_pass;

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

/// Pick `n` evenly-spaced items from a slice (always first + last).
pub(crate) fn even_indices(count: usize, n: usize) -> Vec<usize> {
    if n >= count {
        return (0..count).collect();
    }
    if n <= 1 {
        return vec![0];
    }
    (0..n).map(|i| (i * (count - 1) / (n - 1)) as usize).collect()
}

pub(crate) fn even_sample(frames: &mut Vec<FrameInfo>, cap: usize) {
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
    for p in &removed_paths {
        let _ = std::fs::remove_file(p);
    }
}

pub(crate) fn scale_filter(resolution: u32) -> String {
    format!(
        "scale=w='min({resolution},iw)':h='min({MAX_READ_DIMENSION},ih)':force_original_aspect_ratio=decrease:force_divisible_by=2"
    )
}
