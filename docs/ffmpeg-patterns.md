# FFmpeg Subprocess Patterns in Rust

Complete reference for invoking ffmpeg from Rust using `std::process::Command`. All patterns are designed for use in hermes-video-rs and are direct ports of the Python equivalents in `~/hermes-video/skills/watch/scripts/frames.py` and `whisper.py`.

---

## Table of Contents

1. [Scene Detection](#1-scene-detection)
2. [Keyframe Extraction](#2-keyframe-extraction)
3. [Batch Thumbnail Dedup](#3-batch-thumbnail-dedup)
4. [Audio Extraction](#4-audio-extraction)
5. [Audio Chunking](#5-audio-chunking)
6. [Frame Extraction at Timestamp](#6-frame-extraction-at-timestamp)
7. [Common Utilities](#7-common-utilities)
8. [Alternative: ffmpeg-sidecar Crate](#8-alternative-ffmpeg-sidecar-crate)

---

## 1. Scene Detection

### FFmpeg Command

```bash
ffmpeg -i video.mp4 \
  -vf "select='eq(n\,0)+gt(scene\,0.20)',showinfo" \
  -vsync vfr -f null -
```

**How it works:**
- `select='eq(n\,0)+gt(scene\,0.20)'` — selects frame 0 (always) + frames where scene change score > 0.20
- `showinfo` — prints frame metadata to **stderr** including `pts_time:`
- `-vsync vfr` — variable frame rate output (required for select filter)
- `-f null -` — discard the output, we only care about stderr

**Expected stderr output:**

```
[Parsed_showinfo_0 @ 0x55555556] n:0 pts:0 pts_time:0 pos:...
[Parsed_showinfo_0 @ 0x55555556] n:150 pts:5005000 pts_time:5.005 pos:...
[Parsed_showinfo_0 @ 0x55555556] n:312 pts:10401040 pts_time:10.401 pos:...
```

### Rust Implementation

```rust
use std::process::Command;

/// Detect scene changes in a video file.
///
/// Returns a sorted Vec of timestamps (in seconds) where scene changes occur.
/// The first frame (t=0) is always included.
///
/// # Arguments
/// * `video_path` - Path to the input video file
/// * `threshold`  - Scene change sensitivity (0.0-1.0). Lower = more sensitive.
///                  Default: 0.20 for typical content, 0.25-0.3 for fast-cut content.
fn detect_scenes(video_path: &str, threshold: f64) -> Result<Vec<f64>, String> {
    let output = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-i", video_path,
            "-vf", &format!("select='eq(n\\,0)+gt(scene\\,{})',showinfo", threshold),
            "-vsync", "vfr",
            "-f", "null",
            "-",
        ])
        .output()
        .map_err(|e| format!("Failed to execute ffmpeg: {}", e))?;

    // Parse pts_time from stderr
    // Format: [Parsed_showinfo_0 @ 0x...] n:N pts:... pts_time:0.123456 ...
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut timestamps: Vec<f64> = Vec::new();

    for line in stderr.lines() {
        if let Some(pos) = line.find("pts_time:") {
            let rest = &line[pos + 9..];
            // pts_time is followed by a space
            let time_str: String = rest.chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            if let Ok(t) = time_str.parse::<f64>() {
                timestamps.push(t);
            }
        }
    }

    timestamps.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    Ok(timestamps)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let scenes = detect_scenes("input.mp4", 0.20)?;
    println!("Found {} scene changes:", scenes.len());
    for (i, ts) in scenes.iter().enumerate() {
        let mins = (ts / 60.0) as u64;
        let secs = ts - (mins as f64) * 60.0;
        println!("  Scene {}: {:02}:{:05.2} ({:.3}s)", i + 1, mins, secs, ts);
    }
    Ok(())
}
```

### Key Details

- **Why `-vsync vfr`?** The `select` filter drops frames, so frame-rate-based sync would produce duplicates. VFR ensures only selected frames are output.
- **Why `-f null -`?** We don't need the output video; scene data comes from `showinfo` on stderr.
- **Threshold tuning:** Start at 0.20. Use 0.25-0.30 for fast-cut content (music videos, trailers). Use 0.40-0.50 for only very hard cuts.
- **Escaping:** The backslash before commas (`\,`) in the select expression is required because ffmpeg interprets commas as filter separators.

---

## 2. Keyframe Extraction

### FFmpeg Command

```bash
ffmpeg -skip_frame nokey -i video.mp4 \
  -vsync vfr \
  -vf "scale=1280:-2,showinfo" \
  -q:v 4 \
  keyframe_%04d.jpg
```

**How it works:**
- `-skip_frame nokey` — only decode keyframes (I-frames); much faster than decoding every frame
- `scale=1280:-2` — scale to 1280px wide, height auto (must be even)
- `showinfo` — optional, prints frame info including `n:` frame index
- `-q:v 4` — JPEG quality (2-31, lower = better; 4 is good quality)
- `keyframe_%04d.jpg` — sequential output filenames

### Rust Implementation

```rust
use std::process::Command;
use std::path::Path;

/// Extract only keyframes (I-frames) from a video.
///
/// # Arguments
/// * `video_path`   - Path to the input video file
/// * `output_dir`   - Directory for extracted JPEG frames
/// * `width`        - Target width (height auto-calculated)
/// * `jpeg_quality` - JPEG quality 2-31 (default 4, lower = better)
///
/// # Returns
/// Number of keyframes extracted.
fn extract_keyframes(
    video_path: &str,
    output_dir: &str,
    width: u32,
    jpeg_quality: u32,
) -> Result<u32, String> {
    // Ensure output directory exists
    std::fs::create_dir_all(output_dir)
        .map_err(|e| format!("Failed to create output dir: {}", e))?;

    let output_pattern = format!("{}/keyframe_%04d.jpg", output_dir);

    let output = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-skip_frame", "nokey",
            "-i", video_path,
            "-vsync", "vfr",
            "-vf", &format!("scale={}:-2", width),
            "-q:v", &jpeg_quality.to_string(),
            &output_pattern,
        ])
        .output()
        .map_err(|e| format!("Failed to execute ffmpeg: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ffmpeg failed (exit {}): {}", output.status, stderr));
    }

    // Count extracted files
    let count = count_extracted_frames(output_dir, "keyframe_")?;
    Ok(count)
}

/// Count output frames matching a prefix pattern.
fn count_extracted_frames(dir: &str, prefix: &str) -> Result<u32, String> {
    let mut count = 0;
    let entries = std::fs::read_dir(dir)
        .map_err(|e| format!("Failed to read dir: {}", e))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Read dir error: {}", e))?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with(prefix) && name_str.ends_with(".jpg") {
            count += 1;
        }
    }
    Ok(count)
}

/// Extract keyframes with showinfo for timestamp metadata.
///
/// Returns a Vec of (frame_index, timestamp_seconds, file_path).
fn extract_keyframes_with_timestamps(
    video_path: &str,
    output_dir: &str,
    width: u32,
) -> Result<Vec<(u32, f64, String)>, String> {
    std::fs::create_dir_all(output_dir)
        .map_err(|e| format!("Failed to create output dir: {}", e))?;

    let output_pattern = format!("{}/keyframe_%04d.jpg", output_dir);

    let output = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-skip_frame", "nokey",
            "-i", video_path,
            "-vsync", "vfr",
            "-vf", &format!("scale={}:-2,showinfo", width),
            "-q:v", "4",
            &output_pattern,
        ])
        .output()
        .map_err(|e| format!("Failed to execute ffmpeg: {}", e))?;

    // Parse showinfo output from stderr
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut frames = Vec::new();
    let mut frame_num = 0u32;

    for line in stderr.lines() {
        if !line.contains("Parsed_showinfo") {
            continue;
        }

        let mut pts_time = 0.0f64;
        let mut n = 0u32;

        // Extract pts_time
        if let Some(pos) = line.find("pts_time:") {
            let rest = &line[pos + 9..];
            let time_str: String = rest.chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            pts_time = time_str.parse().unwrap_or(0.0);
        }

        // Extract n:
        if let Some(pos) = line.find(" n:") {
            let rest = &line[pos + 3..];
            let n_str: String = rest.chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            n = n_str.parse().unwrap_or(0);
        }

        let file_path = format!("{}/keyframe_{:04}.jpg", output_dir, frame_num);
        if Path::new(&file_path).exists() {
            frames.push((n, pts_time, file_path));
            frame_num += 1;
        }
    }

    Ok(frames)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let count = extract_keyframes("input.mp4", "./keyframes", 1280, 4)?;
    println!("Extracted {} keyframes", count);

    // Or with timestamps:
    let frames = extract_keyframes_with_timestamps("input.mp4", "./keyframes", 1280)?;
    for (idx, ts, path) in &frames {
        println!("  Frame n={} at {:.3}s: {}", idx, ts, path);
    }
    Ok(())
}
```

### Key Details

- **`-skip_frame nokey`** is the magic flag. It tells ffmpeg to skip non-keyframes during decoding, making extraction 10-50x faster.
- **`-vsync vfr`** is essential — without it, ffmpeg duplicates frames to fill gaps.
- **Timestamp mapping:** Use `showinfo` filter to get `pts_time:` for each frame. The `n:` value in showinfo corresponds to the decoder's internal frame count, not the output file index.
- **Alternative with ffprobe:** For getting keyframe timestamps without extracting frames: `ffprobe -loglevel error -skip_frame nokey -select_streams v:0 -show_entries frame=pkt_pts_time -of csv=p=0 video.mp4`

---

## 3. Batch Thumbnail Dedup

### FFmpeg Command

```bash
ffmpeg -i "frame_%04d.jpg" \
  -vf "scale=16:16,format=gray" \
  -f rawvideo -
```

**How it works:**
- Input: numbered frame images (`frame_0001.jpg`, `frame_0002.jpg`, ...)
- `scale=16:16` — resize to 16×16 pixels (256 bytes per frame in grayscale)
- `format=gray` — single channel, 8-bit
- `-f rawvideo -` — output raw bytes to stdout (no header, no container)

### Rust Implementation

```rust
use std::process::{Command, Stdio};
use std::io::Read;

/// Thumbnail-based frame deduplication.
///
/// Converts each frame to a tiny 16×16 grayscale image and compares
/// consecutive frames byte-by-byte. Frames with identical thumbnails
/// are considered duplicates.
///
/// # Arguments
/// * `frame_pattern` - Input pattern like "frames/frame_%04d.jpg"
/// * `total_frames`  - Total number of frames in the sequence
/// * `threshold`     - Mean absolute pixel diff threshold (0.0 = exact match)
///
/// # Returns
/// Indices of unique (non-duplicate) frames.
fn dedup_frames(
    frame_pattern: &str,
    total_frames: u32,
    threshold: f64,
) -> Result<Vec<u32>, String> {
    let thumb_size: usize = 16 * 16; // 256 bytes per frame

    // We can't use -f null here; we need the raw output.
    // Use `pipe:` output format for rawvideo.
    let mut child = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel", "error",
            "-i", frame_pattern,
            "-vf", "scale=16:16,format=gray",
            "-f", "rawvideo",
            "pipe:1",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn ffmpeg: {}", e))?;

    let mut stdout = child.stdout.take().expect("Failed to capture stdout");
    let mut all_bytes = Vec::new();

    // Read all raw video bytes
    stdout.read_to_end(&mut all_bytes)
        .map_err(|e| format!("Failed to read stdout: {}", e))?;

    let status = child.wait()
        .map_err(|e| format!("Failed to wait for ffmpeg: {}", e))?;

    if !status.success() {
        let mut stderr = String::new();
        child.stderr.take().map(|mut s| s.read_to_string(&mut stderr).ok());
        return Err(format!("ffmpeg failed: {}", stderr));
    }

    let total_bytes = all_bytes.len();
    let frame_count = total_bytes / thumb_size;

    if frame_count == 0 {
        return Ok(vec![]);
    }

    let mut unique_indices: Vec<u32> = vec![0]; // First frame is always unique
    let mut prev_thumb = &all_bytes[0..thumb_size];

    for i in 1..frame_count {
        let offset = i * thumb_size;
        let curr_thumb = &all_bytes[offset..offset + thumb_size];

        // Mean absolute pixel difference
        let diff: f64 = prev_thumb.iter()
            .zip(curr_thumb.iter())
            .map(|(a, b)| (*a as f64 - *b as f64).abs())
            .sum::<f64>() / thumb_size as f64;

        if diff > threshold {
            unique_indices.push(i as u32);
        }

        prev_thumb = curr_thumb;
    }

    Ok(unique_indices)
}

/// Mean absolute pixel difference between two grayscale thumbnails.
fn frame_delta(a: &[u8], b: &[u8]) -> f64 {
    if a.len() != b.len() {
        return f64::INFINITY;
    }
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (*x as f64 - *y as f64).abs())
        .sum::<f64>() / a.len() as f64
}

/// Hamming distance between two byte slices (bit-level comparison).
fn hamming_distance(a: &[u8], b: &[u8]) -> u32 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x ^ y).count_ones())
        .sum()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let total = 300; // e.g., 300 frames extracted from video

    let unique = dedup_frames("frames/frame_%04d.jpg", total, 2.0)?;
    println!("{}/{} frames are unique", unique.len(), total);
    for &idx in &unique {
        println!("  Unique frame: {}", idx);
    }

    Ok(())
}
```

### Key Details

- **Why 16×16 grayscale?** 16×16 = 256 bytes per frame. Comparing 256 bytes is orders of magnitude faster than comparing full-resolution frames. Grayscale removes color noise.
- **`pipe:1`** means stdout. ffmpeg's raw video output format writes pixel data directly — no container, no headers.
- **Threshold:** 0.0 = exact match. For noisy video, try threshold 1.0-3.0 (mean absolute pixel diff) to tolerate compression artifacts.
- **Alternative approach:** Instead of reading all at once, you can read in a streaming fashion using `child.stdout.take()` and compare on-the-fly. See the `dedup_frames` function above.
- **Memory:** For 300 frames at 256 bytes each = 76KB total. This scales well up to ~10,000 frames before memory pressure becomes relevant.

---

## 4. Audio Extraction

### FFmpeg Command

```bash
ffmpeg -i video.mp4 \
  -vn \
  -acodec libmp3lame \
  -ar 16000 \
  -ac 1 \
  -b:a 64k \
  audio.mp3
```

**How it works:**
- `-vn` — discard video stream
- `-acodec libmp3lame` — encode to MP3 (widely supported, good compression)
- `-ar 16000` — 16kHz sample rate (optimal for speech recognition / whisper)
- `-ac 1` — mono audio (speech doesn't need stereo)
- `-b:a 64k` — 64kbps bitrate (sufficient for speech at 16kHz mono)

### Rust Implementation

```rust
use std::process::Command;
use std::path::Path;

/// Extract audio from a video file as mono MP3 at 16kHz.
///
/// Optimized for speech recognition (Whisper-compatible output).
///
/// # Arguments
/// * `video_path`    - Path to the input video file
/// * `audio_path`    - Output path for the MP3 file
/// * `sample_rate`   - Sample rate in Hz (default: 16000)
/// * `bitrate_kbps`  - Bitrate in kbps (default: 64)
fn extract_audio(
    video_path: &str,
    audio_path: &str,
    sample_rate: u32,
    bitrate_kbps: u32,
) -> Result<(), String> {
    let output = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel", "warning",
            "-i", video_path,
            "-vn",
            "-acodec", "libmp3lame",
            "-ar", &sample_rate.to_string(),
            "-ac", "1",
            "-b:a", &format!("{}k", bitrate_kbps),
            audio_path,
        ])
        .output()
        .map_err(|e| format!("Failed to execute ffmpeg: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ffmpeg failed (exit {}): {}", output.status, stderr));
    }

    // Verify output file exists and is non-empty
    let path = Path::new(audio_path);
    if !path.exists() {
        return Err("ffmpeg produced no output file — video may have no audio track".into());
    }
    if std::fs::metadata(path)
        .map(|m| m.len() == 0)
        .unwrap_or(true)
    {
        return Err("ffmpeg produced empty audio file".into());
    }

    Ok(())
}

/// Convenience wrapper with default settings for speech recognition.
fn extract_speech_audio(video_path: &str, audio_path: &str) -> Result<(), String> {
    extract_audio(video_path, audio_path, 16000, 64)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    extract_speech_audio("video.mp4", "audio.mp3")?;
    println!("Audio extracted successfully");

    // Or with custom settings:
    extract_audio("video.mp4", "high_quality.mp3", 44100, 128)?;
    Ok(())
}
```

### Key Details

- **16kHz mono** is the standard for speech recognition (Whisper, Vosk, etc.). Don't use higher rates — it wastes space without improving transcription.
- **libmp3lame** is preferred over native MP3 encoder for better quality at low bitrates.
- **Alternative codecs:**
  - `copy` — copy original audio without re-encoding (fast, but may not be mp3)
  - `pcm_s16le` — WAV output (larger, but simpler to process)
  - `aac` — for AAC output (common in modern containers)
- **`-loglevel warning`** — suppresses verbose output while still showing errors.

---

## 5. Audio Chunking

### FFmpeg Command

```bash
ffmpeg -ss 30 -i audio.mp3 -t 30 -c copy chunk_001.mp3
ffmpeg -ss 60 -i audio.mp3 -t 30 -c copy chunk_002.mp3
ffmpeg -ss 90 -i audio.mp3 -t 30 -c copy chunk_003.mp3
```

**How it works:**
- `-ss 30` — seek to 30 seconds (input seeking, placed **before** `-i` for fast seeking)
- `-t 30` — duration of 30 seconds
- `-c copy` — stream copy (no re-encoding). Very fast — just copies bytes.
- **IMPORTANT:** `-ss` before `-i` = input seeking (fast, seeks to nearest keyframe). `-ss` after `-i` = output seeking (slow, decodes from start).

### Rust Implementation

```rust
use std::process::Command;
use std::path::Path;

/// A chunk of audio extracted from a larger file.
#[derive(Debug)]
struct AudioChunk {
    index: usize,
    offset_secs: f64,
    duration_secs: f64,
    path: String,
}

/// Split an audio file into fixed-duration chunks using stream copy.
///
/// Uses `-c copy` for zero-quality-loss, near-instant extraction.
/// Input seeking (`-ss` before `-i`) ensures fast seeking.
///
/// # Arguments
/// * `audio_path`   - Path to the input audio file
/// * `output_dir`   - Directory for chunk files
/// * `chunk_secs`   - Duration of each chunk in seconds
/// * `total_secs`   - Total duration to process (0 = entire file)
///
/// # Returns
/// Vector of AudioChunk structs with metadata.
fn chunk_audio(
    audio_path: &str,
    output_dir: &str,
    chunk_secs: f64,
    total_secs: f64,
) -> Result<Vec<AudioChunk>, String> {
    std::fs::create_dir_all(output_dir)
        .map_err(|e| format!("Failed to create output dir: {}", e))?;

    let mut chunks = Vec::new();
    let mut offset = 0.0f64;
    let mut index = 0;

    loop {
        if total_secs > 0.0 && offset >= total_secs {
            break;
        }

        let chunk_path = format!("{}/chunk_{:03}.mp3", output_dir, index);

        // Calculate effective duration for this chunk
        let effective_duration = if total_secs > 0.0 {
            let remaining = total_secs - offset;
            if remaining <= 0.0 {
                break;
            }
            chunk_secs.min(remaining)
        } else {
            chunk_secs
        };

        let output = Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel", "error",
                "-y",  // overwrite output
                "-ss", &format!("{:.3}", offset),
                "-i", audio_path,
                "-t", &format!("{:.3}", effective_duration),
                "-c", "copy",
                &chunk_path,
            ])
            .output()
            .map_err(|e| format!("Failed to execute ffmpeg: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // If ffmpeg can't extract (e.g., offset past end), stop
            if stderr.contains("Invalid") || stderr.contains("past") {
                break;
            }
            return Err(format!("ffmpeg failed at offset {}: {}", offset, stderr));
        }

        chunks.push(AudioChunk {
            index,
            offset_secs: offset,
            duration_secs: effective_duration,
            path: chunk_path,
        });

        offset += chunk_secs;
        index += 1;
    }

    Ok(chunks)
}

/// Convenience: chunk audio for Whisper transcription (30-second chunks).
fn chunk_for_whisper(audio_path: &str, output_dir: &str) -> Result<Vec<AudioChunk>, String> {
    chunk_audio(audio_path, output_dir, 30.0, 0.0)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let chunks = chunk_for_whisper("audio.mp3", "./chunks")?;
    println!("Split into {} chunks:", chunks.len());
    for chunk in &chunks {
        println!("  Chunk {}: offset={:.1}s, duration={:.1}s -> {}",
            chunk.index, chunk.offset_secs, chunk.duration_secs, chunk.path);
    }
    Ok(())
}
```

### Key Details

- **`-ss` before `-i` is critical.** When placed before the input, ffmpeg seeks to the nearest keyframe instantly. When placed after, it decodes from the beginning and discards frames — extremely slow for large files.
- **`-c copy`** means zero transcoding. The output chunks are bit-for-bit copies of the source. This is why it's so fast.
- **Whisper chunking:** Whisper models work best with 30-second chunks. The `chunk_for_whisper` convenience function uses this default.
- **Edge case:** The last chunk may be shorter than `chunk_secs`. The implementation handles this by calculating remaining duration.
- **Seeking accuracy:** MP3 seeking is not frame-accurate. The `-ss` flag will seek to the nearest frame boundary. For exact timestamps, you'd need to decode, but this is rarely necessary for transcription chunks.

---

## 6. Frame Extraction at Timestamp

### FFmpeg Command

```bash
ffmpeg -ss 12.5 -i video.mp4 \
  -frames:v 1 \
  -vf "scale=1280:-2" \
  -q:v 4 \
  frame_12.5s.jpg
```

**How it works:**
- `-ss 12.5` — seek to 12.5 seconds (placed before `-i` for fast input seeking)
- `-frames:v 1` — extract exactly one frame
- `scale=1280:-2` — scale to 1280px wide, height auto (must be even number)
- `-q:v 4` — JPEG quality (2-31, lower = better)

### Rust Implementation

```rust
use std::process::Command;

/// Extract a single frame from a video at a specific timestamp.
///
/// # Arguments
/// * `video_path` - Path to the input video file
/// * `output_path` - Path for the output JPEG file
/// * `timestamp`  - Timestamp in seconds (e.g., 12.5)
/// * `width`      - Target width (0 = keep original)
/// * `jpeg_quality` - JPEG quality 2-31 (default 4)
fn extract_frame(
    video_path: &str,
    output_path: &str,
    timestamp: f64,
    width: u32,
    jpeg_quality: u32,
) -> Result<(), String> {
    let mut args = vec![
        "-hide_banner".to_string(),
        "-loglevel".to_string(), "error".to_string(),
        "-y".to_string(),  // overwrite
        "-ss".to_string(), format!("{:.3}", timestamp),
        "-i".to_string(), video_path.to_string(),
        "-frames:v".to_string(), "1".to_string(),
    ];

    // Add scaling if width is specified
    if width > 0 {
        args.push("-vf".to_string());
        args.push(format!("scale={}:-2", width));
    }

    args.push("-q:v".to_string());
    args.push(jpeg_quality.to_string());
    args.push(output_path.to_string());

    let output = Command::new("ffmpeg")
        .args(&args)
        .output()
        .map_err(|e| format!("Failed to execute ffmpeg: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ffmpeg failed (exit {}): {}", output.status, stderr));
    }

    Ok(())
}

/// Extract a frame at timestamp, returning the output path.
///
/// Convenience function that generates the output filename automatically.
fn extract_frame_auto_name(
    video_path: &str,
    output_dir: &str,
    timestamp: f64,
    width: u32,
) -> Result<String, String> {
    let output_path = format!("{}/frame_{:.1}s.jpg", output_dir, timestamp);
    extract_frame(video_path, &output_path, timestamp, width, 4)?;
    Ok(output_path)
}

/// Extract frames at multiple timestamps.
///
/// # Arguments
/// * `timestamps` - Slice of timestamps in seconds
///
/// # Returns
/// Vector of (timestamp, output_path) tuples for successfully extracted frames.
fn extract_frames_batch(
    video_path: &str,
    output_dir: &str,
    timestamps: &[f64],
    width: u32,
) -> Result<Vec<(f64, String)>, String> {
    std::fs::create_dir_all(output_dir)
        .map_err(|e| format!("Failed to create output dir: {}", e))?;

    let mut results = Vec::new();

    for &ts in timestamps {
        match extract_frame_auto_name(video_path, output_dir, ts, width) {
            Ok(path) => results.push((ts, path)),
            Err(e) => eprintln!("Warning: failed to extract frame at {}: {}", ts, e),
        }
    }

    Ok(results)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Single frame
    extract_frame("video.mp4", "frame.jpg", 12.5, 1280, 4)?;
    println!("Extracted frame at 12.5s");

    // Multiple frames
    let timestamps = vec![0.0, 5.0, 10.0, 15.0, 20.0, 25.0, 30.0];
    let frames = extract_frames_batch("video.mp4", "./thumbnails", &timestamps, 320)?;
    println!("Extracted {} frames:", frames.len());
    for (ts, path) in &frames {
        println!("  t={:.1}s -> {}", ts, path);
    }

    Ok(())
}
```

### Key Details

- **`-ss` before `-i`:** Input seeking. Fast but seeks to nearest keyframe. For frame-accurate seeking, place `-ss` after `-i` (slower, decodes from start).
- **`-frames:v 1`:** Ensures exactly one frame is extracted. Without this, ffmpeg would continue extracting frames until the end of the video.
- **Scale syntax:** `scale=1280:-2` means "1280px wide, height auto-calculated to maintain aspect ratio, must be even." The `-2` ensures the height is divisible by 2 (required by most codecs).
- **Batch optimization:** For extracting many frames, consider using `ffprobe` first to get video duration, then batch extract. Each `Command::new("ffmpeg")` call spawns a new process, so there's per-call overhead.

---

## 7. Common Utilities

### Getting Video Duration

```rust
use std::process::Command;

/// Get video duration in seconds using ffprobe.
fn get_duration(video_path: &str) -> Result<f64, String> {
    let output = Command::new("ffprobe")
        .args([
            "-v", "error",
            "-show_entries", "format=duration",
            "-of", "default=noprint_wrappers=1:nokey=1",
            video_path,
        ])
        .output()
        .map_err(|e| format!("Failed to execute ffprobe: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let duration_str = stdout.trim();

    duration_str.parse::<f64>()
        .map_err(|e| format!("Failed to parse duration '{}': {}", duration_str, e))
}
```

### Getting Video Metadata

```rust
/// Simple video info struct.
struct VideoInfo {
    width: u32,
    height: u32,
    fps: f64,
    duration: f64,
    codec: String,
}

fn get_video_info(video_path: &str) -> Result<VideoInfo, String> {
    let output = Command::new("ffprobe")
        .args([
            "-v", "error",
            "-select_streams", "v:0",
            "-show_entries", "stream=width,height,r_frame_rate,codec_name",
            "-show_entries", "format=duration",
            "-of", "json",
            video_path,
        ])
        .output()
        .map_err(|e| format!("Failed to execute ffprobe: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Simple JSON value extraction (use serde_json in production)
    let width = extract_json_u32(&stdout, "width").unwrap_or(0);
    let height = extract_json_u32(&stdout, "height").unwrap_or(0);
    let duration = extract_json_f64(&stdout, "duration").unwrap_or(0.0);

    // Parse r_frame_rate which is "30/1" or "24000/1001"
    let fps = extract_fps(&stdout).unwrap_or(0.0);

    let codec = extract_json_string(&stdout, "codec_name")
        .unwrap_or_else(|| "unknown".to_string());

    Ok(VideoInfo { width, height, fps, duration, codec })
}

fn extract_json_f64(json: &str, key: &str) -> Option<f64> {
    let pattern = format!("\"{}\":", key);
    let start = json.find(&pattern)? + pattern.len();
    let rest = &json[start..];
    let value_str: String = rest.chars()
        .skip_while(|c| c.is_whitespace())
        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
        .collect();
    value_str.parse().ok()
}

fn extract_json_u32(json: &str, key: &str) -> Option<u32> {
    extract_json_f64(json, key).map(|v| v as u32)
}

fn extract_json_string(json: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{}\":\"", key);
    let start = json.find(&pattern)? + pattern.len();
    let rest = &json[start..];
    let value: String = rest.chars()
        .take_while(|c| *c != '"')
        .collect();
    Some(value)
}

fn extract_fps(json: &str) -> Option<f64> {
    let pattern = "\"r_frame_rate\":\"";
    let start = json.find(pattern)? + pattern.len();
    let rest = &json[start..];
    let frac: String = rest.chars()
        .take_while(|c| *c != '"')
        .collect();

    // Parse "30/1" or "24000/1001"
    let parts: Vec<&str> = frac.split('/').collect();
    if parts.len() == 2 {
        let num: f64 = parts[0].parse().ok()?;
        let den: f64 = parts[1].parse().ok()?;
        if den > 0.0 {
            return Some(num / den);
        }
    }
    None
}
```

### Checking ffmpeg Availability

```rust
use std::process::Command;

/// Check if ffmpeg is available in PATH.
fn check_ffmpeg() -> Result<String, String> {
    let output = Command::new("ffmpeg")
        .args(["-version"])
        .output()
        .map_err(|e| format!("ffmpeg not found in PATH: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let first_line = stdout.lines().next().unwrap_or("unknown version");
    Ok(first_line.to_string())
}
```

### Error Handling Pattern

```rust
use std::process::{Command, Output};

/// Robust ffmpeg execution with error handling.
///
/// Returns (success, stdout, stderr) for further processing.
fn run_ffmpeg(args: &[&str]) -> Result<(bool, String, String), String> {
    let output: Output = Command::new("ffmpeg")
        .args(args)
        .output()
        .map_err(|e| format!("Failed to execute ffmpeg: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let success = output.status.success();

    Ok((success, stdout, stderr))
}
```

---

## 8. Alternative: ffmpeg-sidecar Crate

If you prefer a higher-level API over raw `std::process::Command`, the [`ffmpeg-sidecar`](https://crates.io/crates/ffmpeg-sidecar) crate provides:

### Features

- Builder API similar to `std::process::Command`
- Automatic FFmpeg binary download
- Iterator-based frame reading
- Semantic parsing of FFmpeg stderr logs
- Rich metadata extraction

### Example (from their docs)

```rust
use ffmpeg_sidecar::command::FfmpegCommand;

fn main() -> anyhow::Result<()> {
    let iter = FfmpegCommand::new()
        .input("video.mp4")
        .rawvideo()
        .spawn()?
        .iter()?;

    for frame in iter.filter_frames() {
        println!("frame: {}x{}", frame.width, frame.height);
        let pixels: Vec<u8> = frame.data; // raw RGB pixels
    }

    Ok(())
}
```

### When to Use ffmpeg-sidecar vs std::process::Command

| Criterion | std::process::Command | ffmpeg-sidecar |
|-----------|----------------------|----------------|
| Dependencies | None (stdlib only) | External crate |
| Build complexity | Zero | Minimal |
| Control | Full control over args | Limited to builder API |
| Raw bytes piping | Manual stdout handling | Built-in iterator |
| Error parsing | Manual stderr parsing | Parsed metadata |
| Binary management | Must be in PATH | Auto-download option |
| **Recommendation** | **Preferred for hermes-video-rs** | Good for quick prototypes |

**For hermes-video-rs, use `std::process::Command` directly** — it keeps dependencies minimal and gives full control over ffmpeg arguments and output parsing.

---

## Quick Reference

### Pattern Selection Guide

| Task | Command | Rust Pattern |
|------|---------|-------------|
| Find scene changes | `-vf "select=...,showinfo"` + `-f null -` | Parse `pts_time:` from stderr |
| Extract I-frames | `-skip_frame nokey` + `-vsync vfr` | Count output files or parse showinfo |
| Dedup frames | `-vf "scale=16:16,format=gray"` + rawvideo | Read stdout bytes, compare chunks |
| Extract audio | `-vn -acodec libmp3lame -ar 16000 -ac 1` | Check exit status |
| Chunk audio | `-ss {t} -i file -t {dur} -c copy` | Loop with offset increments |
| Extract frame | `-ss {t} -i file -frames:v 1` | Check exit status |

### Common Pitfalls

1. **Escaping in select filter:** Use `\\,` not `,` in Rust strings (double backslash for literal `\,`)
2. **`-vsync vfr` is mandatory** when using `select` filter — without it, ffmpeg adds duplicate frames
3. **`-ss` placement matters:** Before `-i` = fast input seeking; after `-i` = slow decode-from-start
4. **stderr is where the good stuff lives:** `showinfo`, progress, scene scores — all on stderr
5. **rawvideo format:** No headers, no container. You get raw pixel bytes directly.
6. **Scale `-2`:** Ensures output dimensions are even (required by most codecs). `-1` would be exact but may produce odd heights.
7. **`-c copy` for chunking:** Zero transcoding overhead. But seeking is imprecise for frame-accurate cuts.
