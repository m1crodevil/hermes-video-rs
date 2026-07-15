# /watch2

> Rust-powered video analysis for Hermes Agent.
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-2024+-orange.svg)](https://www.rust-lang.org/)
[![Hermes Agent](https://img.shields.io/badge/Hermes-Agent-purple)](https://hermes-agent.nousresearch.com)
[![GitHub stars](https://img.shields.io/github/stars/m1crodevil/hermes-video-rs)](https://github.com/m1crodevil/hermes-video-rs/stargazers)

**Rust rewrite of [hermes-video](https://github.com/m1crodevil/hermes-video)** — same features, 100× faster startup, single binary.

With `/watch2`, you paste a URL or a local path, ask a question, and Hermes fetches captions, downloads only what it needs, extracts frames, pulls a timestamped transcript, and analyzes everything. By the time it answers, it has *seen* the video and *heard* the audio.

```bash
hermes skill install watch2
```

Zero config to start. `yt-dlp` and `ffmpeg` are the only runtime dependencies. Captions cover most public videos for free. Whisper API key is only needed when a video has no captions.

---

## Why Rust?

| | Python (hermes-video) | Rust (hermes-video-rs) |
|---|---|---|
| **Startup** | ~500ms (Python import) | ~5ms |
| **Memory** | ~50-100MB | ~5-15MB |
| **Binary** | 0 (needs Python runtime) | 5.4MB self-contained |
| **Install** | pip + yt-dlp + ffmpeg | Single binary + yt-dlp + ffmpeg |
| **Tests** | 1,379 LOC | 605 LOC (158 passing) |

---

## Use Cases

**Analyze someone else's content.** `/watch2 https://youtu.be/ what hook did they open with?` Hermes looks at the first frames, reads the opening transcript, breaks down the structure. Same for ad creative, competitor launches, podcast intros — anything where the *how* matters as much as the *what*.

**Diagnose a bug from a video.** Someone sends you a screen recording of something broken. `/watch2 bug-repro.mov what's going wrong?` Hermes watches the recording, finds the frame where the issue appears, describes what's on screen, often catches the cause without you ever opening the file.

**Summarize a video.** `/watch2 https://youtu.be/ summarize this` pulls the structure, the key moments, what was actually said and shown. Faster than watching at 2x.

**Cut the hype out of an update video.** `/watch2 https://youtu.be/ what's actually new -- skip the hype` Strip a "game-changer" feature drop down to the few things that matter.

**Turn a playlist into notes.** `/watch2 https://youtu.be/ summarize this to a note` Run it across a series and file a per-video summary, so a channel or course becomes a searchable set of notes instead of hours you have to sit through.

**Verify transcript accuracy with visual evidence.** `/watch2 https://youtu.be/ --auto-moments` Automatically identifies moments in the transcript that need visual verification (proper nouns, game names, claims, deictic references), extracts frames at those timestamps, and validates the transcript against what's actually shown on screen.

---

## How It Works

1. **You paste a video and a question.** URL (anything yt-dlp supports — YouTube, Loom, TikTok, X, Instagram, plus a few hundred more) or a local path (`.mp4`, `.mov`, `.mkv`, `.webm`).
2. **`yt-dlp` checks captions first.** At `transcript` detail, captioned URLs return without downloading video. For other modes, or when Whisper needs audio, it downloads only what the run needs.
3. **`ffmpeg` extracts frames at the chosen detail.** `efficient` decodes keyframes only (`-skip_frame nokey`, near-instant); `balanced`/`token-burner` use scene-change detection with **adaptive thresholds** — lower for long videos (0.12 at 60+ min), higher for short clips (0.25 at ≤1 min). Large gaps between scenes are filled with uniformly-sampled frames to ensure minimum coverage. JPEGs are 512px wide by default and clamped to 1998px tall for Hermes Read compatibility.
4. **The transcript comes from one of two places.** First try: `yt-dlp` pulls native captions (manual or auto-generated) from the source. Fallback: extract a mono 16 kHz 64 kbps mp3 audio clip and ship it to Whisper — Groq's `whisper-large-v3` (preferred) or OpenAI's `whisper-1`.
5. **Frames + transcript are handed to Hermes.** The script builds a `WatchReport` from all pipeline data — metadata, frames with timestamps and reasons, transcript segments (with word-level timing when available from JSON3 captions).
6. **Optional: LLM-driven moment detection.** With `--auto-moments`, the transcript is analyzed to identify moments needing visual verification — proper nouns, claims, deictic references, speaker identity clues. Frames are extracted at those exact timestamps.
7. **Optional: Batch vision verification.** The agent analyzes key frames with specific questions (not generic "what is shown?"), corrects misspelled names, validates claims, and identifies speakers from visual cues.
8. **Hermes answers grounded in what's actually on screen and in the audio.** Not "based on the description" or "according to the title." It saw the frames. It heard the transcript. It verified the facts.
9. **Cleanup.** The downloaded video file is deleted automatically after frame extraction to save disk space (200MB–1GB per run). Pass `--keep-video` to retain it.

---

## Usage

```
/watch2 https://youtu.be/dQw4w9WgXcQ what happens at the 30 second mark?
/watch2 https://www.tiktok.com/@user/video/123 summarize this
/watch2 ~/Movies/screen-recording.mp4 when does the UI break?
/watch2 https://vimeo.com/123 what tools does she mention?
```

**Focused on a specific section** — denser frame budget, lower token cost:
```
/watch2 https://youtu.be/abc --start 2:15 --end 2:45
/watch2 video.mp4 --start 50 --end 60
/watch2 "$URL" --start 1:12:00  # from 1h12m to end
```

**Detail modes:**

| Mode | Speed | Frames | Best For |
|------|-------|--------|----------|
| `screenshot-first` | ~30s | LLM-driven | Long videos with captions |
| `transcript` | Fastest | 0 | Transcript-only, no video download |
| `efficient` | ~0.5s | 50 | Quick scan, keyframes only |
| `balanced` | ~20s | 100 | General analysis (default) |
| `token-burner` | ~21s | Uncapped | Full visual coverage |

```
/watch2 https://youtu.be/abc --detail transcript     # transcript only
/watch2 https://youtu.be/abc --detail efficient      # fast keyframes
/watch2 https://youtu.be/abc --detail balanced       # scene-aware (default)
/watch2 https://youtu.be/abc --detail token-burner   # uncapped
/watch2 https://youtu.be/abc --detail screenshot-first  # one frame per subtitle
```

**Other options:**

| Flag | Description | Default |
|------|-------------|---------|
| `--timestamps T1,T2,...` | Grab frames at specific timestamps | none |
| `--max-frames N` | Override frame cap | 100 |
| `--resolution W` | Frame width (default 512, use 1024 for on-screen text) | 512 |
| `--fps F` | Override auto-fps (max 2.0) | auto |
| `--output markdown\|json\|both` | Output format | markdown |
| `--whisper groq\|openai` | Force Whisper backend | auto |
| `--no-whisper` | Disable transcription | false |
| `--no-dedup` | Keep near-duplicate frames | false |
| `--keep-video` | Retain downloaded video | false |
| `--cookies` | Use Chrome cookies for yt-dlp (age-restricted videos) | false |
| `--out-dir DIR` | Custom working directory | tmp |
| `--auto-moments` | LLM-driven moment detection for visual verification | false |
| `--max-moments N` | Max key moments to identify | 50 |
| `--min-moments N` | Min key moments to detect | 50 |
| `--stats` | Include analysis stats in output | false |
| `--stats-format telegram\|compact` | Stats output format | telegram |

---

## Frame Budget

Token cost is dominated by frames. Every frame is an image; image tokens add up fast. The auto-fps logic exists so you don't blow your context budget on a sparse scan of a 30-minute video that would have been better answered by a focused 30-second window.

| Duration | Default Frame Budget | Coverage |
|----------|---------------------|----------|
| 30s or less | ~30 frames | Dense |
| 30s — 1 min | ~40 frames | Dense |
| 1 — 3 min | ~60 frames | Comfortable |
| 3 — 10 min | ~80 frames | Sparse but workable |
| 10+ min | 100 frames (capped) | Sparse — re-run with `--start`/`--end` |

When the user names a moment ("around 2:30", "the last 30 seconds"), pass `--start` / `--end`. Focused mode gets denser per-second budgets, capped at 2 fps.

**Frame deduplication** runs by default. A dedup pass drops near-identical frames before they reach Hermes, so the frame budget is spent on distinct content. Use `--no-dedup` to disable.

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

**Runtime dependencies:** `yt-dlp`, `ffmpeg`, `ffprobe` (same as Python version)

---

## API Keys

Captions cover the majority of public videos for free. The Whisper fallback only kicks in when a video has no caption track.

| Capability | Requirement | Cost |
|------------|-------------|------|
| Download + native captions | `yt-dlp` + `ffmpeg` | Free |
| Whisper fallback (preferred) | [Groq API key](https://console.groq.com/keys) | ~$0.04/hr |
| Whisper fallback (alt) | [OpenAI API key](https://platform.openai.com/api-keys) | Standard pricing |
| Disable Whisper | `--no-whisper` | Free, frames-only |

---

## Configuration

Same as Python version: `~/.config/watch/.env`

```bash
GROQ_API_KEY=gsk_...        # Preferred (faster, cheaper)
OPENAI_API_KEY=sk-...        # Fallback
WATCH_DETAIL=balanced        # Default detail mode
SETUP_COMPLETE=true
```

---

## Architecture

```
watch2/
├── src/
│   ├── main.rs           # Pipeline orchestrator (8 steps)
│   ├── cli.rs            # clap CLI definition
│   ├── config.rs         # Config loading (.env) + language detection
│   ├── download.rs       # yt-dlp wrapper + YouTube 2026 support
│   ├── frames.rs         # ffmpeg frame extraction (5 engines)
│   ├── scene.rs          # Scene detection (adaptive threshold)
│   ├── dedup.rs          # Frame dedup (ffmpeg batch pipe)
│   ├── transcript.rs     # JSON3/VTT subtitle parser
│   ├── whisper.rs        # Groq/OpenAI API client (4 retries)
│   ├── moments.rs        # LLM moment detection
│   ├── moment_frames.rs  # Frame-moment matching
│   ├── vision.rs         # Vision verification pipeline
│   ├── vision_batch.rs   # Batch multi-frame analysis
│   ├── corrections.rs    # Transcript corrections
│   ├── synthesis.rs      # Grounded synthesis
│   ├── stats.rs          # Analysis stats + token estimation
│   ├── output.rs         # Markdown/JSON report generator
│   ├── setup.rs          # Preflight checks
│   ├── timestamp.rs      # Timestamp parsing
│   └── error.rs          # Error types (thiserror)
└── skill/
    └── watch/
        └── SKILL.md      # Hermes skill definition
```

---

## Development

```bash
# Run tests
cargo test

# Build release
cargo build --release

# Install
sudo cp target/release/watch2 /usr/local/bin/
```

---

## Related Projects

- [bradautomates/claude-video](https://github.com/bradautomates/claude-video) — Original (7.6k stars)
- [m1crodevil/hermes-video](https://github.com/m1crodevil/hermes-video) — Python version

---

## License

MIT. Built on [yt-dlp](https://github.com/yt-dlp/yt-dlp), [ffmpeg](https://ffmpeg.org). Whisper transcription via [Groq](https://groq.com) or [OpenAI](https://openai.com).

Original: [bradautomates/claude-video](https://github.com/bradautomates/claude-video)
