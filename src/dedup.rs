use std::io::Read;
use std::process::{Command, Stdio};

use crate::output::FrameInfo;

const DEDUP_THUMB: u32 = 16;
const DEDUP_THRESHOLD: f64 = 2.0;
const THUMB_BYTES: usize = (DEDUP_THUMB * DEDUP_THUMB) as usize; // 256

/// Extract the starting frame number from a path like "frame_0042.jpg" → 42.
fn extract_start_number(path: &str) -> u32 {
    let stem = std::path::Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("frame_0000");
    // Find the last segment after the last underscore, parse as integer.
    stem.rsplit('_')
        .next()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0)
}

/// Build an ffmpeg image sequence pattern: "frame_0042.jpg" → "frame_%04d.jpg"
fn extract_pattern(path: &str) -> String {
    let p = std::path::Path::new(path);
    let parent = p.parent().unwrap_or(std::path::Path::new("."));
    let stem = p
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("frame_0000");
    let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("jpg");
    // Replace the numeric suffix with %04d.
    if let Some(pos) = stem.rfind('_') {
        let prefix = &stem[..=pos];
        parent
            .join(format!("{prefix}%04d.{ext}"))
            .to_string_lossy()
            .to_string()
    } else {
        parent
            .join(format!("{stem}_{{}}.{ext}"))
            .to_string_lossy()
            .to_string()
    }
}

/// Mean absolute pixel difference between two grayscale thumbnails.
fn frame_delta(a: &[u8], b: &[u8]) -> f64 {
    assert_eq!(a.len(), THUMB_BYTES);
    assert_eq!(b.len(), THUMB_BYTES);
    let sum: f64 = a
        .iter()
        .zip(b.iter())
        .map(|(x, y)| (*x as f64 - *y as f64).abs())
        .sum();
    sum / THUMB_BYTES as f64
}

/// Remove near-duplicate frames using a single ffmpeg batch pipe pass.
///
/// Runs ffmpeg once to decode all frames, resize to a tiny grayscale
/// thumbnail, and pipe raw bytes to stdout. Compares consecutive thumbnails
/// and removes duplicates. Fails open — if ffmpeg fails or byte count
/// doesn't match, returns 0 (no dedup) instead of erroring.
pub fn dedup_frames(frames: &mut Vec<FrameInfo>) -> u32 {
    if frames.len() <= 1 {
        return 0;
    }

    let first_path = &frames[0].path;
    let start_number = extract_start_number(first_path);
    let pattern = extract_pattern(first_path);

    // Build ffmpeg command: decode all frames, resize to 16x16 grayscale,
    // output raw bytes to stdout.
    let mut child = match Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-start_number",
            &start_number.to_string(),
            "-i",
            &pattern,
            "-vf",
            &format!("scale={}:{},format=gray", DEDUP_THUMB, DEDUP_THUMB),
            "-f",
            "rawvideo",
            "-",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return 0, // fail open
    };

    // Read all stdout into a buffer.
    let mut raw_buf = Vec::new();
    if let Some(ref mut stdout) = child.stdout {
        let _ = stdout.read_to_end(&mut raw_buf);
    }
    let _ = child.wait();

    // Split raw bytes into THUMB_BYTES-sized chunks.
    let expected_frames = frames.len();
    let actual_chunks = raw_buf.len() / THUMB_BYTES;
    if actual_chunks != expected_frames {
        // Byte count doesn't match — fail open
        return 0;
    }

    // Mark duplicates using index-based iteration.
    let mut dropped = 0u32;
    let mut to_remove = Vec::new();

    for i in 1..actual_chunks {
        let prev = &raw_buf[(i - 1) * THUMB_BYTES..i * THUMB_BYTES];
        let curr = &raw_buf[i * THUMB_BYTES..(i + 1) * THUMB_BYTES];
        if frame_delta(prev, curr) <= DEDUP_THRESHOLD {
            to_remove.push(i);
            dropped += 1;
        }
    }

    // Remove duplicates from frames vec (reverse order to preserve indices).
    if !to_remove.is_empty() {
        let mut frames_to_remove: Vec<usize> = to_remove.into_iter().rev().collect();
        frames_to_remove.sort_unstable_by(|a, b| b.cmp(a));
        for idx in frames_to_remove {
            let removed = frames.remove(idx);
            let _ = std::fs::remove_file(&removed.path);
        }
    }

    dropped
}
