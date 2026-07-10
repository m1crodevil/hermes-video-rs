---
name: watch
version: "2.0.0"
description: "Watch a video (URL or local path). Rust-powered analysis with frame extraction and transcript generation."
argument-hint: " [question]"
allowed-tools: Bash, Read, AskUserQuestion
homepage: https://github.com/m1crodevil/hermes-video-rs
repository: https://github.com/m1crodevil/hermes-video-rs
author: m1crodevil
license: MIT
user-invocable: true
platforms: [macos, linux]
metadata:
  hermes:
    tags: [video, analysis, multimodal]
    category: content-creation
    requires_toolsets: [terminal]
---

# /watch (Rust)

Rust-powered video analysis. Faster startup (~5ms vs ~200ms), smaller memory footprint, single binary.

## Binary Location

The `watch-rs` binary should be in PATH or at a known location:
```bash
which watch-rs || echo "Binary not found — install: cp ~/hermes-video-rs/target/release/watch-rs /usr/local/bin/"
```

## Quick Start

```bash
# Analyze a video
watch-rs "https://youtu.be/abc" 

# With question
watch-rs "https://youtu.be/abc" --detail balanced

# Local file
watch-rs ~/Videos/recording.mp4
```

## CLI Options

| Flag | Description | Default |
|------|-------------|---------|
| `--detail` | transcript/efficient/balanced/token-burner | balanced |
| `--max-frames N` | Frame cap override | 100 |
| `--resolution W` | Frame width in px | 512 |
| `--fps F` | Override auto-fps | auto |
| `--start T` | Range start | none |
| `--end T` | Range end | none |
| `--timestamps T` | Comma-separated timestamps | none |
| `--whisper groq\|openai` | Force Whisper backend | auto |
| `--no-whisper` | Disable Whisper | false |
| `--no-dedup` | Keep duplicate frames | false |
| `--out-dir DIR` | Working directory | tmp |

## Workflow

1. Run `watch-rs <source>` — outputs markdown report with frame paths + transcript
2. Read each frame path to see the images
3. Combine frames + transcript to answer user questions

## Detail Modes

- `transcript` — No frames, transcript only (fastest)
- `efficient` — Keyframes only, cap 50
- `balanced` — Scene-aware, cap 100 (recommended)
- `token-burner` — Scene-aware, uncapped (max fidelity)

## Configuration

Same as Python version: `~/.config/watch/.env`
```
GROQ_API_KEY=gsk_...
OPENAI_API_KEY=sk-...
WATCH_DETAIL=balanced
SETUP_COMPLETE=true
```
