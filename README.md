# /watch2

> **It watches. It listens. It verifies.**
> Rust-powered video analysis that *sees* frames and *reads* transcripts — then cross-references both to catch what either one misses.

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-2024+-orange.svg)](https://www.rust-lang.org/)
[![Hermes Agent](https://img.shields.io/badge/Hermes-Agent-purple)](https://hermes-agent.nousresearch.com)
[![GitHub stars](https://img.shields.io/github/stars/m1crodevil/hermes-video-rs)](https://github.com/m1crodevil/hermes-video-rs/stargazers)

**Rust rewrite of [hermes-video](https://github.com/m1crodevil/hermes-video)** — same features, 100× faster startup, single binary.

Paste a URL or a local path. Hermes fetches captions, downloads only what it needs, extracts frames at the moments that matter, and cross-references the transcript against what's actually on screen. Auto-captions misspell names? It catches that. A claim doesn't match the visual? It flags that. By the time it answers, it has *seen* the video, *heard* the audio, and *verified* the facts.

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
| **Binary** | 0 (needs Python runtime) | 6.0MB self-contained |
| **Install** | pip + yt-dlp + ffmpeg | Single binary + yt-dlp + ffmpeg + av-scenechange |
| **Tests** | 1,379 LOC | 8,203 LOC (211 passing) |

---

## Use Cases

**Analyze someone else's content.** `/watch2 https://youtu.be/ what hook did they open with?` Hermes looks at the first frames, reads the opening transcript, breaks down the structure. Same for ad creative, competitor launches, podcast intros — anything where the *how* matters as much as the *what*.

**Diagnose a bug from a video.** Someone sends you a screen recording of something broken. `/watch2 bug-repro.mov what's going wrong?` Hermes watches the recording, finds the frame where the issue appears, describes what's on screen, often catches the cause without you ever opening the file.

**Summarize a video.** `/watch2 https://youtu.be/ summarize this` pulls the structure, the key moments, what was actually said and shown. Faster than watching at 2x.

**Cut the hype out of an update video.** `/watch2 https://youtu.be/ what's actually new -- skip the hype` Strip a "game-changer" feature drop down to the few things that matter.

**Turn a playlist into notes.** `/watch2 https://youtu.be/ summarize this to a note` Run it across a series and file a per-video summary, so a channel or course becomes a searchable set of notes instead of hours you have to sit through.

**Catch what captions get wrong.** `/watch2 https://youtu.be/abc --detail transcript-moments` Auto-captions misspell names, garble proper nouns, mishear numbers. The transcript-moments pipeline identifies 50+ key moments, extracts frames at those timestamps, and cross-references the transcript against what's actually on screen. Corrections are grounded in visual evidence, not guesses.

**Verify transcript accuracy with visual evidence.** `/watch2 https://youtu.be/abc --auto-moments` Automatically identifies moments in the transcript that need visual verification (proper nouns, game names, claims, deictic references), extracts frames at those timestamps, and validates the transcript against what's actually shown on screen.

---

## How It Works

1. **You paste a video and a question.** URL (anything yt-dlp supports — YouTube, Loom, TikTok, X, Instagram, plus a few hundred more) or a local path (`.mp4`, `.mov`, `.mkv`, `.webm`).
2. **`yt-dlp` checks captions first.** At `transcript` detail, captioned URLs return without downloading video. For other modes, or when Whisper needs audio, it downloads only what the run needs. **LLM language detection** identifies content language from title+description for optimal subtitle selection.
3. **`ffmpeg` extracts frames at the chosen detail.** `efficient` decodes keyframes only (`-skip_frame nokey`, near-instant); `balanced`/`token-burner` use scene-change detection with **adaptive thresholds** — lower for long videos (0.12 at 60+ min), higher for short clips (0.25 at ≤1 min). Large gaps between scenes are filled with uniformly-sampled frames to ensure minimum coverage. JPEGs are 512px wide by default and clamped to 1998px tall for Hermes Read compatibility.
4. **The transcript comes from one of two places.** First try: `yt-dlp` pulls native captions (manual or auto-generated) from the source. Fallback: extract a mono 16 kHz 64 kbps mp3 audio clip and ship it to Whisper — Groq's `whisper-large-v3` (preferred) or OpenAI's `whisper-1`.
5. **Frames + transcript are handed to Hermes.** The script builds a `WatchReport` from all pipeline data — metadata, frames with timestamps and reasons, transcript segments (with word-level timing when available from JSON3 captions).
6. **Transcript-moments: Phase 1 (prompt generation).** With `--detail transcript-moments`, the transcript is analyzed to identify 50+ key moments that need visual verification — proper nouns, claims, deictic references, speaker identity clues. A `moments_prompt.txt` is generated for the agent.
7. **Transcript-moments: Phase 2 (frame extraction).** The agent writes `key_moments.json`, and watch2 re-runs to extract frames at those exact timestamps. The Rust binary links moments to frames and computes `KeyMomentStats`.
8. **Transcript-moments: Phase 3 (vision analysis).** The agent analyzes key frames with specific questions (not generic "what is shown?"), corrects misspelled names, validates claims, and flags contradictions. Each finding is classified: confirmed, corrected, fabrication, unverified, or partial.
9. **Transcript-moments: Phase 4 (cross-reference + summary).** The agent cross-references transcript text against visual findings, applies corrections, and produces a grounded summary. All data flows through `report.json` — no redundant intermediate files.
10. **Stats + cleanup.** Processing stats are printed if `--stats` is set. The downloaded video file is deleted automatically after frame extraction to save disk space (200MB–1GB per run). Pass `--keep-video` to retain it. Results are cached by default for instant re-runs.

---

## Usage

```
/watch2 https://youtu.be/dQw4w9WgXcQ what happens at the 30 second mark?
/watch2 https://www.tiktok.com/@user/video/123 summarize this
/watch2 ~/Movies/screen-recording.mp4 when does the UI break?
/watch2 https://vimeo.com/123 what tools does she mention?
/watch2 https://youtu.be/abc --detail transcript-moments --min-moments 50
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
| `transcript` | Fastest | 0 | Transcript-only, no video download |
| `transcript-moments` | ~30s phase1 | 4-phase pipeline | Phase1: prompt → Phase2: frames → Phase3: vision → Phase4: cross-ref |
| `screenshot-first` | ~30s | LLM-driven | One frame per subtitle segment |
| `efficient` | ~0.5s | 50 | Quick scan, keyframes only |
| `balanced` | ~20s | 100 | General analysis (default) |
| `token-burner` | ~21s | Uncapped | Full visual coverage |

```
/watch2 https://youtu.be/abc --detail transcript          # transcript only
/watch2 https://youtu.be/abc --detail transcript-moments  # 4-phase moment detection
/watch2 https://youtu.be/abc --detail efficient           # fast keyframes
/watch2 https://youtu.be/abc --detail balanced            # scene-aware (default)
/watch2 https://youtu.be/abc --detail token-burner        # uncapped
/watch2 https://youtu.be/abc --detail screenshot-first    # one frame per subtitle
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
| `--no-cache` | Disable download cache | false |
| `--cache-dir DIR` | Custom cache directory | `~/.cache/watch2` |
| `--auto-moments` | LLM-driven moment detection for visual verification | false |
| `--max-moments N` | Max key moments to identify | 50 |
| `--min-moments N` | Min key moments to detect (auto-calculated if omitted) | auto |
| `--stats` | Include analysis stats in output | false |
| `--stats-format telegram\|compact` | Stats output format | telegram |
| `--fuse-scenes` | Fuse scene boundaries with transcript for better moment detection | false |
| `--no-scene-detection` | Skip av-scenechange scene detection | false |

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

**transcript-moments mode** bypasses this budget entirely — it extracts frames at 50+ agent-identified timestamps (uncapped), so every frame targets a specific claim or entity that needs visual verification.

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

**Runtime dependencies:** `yt-dlp`, `ffmpeg`, `ffprobe`, `av-scenechange` (mandatory for all video modes)

---

## API Keys

Captions cover the majority of public videos for free. The Whisper fallback only kicks in when a video has no caption track.

| Capability | Requirement | Cost |
|------------|-------------|------|
| Download + native captions | `yt-dlp` + `ffmpeg` | Free |
| Whisper fallback (preferred) | [Groq API key](https://console.groq.com/keys) | ~$0.004/min |
| Whisper fallback (alt) | [OpenAI API key](https://platform.openai.com/api-keys) | Standard pricing |
| Disable Whisper | `--no-whisper` | Free, frames-only |

---

## Configuration

Same as Python version: `~/.config/watch/.env`

```bash
GROQ_API_KEY=gsk_...        # Optional — for Whisper fallback
OPENAI_API_KEY=sk-...        # Optional — alternative Whisper provider
WATCH_DETAIL=balanced        # Default detail mode
SETUP_COMPLETE=true
```

### API Key (Optional)

watch2 can run **without** a Whisper API key when subtitles are available via yt-dlp.

- **With API key**: Whisper fallback available for videos without subtitles
- **Without API key**: Only works with videos that have auto/manual captions
- **`--no-whisper`**: Suppresses the "no API key" warning, skips Whisper entirely

When no subtitles are found and no API key is set, watch2 will print:
```
⚠️  No subtitles found for this video.
    Whisper API key required for transcription.
    Set GROQ_API_KEY or OPENAI_API_KEY in ~/.config/watch/.env
    Or use --no-whisper to skip (no transcript available)
```

---

## Architecture

```
watch2/
├── src/
│   ├── main.rs             # Entry point
│   ├── cli.rs              # clap CLI definition
│   ├── config.rs           # Config loading (.env) + language whitelist
│   ├── cache.rs            # Video/subtitle caching (SHA256, 10GB max)
│   ├── download.rs         # yt-dlp wrapper + YouTube 2026 support
│   ├── llm.rs              # LLM language detection (Groq → OpenAI)
│   ├── pipeline.rs         # Pipeline orchestrator (10 steps, 4 phases)
│   ├── frames/             # Frame extraction (8 engines)
│   │   ├── mod.rs          # Re-exports + auto-fps logic
│   │   ├── keyframe.rs     # Keyframe extraction (-skip_frame nokey)
│   │   ├── uniform.rs      # Uniform sampling
│   │   ├── scene.rs        # Scene-based extraction + gap-fill
│   │   ├── two_pass.rs     # Token-burner two-pass extraction
│   │   ├── gap_fill.rs     # Fill gaps between scene cuts
│   │   ├── metadata.rs     # ffprobe metadata extraction
│   │   └── timestamp.rs    # Extract at specific timestamps
│   ├── scene.rs            # Scene detection (adaptive threshold)
│   ├── scene_detect.rs     # av-scenechange integration
│   ├── dedup.rs            # Frame dedup (ffmpeg batch pipe, 16x16 thumbs)
│   ├── transcript.rs       # JSON3/VTT subtitle parser
│   ├── whisper.rs          # Groq/OpenAI API client (4 retries)
│   ├── moments.rs          # LLM moment detection + prompt generation
│   ├── moment_frames.rs    # Frame-moment matching + vision pipeline
│   ├── fusion.rs           # Scene + transcript fusion alignment
│   ├── vision.rs           # Vision verification + batch analysis (1026 LOC)
│   ├── corrections.rs      # Transcript corrections (punctuation-preserving)
│   ├── synthesis.rs        # Grounded synthesis (transcript + visual evidence)
│   ├── stats.rs            # Analysis stats + token estimation
│   ├── output.rs           # Markdown/JSON report generator (WatchReport)
│   ├── setup.rs            # Preflight checks (binaries, API keys, permissions)
│   ├── timestamp.rs        # Timestamp parsing (SS, MM:SS, HH:MM:SS)
│   └── error.rs            # Error types (thiserror)
└── tests/                  # Integration tests (211 passing)
    ├── test_cli.rs
    ├── test_config.rs
    ├── test_download.rs
    ├── test_frames.rs
    ├── test_output.rs
    ├── test_timestamp.rs
    └── test_transcript.rs
```

---

## Development

```bash
# Run all tests
cargo test

# Run specific test suite
cargo test test_frames
cargo test test_transcript
cargo test test_output

# Build release
cargo build --release

# Install
sudo cp target/release/watch2 /usr/local/bin/

# Run with verbose output
RUST_LOG=debug cargo run -- --help
```

---

## Transcript-Moments Pipeline

The `--detail transcript-moments` mode runs a 4-phase pipeline that combines transcript intelligence with visual verification:

```
Phase 1 (Rust): transcript → moments_prompt.txt (LLM prompt for moment detection)
    ↓ Agent reads prompt, identifies 50+ key moments
Phase 2 (Rust): key_moments.json → frames extracted at timestamps → report.json
    ↓ Agent reads report.json, analyzes frames via vision_analyze
Phase 3 (Agent): vision findings → classify (confirmed/corrected/fabrication/unverified)
    ↓ Cross-reference gate: transcript × vision × scene
Phase 4 (Agent): corrections → grounded summary
```

**Data flow**: `report.json` is the single source of truth. The Rust binary outputs structured data; the agent reads it directly. No intermediate JSON files needed.

**Cross-reference methodology**: Every vision finding is classified into 5 categories:
- ✅ **confirmed** — vision matches transcript
- 🔧 **corrected** — vision shows different spelling/entity
- ❓ **fabrication** — claim has no visual evidence
- ⚠️ **unverified** — cannot determine from visual alone
- 🔸 **partial** — partially shown on screen

---

## Related Projects

- [bradautomates/claude-video](https://github.com/bradautomates/claude-video) — Original (7.6k stars)
- [m1crodevil/hermes-video](https://github.com/m1crodevil/hermes-video) — Python version

---

## License

MIT. Built on [yt-dlp](https://github.com/yt-dlp/yt-dlp), [ffmpeg](https://ffmpeg.org). Whisper transcription via [Groq](https://groq.com) or [OpenAI](https://openai.com).

Original: [bradautomates/claude-video](https://github.com/bradautomates/claude-video)
