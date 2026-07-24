# watch2 — Video Analysis for AI Agents

> **It watches. It listens. It verifies.**
> Rust-powered video analysis skill for Hermes Agent — transcript-first with scene detection. Cross-references frames against captions to catch what either one misses.

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-2024+-orange.svg)](https://www.rust-lang.org/)
[![Hermes Agent](https://img.shields.io/badge/Hermes-Agent-purple)](https://hermes-agent.nousresearch.com)
[![GitHub stars](https://img.shields.io/github/stars/m1crodevil/hermes-video-rs)](https://github.com/m1crodevil/hermes-video-rs/stargazers)
[![Version](https://img.shields.io/badge/version-7.2.0-blue.svg)](https://github.com/m1crodevil/hermes-video-rs/releases)

**Works with:** Hermes Agent · Claude Code · Codex · Any AI agent that reads files

Paste a URL or a local path. Hermes fetches captions, downloads the video, detects scene changes, extracts frames at key moments, and cross-references the transcript against what's actually on screen. Auto-captions misspell names? It catches that. A claim doesn't match the visual? It flags that.

Zero config to start. `yt-dlp`, `ffmpeg`, and `av-scenechange` are the only runtime dependencies. Captions cover most public videos for free. Whisper API key is only needed when a video has no captions.

---

## Quick Install

**Hermes Agent (recommended):**
```bash
hermes skill install watch2
```

**From source:**
```bash
git clone https://github.com/m1crodevil/hermes-video-rs && cd hermes-video-rs
cargo build --release
sudo cp target/release/watch2 /usr/local/bin/
```

**Runtime dependencies:** `yt-dlp`, `ffmpeg`, `ffprobe`, `av-scenechange`

---

## What People Use It For

**Analyze someone else's content.**
```bash
watch2 https://youtu.be/ what hook did they open with?
```

**Diagnose a bug from a video.**
```bash
watch2 bug-repro.mov what's going wrong?
```

**Summarize a video.**
```bash
watch2 https://youtu.be/ summarize this
```

**Catch what captions get wrong.**
```bash
watch2 https://youtu.be/ are any names or terms misspelled in the captions?
```

---

## How It Works

watch2 runs a **single linear pipeline** — no mode branching, no configuration trees:

```
Video URL / local path
    ↓
1. Detect language via yt-dlp metadata (quick, no download)
    ↓
2. Download video (720p) + targeted subtitles (JSON3) via yt-dlp
    ↓
3. Parse transcript from best-matching subtitle file
    ↓
4. Whisper fallback (if no captions and API key available)
    ↓
5. Scene detection via av-scenechange
    ↓
6. Agent reads report.json → selects key moments via LLM → extracts frames
    ↓
7. Cleanup video file (save disk space)
    ↓
8. Build WatchReport (markdown/JSON)
```

The binary handles data extraction only. All intelligence (LLM calls, moment selection, analysis) is handled by the agent. No `moments_prompt.txt`, no `key_moments.json` — the agent reads `report.json` and decides what to analyze.

### Cross-Reference Methodology

Every vision finding is classified:
- ✅ **confirmed** — vision matches transcript
- 🔧 **corrected** — vision shows different spelling/entity
- ❓ **fabrication** — claim has no visual evidence
- ⚠️ **unverified** — cannot determine from visual alone
- 🔸 **partial** — partially shown on screen

---

## Key Features

| Feature | Detail |
|---------|--------|
| Transcript-first | JSON3 captions with word-level timing |
| Scene detection | av-scenechange for visual boundaries |
| Cross-reference | Frames vs transcript — catches misspellings, fabrications |
| Single binary | ~6MB, zero config, 5ms cold start |
| Agent-native | Outputs report.json, agent handles intelligence |
| Multi-platform | YouTube, TikTok, Vimeo, local files, any URL yt-dlp supports |
| Cache-aware | SHA256 dedup, skip re-downloads |
| Whisper fallback | Groq ($0.004/min) or OpenAI — only when no captions |

---

## CLI Reference

```bash
# Basic usage
watch2 https://youtu.be/dQw4w9WgXcQ what happens at the 30 second mark?
watch2 https://www.tiktok.com/@user/video/123 summarize this
watch2 ~/Movies/screen-recording.mp4 when does the UI break?
watch2 https://vimeo.com/123 what tools does she mention?
```

| Flag | Description | Default |
|------|-------------|---------|
| `--resolution W` | Frame width in pixels (128–4096) | 512 |
| `--out-dir DIR` | Custom working directory | temp dir |
| `--keep-video` | Retain downloaded video after processing | false |
| `--cookies` | Use Chrome cookies for yt-dlp (age-restricted videos) | false |
| `--no-whisper` | Disable Whisper fallback transcription | false |
| `--no-dedup` | Keep near-duplicate frames | false |
| `--output markdown\|json\|both` | Output format | markdown |
| `--no-cache` | Disable download cache | false |
| `--cache-dir DIR` | Custom cache directory | `~/.cache/watch2` |
| `--timestamps T` | Comma-separated timestamps for cue frame extraction (e.g. "00:30,01:15,02:45") | none |

---

## Output Formats

| Format | Command | Use When |
|--------|---------|----------|
| Markdown (default) | `watch2 URL question` | Agent reads directly |
| JSON | `watch2 URL question --output json` | Programmatic processing |
| Both | `watch2 URL question --output both` | Agent + file storage |

The `WatchReport` includes: video metadata, extracted frames with timestamps, full transcript with word-level timing (when available from JSON3 captions), scene boundaries from av-scenechange, key moment metadata (LLM-selected moments with reasons), and warnings for sparse coverage or missing transcript.

---

## API Keys & Configuration

Captions cover the majority of public videos for free. The Whisper fallback only kicks in when a video has no caption track.

| Capability | Requirement | Cost |
|------------|-------------|------|
| Download + native captions | `yt-dlp` + `ffmpeg` | Free |
| Agent-side moment selection | Agent LLM (via Hermes) | Included |
| Whisper fallback (preferred) | [Groq API key](https://console.groq.com/keys) | ~$0.004/min |
| Whisper fallback (alt) | [OpenAI API key](https://platform.openai.com/api-keys) | Standard pricing |
| Disable Whisper | `--no-whisper` | Free, frames-only |

Config file: `~/.config/watch/.env`

```bash
GROQ_API_KEY=gsk_...        # Optional — for Whisper fallback
OPENAI_API_KEY=sk-...        # Optional — alternative Whisper provider
SETUP_COMPLETE=true
```

---

## Architecture

```
watch2/
├── src/
│   ├── main.rs             # Entry point — CLI, cache init, pipeline run
│   ├── cli.rs              # clap CLI definition (source, resolution, flags)
│   ├── config.rs           # Config loading (.env) — API keys
│   ├── setup.rs            # Preflight checks (binaries, API keys)
│   ├── error.rs            # WatchError enum (thiserror)
│   ├── download.rs         # yt-dlp wrapper with retry + caching
│   ├── transcript.rs       # JSON3/VTT subtitle parser
│   ├── timestamp.rs        # Timestamp parsing (SS, MM:SS, HH:MM:SS)
│   ├── pipeline.rs         # Linear pipeline — language detection, download, transcript, scenes
│   ├── frames/
│   │   ├── mod.rs          # auto-fps, scale filter, FrameMeta
│   │   ├── metadata.rs     # ffprobe video metadata
│   │   └── timestamp.rs    # Frame extraction at specific timestamps
│   ├── scene_detect.rs     # av-scenechange integration
│   ├── moments.rs          # LLM moment detection + prompt generation
│   ├── moment_frames.rs    # Moment-frame linking + timestamp extraction
│   ├── vision.rs           # Vision analysis (batch LLM calls)
│   ├── output.rs           # WatchReport structs + markdown/JSON output
│   ├── cache.rs            # Download cache (SHA256, video + subtitles)
│   └── whisper.rs          # Groq/OpenAI Whisper API client
└── tests/                  # Integration tests (170 passing)
```

---

## Why Rust?

| | Python (hermes-video) | Rust (hermes-video-rs) |
|---|---|---|
| **Startup** | ~500ms (Python import) | ~5ms |
| **Memory** | ~50-100MB | ~5-15MB |
| **Binary** | 0 (needs Python runtime) | ~6MB self-contained |
| **Install** | pip + yt-dlp + ffmpeg | Single binary + yt-dlp + ffmpeg + av-scenechange |
| **Tests** | 1,379 LOC | 5,311 LOC (170 passing) |

---

## Development

```bash
# Run all tests
cargo test

# Run specific test suites
cargo test test_cli
cargo test test_transcript
cargo test test_output
cargo test test_frames

# Build release
cargo build --release

# Install
sudo cp target/release/watch2 /usr/local/bin/

# Run with verbose output
RUST_LOG=debug cargo run -- --help
```

---

## Related Projects

**Note:** This is a Rust rewrite of [hermes-video](https://github.com/m1crodevil/hermes-video) — same features, 100× faster startup, single binary.

- [bradautomates/claude-video](https://github.com/bradautomates/claude-video) — Original inspiration (7.6k stars)
- [m1crodevil/hermes-video](https://github.com/m1crodevil/hermes-video) — Python version (same features, slower startup)

---

## License

MIT. Built on [yt-dlp](https://github.com/yt-dlp/yt-dlp), [ffmpeg](https://ffmpeg.org), [av-scenechange](https://github.com/opensourcedaa/av-scenechange). Whisper transcription via [Groq](https://groq.com) or [OpenAI](https://openai.com).

Original: [bradautomates/claude-video](https://github.com/bradautomates/claude-video)
