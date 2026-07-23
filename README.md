# watch2

> **It watches. It listens. It verifies.**
> Rust-powered video analysis that *sees* frames and *reads* transcripts — then cross-references both to catch what either one misses.

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-2024+-orange.svg)](https://www.rust-lang.org/)
[![Hermes Agent](https://img.shields.io/badge/Hermes-Agent-purple)](https://hermes-agent.nousresearch.com)
[![GitHub stars](https://img.shields.io/github/stars/m1crodevil/hermes-video-rs)](https://github.com/m1crodevil/hermes-video-rs/stargazers)

**Rust rewrite of [hermes-video](https://github.com/m1crodevil/hermes-video)** — same features, 100× faster startup, single binary.

Paste a URL or a local path. Hermes fetches captions, downloads the video, detects scene changes, extracts frames at the moments that matter, and cross-references the transcript against what's actually on screen. Auto-captions misspell names? It catches that. A claim doesn't match the visual? It flags that.

```bash
hermes skill install watch2
```

Zero config to start. `yt-dlp`, `ffmpeg`, and `av-scenechange` are the only runtime dependencies. Captions cover most public videos for free. Whisper API key is only needed when a video has no captions.

---

## Why Rust?

| | Python (hermes-video) | Rust (hermes-video-rs) |
|---|---|---|
| **Startup** | ~500ms (Python import) | ~5ms |
| **Memory** | ~50-100MB | ~5-15MB |
| **Binary** | 0 (needs Python runtime) | ~6MB self-contained |
| **Install** | pip + yt-dlp + ffmpeg | Single binary + yt-dlp + ffmpeg + av-scenechange |
| **Tests** | 1,379 LOC | 5,311 LOC (173 passing) |

---

## Use Cases

**Analyze someone else's content.** `watch2 https://youtu.be/ what hook did they open with?` Hermes looks at the first frames, reads the opening transcript, breaks down the structure.

**Diagnose a bug from a video.** `watch2 bug-repro.mov what's going wrong?` Hermes watches the recording, finds the frame where the issue appears, describes what's on screen.

**Summarize a video.** `watch2 https://youtu.be/ summarize this` pulls the structure, the key moments, what was actually said and shown. Faster than watching at 2×.

**Cut the hype out of an update video.** `watch2 https://youtu.be/ what's actually new --skip-the-hype`

**Catch what captions get wrong.** The pipeline sends the transcript to an LLM which selects key moments automatically, then extracts frames at those timestamps and cross-references the transcript against what's actually on screen.

**Verify transcript accuracy with visual evidence.** Automatically identifies moments in the transcript that need visual verification (proper nouns, game names, claims, deictic references) via LLM selection, extracts frames at those timestamps, and validates the transcript against what's actually shown.

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
6. LLM selects key moments (Groq/OpenAI inline) + extracts frames at those timestamps
    ↓
7. Cleanup video file (save disk space)
    ↓
8. Build WatchReport (markdown/JSON)
```

### Single-Run Design

watch2 runs everything in one pass — no re-running, no intermediate files:

1. Detects language via quick metadata call (~1 sec)
2. Downloads the video and targeted subtitles (1-2 requests instead of 157)
3. Parses the transcript from the best-matching language
4. Runs scene detection
5. Sends the transcript to an LLM (Groq/OpenAI) which selects the key moments inline
6. Extracts frames at those timestamps
7. Builds a `WatchReport` with frames, transcript, scene boundaries, and key moment metadata
7. Cleans up the video file

Everything happens in a single invocation. No `moments_prompt.txt`, no `key_moments.json`, no agent handoff — the LLM selects moments directly during the pipeline run.

---

## Usage

```bash
# Basic usage
watch2 https://youtu.be/dQw4w9WgXcQ what happens at the 30 second mark?
watch2 https://www.tiktok.com/@user/video/123 summarize this
watch2 ~/Movies/screen-recording.mp4 when does the UI break?
watch2 https://vimeo.com/123 what tools does she mention?
```

### CLI Flags

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

---

## Output Formats

**Markdown** (default):
```
watch2 https://youtu.be/abc summarize this
```

**JSON**:
```
watch2 https://youtu.be/abc --output json
```

**Both** (markdown to stdout, JSON written to `report.json`):
```
watch2 https://youtu.be/abc --output both
```

The `WatchReport` includes:
- Video metadata (title, uploader, language, duration)
- Extracted frames with timestamps and reasons
- Full transcript with word-level timing (when available from JSON3 captions)
- Scene boundaries from av-scenechange
- Key moment metadata (LLM-selected moments with reasons)
- Warnings for sparse coverage, missing transcript, etc.

---

## Installation

**Hermes Agent (recommended):**
```bash
hermes skill install watch2
```

**From source:**
```bash
git clone https://github.com/m1crodevil/hermes-video-rs
cd hermes-video-rs
cargo build --release
sudo cp target/release/watch2 /usr/local/bin/
```

**Runtime dependencies:** `yt-dlp`, `ffmpeg`, `ffprobe`, `av-scenechange`

---

## API Keys

Captions cover the majority of public videos for free. The Whisper fallback only kicks in when a video has no caption track.

| Capability | Requirement | Cost |
|------------|-------------|------|
| Download + native captions | `yt-dlp` + `ffmpeg` | Free |
| LLM moment selection + Whisper fallback (preferred) | [Groq API key](https://console.groq.com/keys) | ~$0.004/min |
| LLM moment selection + Whisper fallback (alt) | [OpenAI API key](https://platform.openai.com/api-keys) | Standard pricing |
| Disable Whisper | `--no-whisper` | Free, frames-only |

---

## Configuration

Config file: `~/.config/watch/.env`

```bash
GROQ_API_KEY=gsk_...        # Optional — for Whisper fallback
OPENAI_API_KEY=sk-...        # Optional — alternative Whisper provider
SETUP_COMPLETE=true
```

### API Key Behavior

- **With API key**: Whisper fallback available for videos without subtitles
- **Without API key**: Only works with videos that have auto/manual captions
- **`--no-whisper`**: Suppresses the "no API key" warning, skips Whisper entirely

When no subtitles are found and no API key is set:
```
⚠️  No subtitles found. Set GROQ_API_KEY or OPENAI_API_KEY.
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
│   ├── llm.rs              # LLM language detection (Groq → OpenAI)
│   ├── pipeline.rs         # Linear pipeline — no mode branching
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
└── tests/                  # Integration tests (173 passing)
```

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

## Key Moments Pipeline

The moment-detection pipeline runs inline during a single invocation:

```
Video URL / local path
    ↓
Download + transcript + scene detection
    ↓
LLM (Groq/OpenAI) selects key moments from transcript + scene data
    ↓
Frame extraction at selected timestamps → WatchReport
    ↓
Agent reads report, analyzes frames via vision (optional)
```

**Data flow**: `report.json` is the single source of truth. The Rust binary outputs structured data; the agent reads it directly.

**Cross-reference methodology**: Every vision finding is classified:
- ✅ **confirmed** — vision matches transcript
- 🔧 **corrected** — vision shows different spelling/entity
- ❓ **fabrication** — claim has no visual evidence
- ⚠️ **unverified** — cannot determine from visual alone
- 🔸 **partial** — partially shown on screen

---

## Dependencies

Core:
- `clap` 4 — CLI parsing
- `tokio` 1 — async runtime
- `serde` / `serde_json` 1 — serialization
- `reqwest` 0.12 — HTTP client (Whisper API calls)
- `anyhow` 1 / `thiserror` 2 — error handling
- `av-scenechange` 0.24 — scene boundary detection
- `async-openai` 0.41 — OpenAI Whisper API
- `groq-api-rust` 0.3 — Groq Whisper API
- `sha2` 0.10 — cache key hashing
- `regex` 1 — transcript parsing
- `dotenvy` 0.15 — .env config loading
- `dirs` 6 — platform cache directories

Dev:
- `assert_cmd` 2 / `predicates` 3 — CLI integration tests

---

## Related Projects

- [bradautomates/claude-video](https://github.com/bradautomates/claude-video) — Original (7.6k stars)
- [m1crodevil/hermes-video](https://github.com/m1crodevil/hermes-video) — Python version

---

## License

MIT. Built on [yt-dlp](https://github.com/yt-dlp/yt-dlp), [ffmpeg](https://ffmpeg.org), [av-scenechange](https://github.com/opensourcedaa/av-scenechange). Whisper transcription via [Groq](https://groq.com) or [OpenAI](https://openai.com).

Original: [bradautomates/claude-video](https://github.com/bradautomates/claude-video)
