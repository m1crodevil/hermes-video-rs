use crate::output::FrameInfo;

const DEDUP_THUMB: u32 = 16;
const DEDUP_THRESHOLD: f64 = 2.0;

/// Remove near-duplicate frames by comparing grayscale thumbnails.
pub fn dedup_frames(frames: &mut Vec<FrameInfo>) -> u32 {
    let mut dropped = 0u32;
    let mut prev_raw: Option<Vec<u8>> = None;
    frames.retain(|f| {
        let img = image::open(&f.path).ok();
        let thumb = img.map(|i| {
            let resized = i.resize_exact(DEDUP_THUMB, DEDUP_THUMB, image::imageops::FilterType::Nearest);
            resized.to_luma8().into_raw()
        });
        let keep = match (&thumb, &prev_raw) {
            (Some(t), Some(p)) => {
                let diff: f64 = t.iter().zip(p.iter()).map(|(a, b)| (*a as f64 - *b as f64).abs()).sum::<f64>() / (DEDUP_THUMB * DEDUP_THUMB) as f64;
                diff > DEDUP_THRESHOLD
            }
            _ => true,
        };
        if keep { prev_raw = thumb; true } else { dropped += 1; false }
    });
    dropped
}
