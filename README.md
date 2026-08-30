# watch2

Rust video-evidence extraction for AI agents.

`watch2` downloads a video, collects captions and scene boundaries, and extracts frames only at timestamps chosen by the invoking agent. It does not select moments or make content judgments.

## Install

```bash
git clone https://github.com/m1crodevil/hermes-video-rs
cd hermes-video-rs
cargo build --release
```

Runtime tools: `yt-dlp`, `ffmpeg`, `ffprobe`, and `av-scenechange`.

## Workflow

```bash
# Pass 1: collect evidence. report.json is always written to --out-dir.
watch2 "https://youtu.be/VIDEO" --out-dir /tmp/watch --output json

# Agent reads /tmp/watch/report.json and selects evidence timestamps.

# Pass 2: extract the selected frames and refresh report.json.
watch2 "https://youtu.be/VIDEO" \
  --out-dir /tmp/watch \
  --keep-video \
  --timestamps "00:30,01:15,02:45" \
  --output json
```

The report contains metadata, a timestamped transcript, scene boundaries, extracted-frame paths, and warnings. The agent owns moment selection, visual analysis, and conclusions.

## CLI

| Flag | Meaning |
|---|---|
| `--out-dir DIR` | Working directory; receives `report.json` |
| `--timestamps T` | Comma-separated frame timestamps (`MM:SS`, `HH:MM:SS`, or seconds) |
| `--resolution W` | Frame width, 128–4096; default 512 |
| `--keep-video` | Keep the downloaded source video |
| `--cookies` | Use Chrome cookies for yt-dlp |
| `--no-whisper` | Disable Groq/OpenAI transcription fallback |
| `--no-cache` | Disable the download cache |
| `--cache-dir DIR` | Override cache location |
| `--output markdown\|json\|both` | Select stdout format; `report.json` is still written |

## Transcription fallback

When captions are unavailable, set one key in `~/.config/watch/.env` or the environment:

```bash
GROQ_API_KEY=gsk_...
# or
OPENAI_API_KEY=sk_...
```

## Development

```bash
cargo fmt --check
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo build --release
```

## Design

- One Rust binary; no Python runtime.
- Agent-selected timestamps are the only frame-extraction input.
- `report.json` is a durable handoff artifact for both workflow passes.
- Video, subtitles, and metadata are cached to avoid redundant downloads.

MIT License.
