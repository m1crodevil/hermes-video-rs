---
name: watch2
version: "8.2.0"
description: "Use when analyzing a video URL or local video with transcript and selected frames."
argument-hint: " <url-or-path> [question]"
allowed-tools: Bash, Read, AskUserQuestion
homepage: https://github.com/m1crodevil/hermes-video-rs
repository: https://github.com/m1crodevil/hermes-video-rs
license: MIT
user-invocable: true
platforms: [macos, linux]
metadata:
  hermes:
    tags: [video, multimodal, rust]
    category: content-creation
    requires_toolsets: [terminal]
linked_files:
  - references/agent-workflow.md
  - references/frame-extraction.md
  - references/scene-detection.md
  - references/transcript-features.md
  - references/configuration.md
  - references/pitfalls.md
---

# /watch2

Use `watch2` to collect video evidence. The Rust binary downloads media, parses captions, detects scenes, and extracts frames only at timestamps selected by the agent. The agent performs selection, vision review, and conclusions.

## Required workflow

```bash
# Pass 1: collect metadata, transcript, and scene boundaries.
watch2 "URL_OR_PATH" --out-dir /tmp/watch-XXX --output json

# Read report.json with jq. Do not use Python for this workflow.
jq '{title, uploader, language, duration, scene_count}' /tmp/watch-XXX/report.json
jq -r '.transcript[] | "[\(.start) → \(.end)] \(.text)"' /tmp/watch-XXX/report.json

# Select evidence timestamps, then extract only those frames.
watch2 "URL_OR_PATH" \
  --out-dir /tmp/watch-XXX \
  --keep-video \
  --timestamps "00:30,01:15,02:45" \
  --output json

# report.json is refreshed on every output mode.
jq '.frames[] | {path, timestamp, reason}' /tmp/watch-XXX/report.json
```

## Rules

- Extract frames only with `--timestamps`; no timestamps means no frames.
- Select timestamps from transcript and scene boundaries. Add enough coverage for the video duration.
- Inspect every extracted frame before making visual claims.
- Use `jq` for report inspection, never Python helpers.
- Check `report.json.analysis_capabilities.visual_verification` before writing visual claims. It is true only when frames exist.
- On YouTube HTTP 403, do not retry or claim visual analysis. Configure a yt-dlp PO-token provider, pass `--cookies-file` (0600), use a local video, or explicitly use `--allow-transcript-only`.
- If `watch2` fails, inspect its error; use `ffprobe` or `ffmpeg` only for diagnosis or a documented manual fallback.
- Return user-facing conclusions, not workflow logs or raw frame-by-frame notes unless requested.

## CLI

| Flag | Meaning |
|---|---|
| `--out-dir DIR` | Working directory; always receives `report.json` |
| `--timestamps T` | Comma-separated frame timestamps |
| `--resolution W` | Frame width; 128–4096, default 512 |
| `--keep-video` | Keep the downloaded source video |
| `--cookies` | Use Chrome cookies for yt-dlp |
| `--cookies-file PATH` | Use a permission-restricted Netscape cookie file |
| `--allow-transcript-only` | Allow a report without visual evidence after a video-stream 403 |
| `--no-whisper` | Disable Groq/OpenAI transcription fallback |
| `--output markdown|json|both` | Choose stdout format; JSON file is always written |

## Dependencies

`watch2`, `yt-dlp` (URLs), `ffmpeg`, `ffprobe`, `av-scenechange`, and `jq`.

## Output shape

`report.json` contains metadata, transcript, scene boundaries, extracted frames, and warnings. It does **not** contain agent-selected key moments or LLM analysis.
