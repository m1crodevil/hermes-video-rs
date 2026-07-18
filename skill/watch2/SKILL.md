---
name: watch2
version: "4.2.0"
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

# /watch2

Rust-powered video analysis. Faster startup (~5ms), smaller memory (~5-15MB), single binary (5.4MB).

## When to Use /watch2

- User shares a video URL (YouTube, TikTok, Vimeo, Instagram, etc.)
- User shares a local video file path (.mp4, .mov, .mkv, .webm)
- User asks about video content ("what happens in this video?")
- User wants to analyze/summarize a video

## When NOT to Use /watch2

- Download only → use `yt-dlp` directly
- Edit/cut video → use `ffmpeg` directly
- Audio transcription only → use `whisper` directly
- Video without audio and no captions → use `--detail efficient` (keyframes only)

## Rust-Only Rule (MANDATORY)

**NEVER fall back to Python scripts when watch2 fails.** This skill is the Rust version. Users who install `/watch2` may NOT have the Python `/watch` skill. Python is not a dependency.

When watch2 fails:
1. Try a different `--detail` flag first
2. Check error output from watch2, diagnose the specific failure
3. Use `ffprobe`/`ffmpeg` CLI directly for metadata checks (these are system tools, not Python)
4. If the Rust binary has a bug, report it — don't work around it

The **only** acceptable manual intervention: using `ffprobe` to check video metadata when watch2 reports zero duration, or using `ls` to verify subtitle files exist. These are diagnostic steps, not workarounds.

## Binary

```bash
which watch2 || echo "Install: cp ~/hermes-video-rs/target/release/watch2 /usr/local/bin/"
```

## Quick Start

```bash
# Captioned video (RECOMMENDED — deep analysis)
watch2 "https://youtu.be/abc" --detail transcript-moments --min-moments 50 --out-dir /tmp/watch-TM

# No captions, scene detection
watch2 "https://youtu.be/abc" --detail balanced

# Local file
watch2 ~/Videos/recording.mp4

# Other modes
watch2 "https://youtu.be/abc" --detail transcript        # No frames, transcript only
watch2 "https://youtu.be/abc" --detail efficient         # Keyframes, cap 50
watch2 "https://youtu.be/abc" --detail token-burner      # Two-pass, uncapped
```

## Decision Tree

```
Video has captions (YouTube auto-captions or manual)?
├── YES, video >10 min → --detail transcript-moments (deep analysis) ← ALWAYS FIRST
├── YES, video <10 min → --detail balanced or efficient
├── NO, has audio → --detail balanced (scene detection)
└── NO, no audio → --detail transcript (text only)
```

**Quick check for captions:**
```bash
watch2 "URL" --detail transcript  # Fastest — no video download
# If transcript appears → captions exist → re-run with transcript-moments
```

## Workflow

### Transcript-First Mode (recommended for videos with captions)

When captions are available, this is the **fastest and most accurate** approach:

- [ ] Step 1: Run `watch2 "<source>" --detail transcript-moments --min-moments 50 --out-dir <FIXED_DIR>`
  - First run: generates `moments_prompt.txt` (no video download, ~15s)
  - **CRITICAL: Use `--out-dir` to pin the working directory.** Without it, each run creates a new `/tmp/watch-XXXX` and `key_moments.json` from run 1 is lost on run 2.
  - **VERIFICATION REQUIRED:** After Step 1, check that `<workdir>/moments_prompt.txt` exists. If it does NOT exist:
    1. Check output for "No subtitles found — falling through"
    2. Check if `.json3` files exist in `<workdir>/download/`
    3. If `.json3` files exist → follow Manual Fallback Pipeline (parse transcript → identify moments → extract at timestamps)
    4. If no `.json3` files → video has no captions, use `--detail balanced` instead
    5. **NEVER shortcut to "scene detection → sample 21 evenly" when captions exist**
- [ ] Step 2: Read `<workdir>/moments_prompt.txt`, analyze transcript, identify 50+ key moments
- [ ] Step 3: Write moments as JSON to `<workdir>/key_moments.json`
- [ ] Step 4: Re-run `watch2` with same args including `--out-dir <FIXED_DIR>` (video downloads + frames extracted at all moment timestamps)
- [ ] Step 5: `vision_analyze` 21+ representative frames (from the 50+ extracted) with specific questions from `key_moments.json`
- [ ] Step 6: Apply corrections to transcript, generate grounded summary

### Background Mode (Long Videos >10 min)

For videos longer than 10 minutes, use background mode to avoid terminal timeout:

```bash
# Long video — ALWAYS background
terminal(
  command='watch2 "https://youtu.be/abc" --detail balanced --stats --out-dir /tmp/watch-XXX',
  background=True,
  notify_on_complete=True
)
```

Wait for completion:
1. `process(action='wait', session_id=<from Step 1>, timeout=600)`
2. `process(action='log', session_id=<from Step 1>)` — parse output
3. Parse work dir from `[watch2] working dir: /tmp/watch-XXXX`
4. Proceed with vision analysis on extracted frames

### Fallback: Balanced Mode (no captions)

When no captions are available, use scene detection:

```bash
watch2 "<source>" --detail balanced
```

Then sample ~21 frames evenly and `vision_analyze` strategically.

## CLI Options

| Flag | Description | Default |
|------|-------------|---------|
| `--detail` | transcript/transcript-moments/efficient/balanced/token-burner/screenshot-first | balanced |
| `--max-frames N` | Frame cap override | 100 |
| `--resolution W` | Frame width in px | 512 |
| `--fps F` | Override auto-fps (max 2.0) | auto |
| `--start T` | Range start (SS, MM:SS, HH:MM:SS) | none |
| `--end T` | Range end | none |
| `--timestamps T` | Comma-separated timestamps for cue frames | none |
| `--output` | Output format: markdown, json, both | markdown |
| `--keep-video` | Keep downloaded video after processing | false |
| `--cookies` | Use Chrome cookies (opt-in, breaks android_vr) | false |
| `--auto-moments` | Generate moment detection prompt | false |
| `--max-moments N` | Maximum moments to detect | 50 |
| `--min-moments N` | Minimum moments to detect (auto-calculated if omitted) | auto |
| `--stats` | Show analysis statistics | false |
| `--stats-format` | Stats format: telegram or compact | telegram |
| `--whisper groq\|openai` | Force Whisper backend | auto |
| `--no-whisper` | Disable Whisper fallback | false |
| `--no-dedup` | Keep duplicate frames | false |
| `--out-dir DIR` | Working directory | tmp |
| `--no-cache` | Disable download cache | false |
| `--cache-dir DIR` | Custom cache directory | ~/.cache/watch2/ |

## Detail Modes

| Mode | When | Speed | Accuracy |
|------|------|-------|----------|
| **transcript-moments** | ✅ Video with captions (YouTube auto/manual) | ~15s phase1 + frame extraction | ⭐⭐⭐ Highest |
| **balanced** | Video without captions, need visual coverage | ~300s for 58min | ⭐⭐ Good |
| **efficient** | Quick overview, hard cuts, short videos | ~10-20s | ⭐ Basic |
| **transcript** | Dialogue-heavy, no visual needed | ~5s | ⭐⭐ (audio only) |
| **token-burner** | Max fidelity, short videos | ~500s+ | ⭐⭐⭐ Highest |
| **screenshot-first** | Long videos with captions, speed priority | ~35s | ⭐⭐ Good |

**Rule of thumb:** For videos >10 min with captions, ALWAYS use `transcript-moments`. It's the most accurate path — auto-captions (especially non-English) contain errors that only visual verification catches.

## Frame Engines

| Engine | When Used | How It Works |
|--------|-----------|--------------|
| **scene** | balanced, ≥8 scene changes | ffmpeg `select='gt(scene,T)'` → extract at cuts. T is adaptive (0.12-0.25 based on duration) |
| **two-pass** | token-burner | Pass 1: scene detection (uncapped). Pass 2: uniform at 50% density. Merge + dedup |
| **keyframe** | efficient, ≥4 I-frames | ffmpeg `-skip_frame nokey` → I-frames only |
| **uniform** | fallback when scene/keyframe too few | Fixed fps extraction |
| **gap-fill** | balanced, large gaps between scenes | Uniform frames inserted in gaps >2× expected interval |
| **transcript-cue** | `--timestamps` flag | One frame per timestamp (pinned) |

## Transcript Features

- **Range filtering**: `--start`/`--end` filters both frames AND transcript segments
- **Word-level timing**: JSON3 subtitles include per-word timestamps + ASR confidence
- **Language detection**: Auto-selects best subtitle language (26 languages supported)

## Output Formats

```bash
# Markdown (default)
watch2 video.mp4

# JSON (for programmatic use)
watch2 video.mp4 --output json | jq .

# Both (markdown to stdout + report.json file)
watch2 video.mp4 --output both
```

## Output Format (Telegram)

Always use this structure when delivering watch2 results:

```
🎬 **[Video Title]**
Channel: [Uploader]
Published: [date] | Duration: [time]
Views: [N] · Likes: [N]

---

[Analysis content here]

---

📊 **Analysis Stats**
━━━━━━━━━━━━━━━━━━━━━━━━
⏱️ Processing Time: [X]s
🎬 Video Duration: [time]
🖼️ Frames Extracted: [N] @ [resolution]px ([engine])
📝 Transcript: [N] segments [source]
🎯 Key Moments: [N] detected
━━━━━━━━━━━━━━━━━━━━━━━━

_Work dir: `[path]` — frames + transcript retained._
```

**Rules:**
- Use `**bold**` for title only
- Use `·` (middle dot) as separator, not `|` or `,`
- Keep metadata compact on 1-2 lines
- Add `---` separator before and after main content
- Always include work dir footer
- **NEVER** use raw markdown table syntax (`| col | col |`) in Telegram output
- Always include stats block — compile from `--stats` output or `report.json`

## Stats Collection (MANDATORY)

Always collect stats after watch2 completes — even on timeout or crash. The work directory contains everything needed.

**Primary path (report.json exists):**
Read `<workdir>/report.json` for full metadata.

**Fallback path (report.json missing):**
```bash
# Get duration
ffprobe -v quiet -show_entries format=duration \
  -of default=noprint_wrappers=1:nokey=1 /tmp/watch-XXX/download/video.mp4

# Count frames
ls /tmp/watch-XXX/frames/*.jpg 2>/dev/null | wc -l

# Check transcript
ls /tmp/watch-XXX/download/*.json3 2>/dev/null
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
GROQ_API_KEY=gsk_...        # Optional — for Whisper fallback
OPENAI_API_KEY=sk-...        # Optional — alternative Whisper provider
WATCH_DETAIL=balanced
SETUP_COMPLETE=true
```

### API Key (Optional)

watch2 can run **without** a Whisper API key when subtitles are available via yt-dlp.

- **With API key**: Whisper fallback available for videos without subtitles
- **Without API key**: Only works with videos that have auto/manual captions
- **`--no-whisper`**: Suppresses the "no API key" warning, skips Whisper entirely

When no subtitles are found and no API key is set, watch2 prints a clear explanation:
```
⚠️  No subtitles found for this video.
    Whisper API key required for transcription.
    Set GROQ_API_KEY or OPENAI_API_KEY in ~/.config/watch/.env
    Or use --no-whisper to skip (no transcript available)
```

## YouTube 2026 Support

Auto-detects and uses:
- **deno** — JS runtime for YouTube challenge solving
- **curl_cffi** — Browser impersonation (anti-bot)
- **Chrome cookies** — Authenticated sessions (opt-in via --cookies, breaks android_vr)

No manual flags needed — just ensure deps are installed.

## LLM Features (Agent-Driven)

- `--auto-moments` — generates moment detection prompt for agent
- `--max-moments N` / `--min-moments N` — control moment count
- `--stats` / `--stats-format telegram|compact` — analysis statistics

## Anti-Hallucination Rules

When analyzing frames, you MUST:

1. **Cite timestamps** — every claim must reference a specific `[MM:SS]`
2. **Zero fabrication** — if you can't read text on screen, say "unreadable"
3. **Distinguish** between what you SEE vs what you INFER
4. **Flag uncertainty** — use "appears to be" vs "is"
5. **Cross-reference** — check transcript against visual evidence
6. **No assumptions** — don't fill gaps with plausible guesses
7. **Report contradictions** — if transcript says X but frame shows Y, note it
8. **Source every correction** — "Frame at 2:15 shows 'Ragnarok' not 'Raknarok'"

## Pitfalls

### Video Downloads at Full Quality (No 720p Cap)

**Symptom**: watch2 downloads a 3GB+ video file for a 57-minute YouTube video.

**Cause**: Missing `-f` format flag. Without it, yt-dlp downloads best quality (4K = 3GB).

**Fix** (v4.2.1+): `download.rs` now passes `-f bv*[height<=720]+ba/b[height<=720]/bv+ba/b` and `--merge-output-format mp4`.

**Verify after update:** Check `download.rs` for format string parity with Python version.

### Duration Detection Fails

**Symptom**: watch2 reports `"Video has zero or negative duration (0.00s)"` and produces an empty report.

**Diagnosis (Rust-native, NO Python):**
```bash
OUTDIR="/tmp/watch-XXX"  # Use the --out-dir you passed to watch2

# 1. Verify download exists
ls -la "$OUTDIR/download/"

# 2. Get real duration via ffprobe
ffprobe -v quiet -show_entries format=duration \
  -of default=noprint_wrappers=1:nokey=1 "$OUTDIR/download/video.mp4"

# 3. Check subtitle files
ls "$OUTDIR/download/"*.json3 "$OUTDIR/download/"*.vtt 2>/dev/null
```

**If ffprobe shows valid duration but watch2 reports 0:** This is a bug in `frames/metadata.rs`. Report it on GitHub — do NOT work around with manual extraction.

### Subtitle Detection (Fixed in v4.4.0+)

**Previously**: watch2 could say "no captions" even when `.json3` files existed in the download directory. Root cause was `Path::extension()` returning `"json3"` (no dot) but code comparing with `".json3"` (with dot) — the comparison always failed.

**Current status**: Fixed. `find_video()` and `find_subtitle()` now use correct extension patterns without dot prefix.

**If this still occurs** (shouldn't happen on v4.4.0+):
```bash
# 1. Check what subtitle files exist
ls -la /tmp/watch-XXX/download/*.json3 /tmp/watch-XXX/download/*.vtt

# 2. If files exist, try transcript mode directly
watch2 "URL" --detail transcript --out-dir /tmp/watch-XXX

# 3. If transcript mode also fails, report as bug
```

### Video Not Cleaned Up After Processing

**Symptom**: Downloaded video (potentially GBs) remains on disk after watch2 finishes.

**Check**: `--keep-video` flag was passed? If not, cleanup logic in `main.rs` should auto-delete.

### Vision Analysis is Agent-Driven

**Important**: watch2 outputs frame paths, NOT analyzed images. The agent must call `vision_analyze` on each frame to see the content. Do NOT expect watch2 to return image descriptions.

**Efficient pattern:**
```bash
# Run watch2
watch2 "https://youtu.be/abc" --detail efficient

# Then analyze key frames (not all — expensive)
vision_analyze(frame_0001.jpg)  # First frame
vision_analyze(frame_0015.jpg)  # Middle
vision_analyze(frame_0030.jpg)  # End
```

Analyzing all 30+ frames is expensive (30 API calls). Sample strategically unless the user requests full coverage.

### Don't Skip transcript-moments for Captioned Videos

**MISTAKE**: Running `--detail efficient` or `--detail balanced` on a video that has captions, then only analyzing 3-5 frames with `vision_analyze`. This misses the entire transcript-moments pipeline and produces shallow analysis.

**CORRECT**: Follow the [Workflow](#workflow) section — Transcript-First Mode. If the video has captions (check for `.json3` or `.vtt` files in the download directory), ALWAYS use `--detail transcript-moments` for deep analysis.

**Why this matters**: Auto-captions (especially non-English) contain errors — misspelled proper nouns, garbled names, incorrect claims. The transcript-moments pipeline catches these by combining transcript intelligence with visual verification.

### Finding Top Moments in Transcript

After watch2 extracts the transcript, use `search_files` with regex to find the most dramatic/impactful moments:

```bash
search_files \
  --pattern "70% chance|extinction|dictator|scary|chilling|lost.*million" \
  --path /tmp/watch-XXX/transcript.txt \
  --output-mode content \
  --limit 60
```

Cross-reference with frame timestamps to confirm visual context, then compile top 10-15 moments as a table with: `# | Timestamp | Topic | Quote`.

## Script Reference

| Script | Purpose | When to Use |
|--------|---------|-------------|
| `moments.rs` | Generate LLM prompt for moment detection | Phase 1 |
| `moment_frames.rs` | Match moments to extracted frames | Phase 2 |
| `vision.rs` | Vision analysis (single + batch, merged) | Phase 3 |
| `corrections.rs` | Apply corrections to transcript | Phase 4 |
| `synthesis.rs` | Generate grounded synthesis prompt | Phase 4 |
| `cache.rs` | Download caching (SHA256 keys, LRU eviction) | All runs |
