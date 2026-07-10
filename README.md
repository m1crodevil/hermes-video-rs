# hermes-video-rs

**Rust rewrite of [hermes-video](https://github.com/m1crodevil/hermes-video)** — fast, single-binary video analysis for [Hermes Agent](https://github.com/NousResearch/hermes-agent).

## Why Rust?

| | Python (hermes-video) | Rust (hermes-video-rs) |
|---|---|---|
| **Binary size** | N/A (requires Python + deps) | 7.3MB single binary |
| **Startup time** | ~200ms (Python import) | ~5ms |
| **Memory** | ~50-80MB | ~5-15MB |
| **Dependencies** | Python 3.11, pip, yt-dlp, ffmpeg | yt-dlp, ffmpeg (runtime) |

## Install

```bash
# From source
git clone https://github.com/m1crodevil/hermes-video-rs
cd hermes-video-rs
cargo build --release
cp target/release/watch-rs /usr/local/bin/

# Or via Hermes skill
hermes skill add hermes-video-rs
```

## Usage

```bash
# Analyze a YouTube video
watch-rs "https://youtu.be/abc123"

# Local file
watch-rs ~/Videos/recording.mp4

# Detail modes
watch-rs "https://youtu.be/abc" --detail transcript    # No frames
watch-rs "https://youtu.be/abc" --detail efficient     # Keyframes, cap 50
watch-rs "https://youtu.be/abc" --detail balanced      # Scene-aware, cap 100
watch-rs "https://youtu.be/abc" --detail token-burner  # Uncapped

# Focus on a section
watch-rs "https://youtu.be/abc" --start 0:30 --end 1:00

# Force Whisper backend
watch-rs "https://youtu.be/abc" --whisper groq
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
| `--timestamps T` | Comma-separated timestamps | none |
| `--whisper groq\|openai` | Force Whisper backend | auto |
| `--no-whisper` | Disable Whisper fallback | false |
| `--no-dedup` | Keep duplicate frames | false |
| `--out-dir DIR` | Working directory | tmp |

## Configuration

Same as Python version: `~/.config/watch/.env`

```bash
GROQ_API_KEY=gsk_...        # Preferred (faster, cheaper)
OPENAI_API_KEY=sk-...        # Fallback
WATCH_DETAIL=balanced        # Default detail mode
SETUP_COMPLETE=true
```

## Architecture

```
watch-rs/
├── src/
│   ├── main.rs           # Pipeline orchestrator
│   ├── cli.rs            # clap CLI definition
│   ├── config.rs         # Config loading (.env)
│   ├── download.rs       # yt-dlp CLI wrapper
│   ├── frames.rs         # ffmpeg frame extraction
│   ├── scene.rs          # Scene detection (ffmpeg scdet)
│   ├── transcript.rs     # JSON3/VTT subtitle parser
│   ├── whisper.rs        # Groq/OpenAI API client
│   ├── dedup.rs          # Frame dedup (image crate)
│   ├── output.rs         # Markdown report generator
│   ├── setup.rs          # Preflight checks
│   ├── timestamp.rs      # Timestamp parsing
│   └── error.rs          # Error types
└── skill/
    └── watch/
        └── SKILL.md      # Hermes skill definition
```

## How It Works

1. **Download** — yt-dlp fetches video + captions (supports 1800+ platforms)
2. **Captions** — Parse JSON3/VTT subtitles (free, instant)
3. **Frames** — ffmpeg extracts frames at auto-scaled FPS (scene-aware or keyframe)
4. **Transcript** — Whisper API fallback if no captions (Groq preferred)
5. **Output** — Markdown report with frame paths + timestamped transcript

## Dependencies

**Runtime:** yt-dlp, ffmpeg, ffprobe (same as Python version)
**Build:** Rust 2024+, cargo

## License

MIT
