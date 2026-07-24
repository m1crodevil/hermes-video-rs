---
name: watch2
version: "8.0.0"
description: "Rust-powered video analysis for AI agents — transcript-first with scene detection"
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
linked_files:
  - references/agent-workflow.md
  - references/moment-selection.md
  - references/pitfalls.md
  - references/workflow-details.md
  - references/visual-verification.md
  - references/frame-extraction.md
  - references/scene-detection.md
  - references/transcript-features.md
  - references/reading-report.md
  - references/stats-collection.md
  - references/language-detection.md
  - references/configuration.md
  - references/script-reference.md
  - references/manual-fallback-pipeline.md
---

# /watch2

Rust-powered video analysis. Faster startup (~5ms), smaller memory (~5-15MB), single binary (5.4MB).

## Quick Reference

**Binary:** `watch2 "URL" --out-dir /tmp/watch-XXX --output both`
**Flow:** Run binary (no frames) → Read report.json with jq → Agent selects moments → watch2 --timestamps → Vision analyze → Analysis
**Flags:** --timestamps, --keep-video, --out-dir, --output, --resolution
**Minimum frames:** ≥21 (MANDATORY)
**Transcript required:** Yes — binary exits without it
**Frame analysis:** Analyze EVERY extracted frame with vision_analyze. NEVER skip frames.
**Report parsing:** Use `jq` — NEVER Python (`python3`, `execute_code`).

**Use when:** User shares video URL or local path, asks about video content
**Don't use when:** Download only (yt-dlp), Edit video (ffmpeg), Audio only (whisper)

## When to Use /watch2

- User shares a video URL (YouTube, TikTok, Vimeo, Instagram, etc.)
- User shares a local video file path (.mp4, .mov, .mkv, .webm)
- User asks about video content ("what happens in this video?")
- User wants to analyze/summarize a video

## When NOT to Use /watch2

- Video without audio and no captions
- Download only → use `yt-dlp` directly
- Edit/cut video → use `ffmpeg` directly
- Audio transcription only → use `whisper` directly

## Output Philosophy

The user wants to understand what the video is about. Deliver comprehensive analysis — like a thorough article review.

**DO:** Summarize key arguments, main findings, conclusions, important quotes, context. Match user language. Structure for readability.

**DON'T:** Show work process. No cross-reference tables, no correction sections, no frame-by-frame notes.

**Data flow:** `binary → report.json → agent reads transcript+scenes via jq → agent selects moments → agent calls watch2 --timestamps → binary extracts frames → agent vision_analyze → cross-reference → summary`

**NEVER use `python3`, `execute_code`, or Python scripts** during watch2 analysis. This is a pure Rust pipeline — use `jq` for all JSON parsing.

**STOP when:**
- Analysis is comprehensive (key findings + main arguments + conclusions)
- All cross-references incorporated naturally into summary
- No process artifacts leak into output text

**Frame analysis rule:** After extracting frames, analyze EVERY frame with vision_analyze. Minimum 21 frames. NEVER skip frames to "save API calls" — fewer frames = blind spots in visual analysis.

## Report Parsing (MANDATORY)

Parse `report.json` with `jq`. NEVER use Python.

### Metadata
```bash
jq '{title, uploader, language, duration, engine, scene_count}' /tmp/watch-XXX/report.json
```

### Frame list
```bash
jq '.frames[] | {path, timestamp, reason}' /tmp/watch-XXX/report.json
```

### Transcript
```bash
jq -r '.transcript[] | "[\(.start) → \(.end)] \(.text)"' /tmp/watch-XXX/report.json
```

### Scene boundaries
```bash
jq '.scene_boundaries[] | {start_sec, end_sec, duration_sec}' /tmp/watch-XXX/report.json
```

### Key moments (if available)
```bash
jq '.key_moments[] | {timestamp, reason, question}' /tmp/watch-XXX/report.json
```

## Output Template

🎬 **[Video Title]**
Channel: [Uploader] · Duration: [time]

---
[Comprehensive analysis content — what the video is about, key findings, main arguments, conclusions]
---

**Rules:**
- Use `**bold**` for title only
- Use `·` (middle dot) as separator
- Keep metadata compact on 1-2 lines
- Add `---` separator before and after main content
- **NEVER** use raw markdown table syntax in Telegram output
- **Stats block is OPTIONAL** — include only if user asks
- **NEVER output** cross-reference tables, correction sections, or verification trails

### Example Outputs

**Example 1: Simple video summary**
🎬 **How to Build a REST API in 10 Minutes**
Channel: TechWithTim · Duration: 10:23

---
This video walks through building a REST API using Node.js and Express. The host covers route setup, middleware configuration, and error handling.
---

**Example 2: Cross-reference finding**
The transcript mentions "Ragnarok" at 0:54, but on-screen text shows "Ragnarök" (with umlaut). Common ASR error for Scandinavian names.

**Example 3: Error case (no transcript)**
⚠️ No transcript available. Set GROQ_API_KEY or OPENAI_API_KEY for Whisper.

## Rust-Only Rule (MANDATORY)

**NEVER fall back to Python scripts when watch2 fails.** This is the Rust version.

When watch2 fails:
1. Check error output from watch2, diagnose the specific failure
2. Use `ffprobe`/`ffmpeg` CLI directly for metadata checks (system tools, not Python)
3. If the Rust binary has a bug, report it

**Only acceptable manual interventions:**
1. Using `ffprobe` to check video metadata (diagnostic)
2. Using `ls` to verify subtitle files exist (diagnostic)
3. Using `ffmpeg` to extract frames at specific timestamps when duration detection fails (workaround)

**NEVER acceptable:**
- `python3 -c "..."` for JSON parsing (use `jq`)
- `execute_code` for report extraction (use `jq`)
- Any Python script for any part of the pipeline

## Binary

```bash
which watch2 || echo "Install: cp ~/hermes-video-rs/target/release/watch2 /usr/local/bin/"
which av-scenechange || echo "Install: cargo install av-scenechange --features ffmpeg"
```

### Mandatory Dependencies

| Binary | Purpose | Required |
|--------|---------|----------|
| `watch2` | Main binary | ✅ |
| `av-scenechange` | Scene detection | ✅ |
| `ffmpeg` | Frame extraction, video processing | ✅ |
| `ffprobe` | Video metadata | ✅ |
| `yt-dlp` | Video download | ✅ (URLs) |
| `jq` | JSON parsing (report.json) | ✅ |

## Quick Start

**Recommended workflow (two passes):**

```bash
# Pass 1: Get transcript + scene data (NO frames extracted)
watch2 "https://youtu.be/abc" --out-dir /tmp/watch-XXX --output both

# Read report with jq
jq '{title, uploader, duration, scene_count}' /tmp/watch-XXX/report.json
jq -r '.transcript[] | "[\(.start) → \(.end)] \(.text)"' /tmp/watch-XXX/report.json

# Pass 2: After agent selects 21-25 key moments
watch2 "https://youtu.be/abc" --timestamps "00:30,01:15,02:45,..." --keep-video --out-dir /tmp/watch-XXX
```

**Steps:**
1. Run binary → get report.json (transcript + scene boundaries, NO frames)
2. Agent reads report.json via jq, selects 21-25 key moments using transcript + scene data
3. Run binary again with `--timestamps` → extract frames at key moments
4. Vision analyze ALL extracted frames (minimum 21)

**Why two passes?** Pass 1 provides transcript + scene data for intelligent moment selection. Pass 2 extracts frames only at agent-selected moments — targeted at speaker transitions, topic changes, visual demonstrations. No wasted frames.

## CLI Options

| Flag | Description | Default |
|------|-------------|---------|
| `--resolution W` | Frame width (128–4096) | 512 |
| `--out-dir DIR` | Working directory | temp dir |
| `--keep-video` | Retain video | false |
| `--cookies` | Chrome cookies | false |
| `--no-whisper` | Disable Whisper | false |
| `--no-dedup` | Keep duplicates | false |
| `--output fmt` | markdown/json/both | markdown |
| `--no-cache` | Disable cache | false |
| `--cache-dir DIR` | Cache directory | `~/.cache/watch2` |
| `--timestamps T` | Comma-separated MM:SS | none (no frames) |

## Output Formats

| Format | Command | Use When |
|--------|---------|----------|
| Markdown (default) | `watch2 URL question` | Agent reads directly |
| JSON | `watch2 URL question --output json` | Programmatic processing |
| Both | `watch2 URL question --output both` | Agent + file storage |

The `WatchReport` includes: video metadata, full transcript with word-level timing, scene boundaries, key moment metadata, and warnings. Frames are only present when `--timestamps` is provided.

## Configuration

Config file: `~/.config/watch/.env`

```bash
GROQ_API_KEY=gsk_...        # Optional — for Whisper fallback
OPENAI_API_KEY=sk-...        # Optional — alternative Whisper
SETUP_COMPLETE=true
```

**API key behavior:**
- **With key:** Whisper fallback available
- **Without key:** Only works with videos that have captions
- **`--no-whisper`:** Suppresses warning, skips Whisper

## Output Reminder

When delivering results: **analysis content only**. Title + metadata + comprehensive summary. No process artifacts, no correction tables, no stats block (unless asked), no work dir footer.

## References

For detailed workflows, load reference files via `skill_view`:
- **agent-workflow.md** — Full 8-step workflow with jq commands
- **moment-selection.md** — LLM criteria + prompt template
- **pitfalls.md** — Debugging reference (277 lines)
- **workflow-details.md** — Single-pass pipeline details
- **visual-verification.md** — Zero fabrication rules
- **reading-report.md** — jq extraction commands
- **stats-collection.md** — How to get stats
- **configuration.md** — API keys + YouTube 2026
- **script-reference.md** — Source file map
