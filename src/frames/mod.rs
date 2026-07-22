pub mod metadata;
pub mod timestamp;

pub const MAX_FPS: f32 = 2.0;
const MAX_READ_DIMENSION: u32 = 1998;

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
pub use timestamp::extract_at_timestamps;

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

pub(crate) fn scale_filter(resolution: u32) -> String {
    format!(
        "scale=w='min({resolution},iw)':h='min({MAX_READ_DIMENSION},ih)':force_original_aspect_ratio=decrease:force_divisible_by=2"
    )
}
