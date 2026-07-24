> **v5.0.0 refactor (Jul 2026):** OxiMedia integration still available for editing workflows.
> watch2 focuses on analysis only — use OxiMedia separately for trim/merge/timeline.

# Pure Rust Video Editing Options

When the user needs video editing (trim, merge, timeline) in pure Rust without FFmpeg dependency, use OxiMedia.

## OxiMedia (Recommended)

**GitHub:** https://github.com/cool-japan/oximedia
**Version:** v0.2.0 (stable: 0.1.9)
**MSRV:** Rust 1.87+
**License:** Apache 2.0
**Binary:** `oximedia-cli` (~25MB single binary)

### Key Features
- 114 crates, ~2.95M SLOC, 101K+ tests
- Zero C/C++/Fortran in default build
- Patent-free codecs only: AV1, VP9, VP8, Opus, Vorbis, FLAC
- Smart trim with AI-powered analysis (silence, scene boundaries, motion, audio peaks)
- Multi-track timeline, transitions, effects, title overlay
- WASM support, async-first (Tokio)

### CLI Usage

```bash
# Probe video info
oximedia probe input.mp4

# Transcode (H.264 -> AV1 if needed)
oximedia transcode input.mp4 -o output.mp4 --codec av1

# Create clip with in/out points
oximedia clips create -i input.mp4 -n "hook" \
  --tc-in 00:00:05:00 --tc-out 00:00:15:00 --db clips.json

# Trim clip
oximedia clips trim -c <clip_id> \
  --tc-in 00:00:08:00 --tc-out 00:00:12:00 --db clips.json

# Merge clips
oximedia clips merge -c "id1,id2,id3" --name "final" --db clips.json

# Export
oximedia clips export --db clips.json --output final.mp4

# Timeline workflow
oximedia timeline create --name "project" --fps 30
oximedia timeline add-clip --input clip1.mp4 --start 0
oximedia timeline render --output final.mp4
```

### Rust API Usage

```rust
use oximedia_edit::{Timeline, TimelineEditor, Clip, ClipType, smart_trim};
use oximedia_core::Rational;

let mut timeline = Timeline::new(
    Rational::new(1, 1000),  // 1ms timebase
    Rational::new(30, 1),     // 30 fps
);
let video_track = timeline.add_track(oximedia_edit::TrackType::Video);
let clip = Clip::new(1, ClipType::Video, 0, 10000);
timeline.add_clip(video_track, clip)?;

// Smart trim - analyze for optimal cut points
let config = smart_trim::SmartTrimConfig::default();
let suggestions = smart_trim::analyze_trim_points(&timeline, clip.id(), &config)?;
```

### Install

```bash
git clone https://github.com/cool-japan/oximedia
cd oximedia
cargo build --release -p oximedia-cli
# Binary: target/release/oximedia
```

## When to Use Each Tool

| Task | OxiMedia | FFmpeg CLI | watch2 |
|------|----------|------------|--------|
| Download video | ❌ | ❌ | ✅ |
| Analyze/inspect | ❌ | ✅ probe | ✅ |
| Trim/cut | ✅ | ✅ | ❌ |
| Merge/concat | ✅ | ✅ | ❌ |
| Timeline edit | ✅ | ❌ | ❌ |
| Effects/transitions | ✅ | ✅ (limited) | ❌ |
| Smart trim (AI) | ✅ | ❌ | ❌ |
| Transcode | ✅ | ✅ | ❌ |

## Patent-Free Codec Limitation

OxiMedia only supports patent-free codecs (AV1, VP9, VP8, Opus, Vorbis, FLAC).
Input H.264/H.265/AAC videos must be transcoded first:

```bash
oximedia transcode input_h264.mp4 -o input_av1.mp4 --codec av1
```

## Pitfalls

1. **H.264 input requires transcode** — OxiMedia won't accept patent-encumbered codecs
2. **Early stage** — v0.2.0, may have bugs; test thoroughly
3. **Build time** — ~10 min for full release build (114 crates)
4. **No web download** — Use watch2/yt-dlp for downloading, OxiMedia for editing only
