---
name: watch-rs
version: "3.0.0"
description: "Watch a video (URL or local path). Rust-powered analysis with frame extraction and transcript generation."
argument-hint: " <url-or-path> [question]"
allowed-tools: Bash, Read, AskUserQuestion
homepage: https://github.com/m1crodevil/hermes-video-rs
repository: https://github.com/m1crodevil/hermes-video-rs
author: m1crodevil
license: MIT
user-invocable: true
platforms: [macos, linux]
metadata:
  hermes:
    tags: [video, analysis, multimodal, rust]
    category: content-creation
    requires_toolsets: [terminal]
---

# /watch-rs

Rust-powered video analysis. Faster startup (~5ms), smaller memory (~5-15MB), single binary (5.2MB).

## Binary

```bash
which watch-rs || echo "Install: cp ~/hermes-video-rs/target/release/watch-rs /usr/local/bin/"
```

## Quick Start

```bash
# Analyze a video
watch-rs "https://youtu.be/abc"

# Local file
watch-rs ~/Videos/recording.mp4

# Detail modes
watch-rs "https://youtu.be/abc" --detail transcript    # No frames
watch-rs "https://youtu.be/abc" --detail efficient     # Keyframes, cap 50
watch-rs "https://youtu.be/abc" --detail balanced      # Scene-aware, cap 100
watch-rs "https://youtu.be/abc" --detail token-burner  # Uncapped
```

## CLI Options

| Flag | Description | Default |
|------|-------------|---------|
| `--detail` | transcript/efficient/balanced/token-burner | balanced |
| `--max-frames N` | Frame cap override | 100 |
| `--resolution W` | Frame width in px | 512 |
| `--fps F` | Override auto-fps (max 2.0) | auto |
| `--start T` | Range start (SS, MM:SS, HH:MM:SS) | none |
| `--end T` | Range end | none |
| `--timestamps T` | Comma-separated timestamps for cue frames | none |
| `--output` | Output format: markdown, json, both | markdown |
| `--keep-video` | Keep downloaded video after processing | false |
| `--whisper groq\|openai` | Force Whisper backend | auto |
| `--no-whisper` | Disable Whisper fallback | false |
| `--no-dedup` | Keep duplicate frames | false |
| `--out-dir DIR` | Working directory | tmp |

## Workflow

1. Run `watch-rs <source>` — outputs markdown report with frame paths + transcript
2. Read each frame path to see the images
3. Combine frames + transcript to answer user questions

## Detail Modes

- `transcript` — No frames, transcript only (fastest)
- `efficient` — Keyframes only (I-frames via `-skip_frame nokey`), cap 50
- `balanced` — Scene-aware (detect cuts → extract per-scene), cap 100 (recommended)
- `token-burner` — Scene-aware, uncapped (max fidelity)

## Frame Engines

| Engine | When Used | How It Works |
|--------|-----------|--------------|
| **scene** | balanced/token-burner, ≥8 scene changes | ffmpeg `select='gt(scene,0.20)'` → extract at cuts |
| **keyframe** | efficient, ≥4 I-frames | ffmpeg `-skip_frame nokey` → I-frames only |
| **uniform** | fallback when scene/keyframe too few | Fixed fps extraction |
| **transcript-cue** | `--timestamps` flag | One frame per timestamp (pinned) |

## Output Formats

```bash
# Markdown (default)
watch-rs video.mp4

# JSON (for programmatic use)
watch-rs video.mp4 --output json | jq .

# Both (markdown to stdout + report.json file)
watch-rs video.mp4 --output both
```

## Language Detection

Automatically detects video language and selects best subtitles:
1. Manual subs in video language
2. Auto-generated subs in video language
3. Manual English
4. Auto English
5. Video language (triggers Whisper fallback)

## Configuration

Same as Python version: `~/.config/watch/.env`
```
GROQ_API_KEY=gsk_...
OPENAI_API_KEY=sk-...
WATCH_DETAIL=balanced
SETUP_COMPLETE=true
```

## YouTube 2026 Support

Auto-detects and uses:
- **deno** — JS runtime for YouTube challenge solving
- **curl_cffi** — Browser impersonation (anti-bot)
- **Chrome cookies** — Authenticated sessions

No manual flags needed — just ensure deps are installed.
