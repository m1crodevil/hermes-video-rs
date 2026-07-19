---
name: watch2
version: "4.5.0"
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
- Edit/cut video → use `ffmpeg` directly, or **OxiMedia** (pure Rust, see [references/rust-video-editing.md](references/rust-video-editing.md))
- Audio transcription only → use `whisper` directly
- Video without audio and no captions → use `--detail efficient` (keyframes only)
- Need trim/merge/timeline editing → see [references/rust-video-editing.md](references/rust-video-editing.md) for OxiMedia (pure Rust) or FFmpeg CLI

## Output Philosophy

The user wants to understand what the video is about. Your job is to deliver a comprehensive, well-structured analysis of the video's content — like a thorough article review.

**DO**: Summarize key arguments, main findings, conclusions, important quotes, and context. Structure it for readability. Match the user's language (Indo/English mix is fine).

**DON'T**: Show your work process. No cross-reference tables, no correction sections, no frame-by-frame notes, no verification trails. The analytical rigor happens internally; the output is the result.

**What the output IS**: A comprehensive understanding of the video's content — what it's about, key arguments, main findings, conclusions. Like a well-written article review.

**What the output is NOT**: A process report showing how the agent verified each claim, which frames were analyzed, or what corrections were applied.

## Output Format (Telegram)

Always use this structure when delivering watch2 results:

```
🎬 **[Video Title]**
Channel: [Uploader] · Published: [date] | Duration: [time]

---

[Comprehensive analysis content — what the video is about, key findings, main arguments, conclusions]

---
```

**Rules:**
- Use `**bold**` for title only
- Use `·` (middle dot) as separator, not `|` or `,`
- Keep metadata compact on 1-2 lines
- Add `---` separator before and after main content
- **NEVER** use raw markdown table syntax (`| col | col |`) in Telegram output
- **Stats block is OPTIONAL** — include only if the user specifically asks for processing stats
- **NEVER output**: cross-reference tables, correction sections, verification trails, or frame-by-frame notes

## Rust-Only Rule (MANDATORY)

**NEVER fall back to Python scripts when watch2 fails.** This skill is the Rust version. Users who install `/watch2` may NOT have the Python `/watch` skill. Python is not a dependency.

When watch2 fails:
1. Try a different `--detail` flag first
2. Check error output from watch2, diagnose the specific failure
3. Use `ffprobe`/`ffmpeg` CLI directly for metadata checks (these are system tools, not Python)
4. If the Rust binary has a bug, report it — don't work around it

The **only** acceptable manual interventions:
1. Using `ffprobe` to check video metadata when watch2 reports zero duration (diagnostic)
2. Using `ls` to verify subtitle files exist (diagnostic)
3. Using `ffmpeg` to extract frames at specific timestamps when watch2's duration detection fails but ffprobe confirms valid duration (workaround for known bug)

## Binary

```bash
which watch2 || echo "Install: cp ~/hermes-video-rs/target/release/watch2 /usr/local/bin/"
which av-scenechange || echo "Install: cargo install av-scenechange --features ffmpeg"
```

### Mandatory Dependencies

| Binary | Purpose | Required |
|--------|---------|----------|
| `watch2` | Main binary | ✅ |
| `av-scenechange` | Scene detection (mandatory) | ✅ |
| `ffmpeg` | Frame extraction, video processing | ✅ |
| `ffprobe` | Video metadata | ✅ |
| `yt-dlp` | Video download | ✅ (URLs) |

**av-scenechange** is mandatory and runs on ALL modes that process video (balanced, efficient, token-burner, transcript-moments Phase 2). It is NOT used for `--detail transcript` (text-only mode).

Installation:
```bash
cargo install av-scenechange --features ffmpeg
# Verify:
which av-scenechange
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
- [ ] Step 3a: **Schema** — `key_moments.json` MUST use this exact format:
  ```json
  [
    {
      "timestamp": 54.0,
      "timestamp_fmt": "0:54",
      "word": "Ragnarok",
      "context": "Ya kan Ragnarok. Tahu Raknarok?",
      "reason": "proper_noun",
      "question": "What game name is displayed on screen?",
      "priority": 1,
      "frame_path": null
    }
  ]
  ```
  - `timestamp` — **f64 seconds** (not MM:SS string!). `0:54` → `54.0`, `2:30` → `150.0`
  - `timestamp_fmt` — MM:SS string, auto-derived from `timestamp`. Agent MUST provide this.
  - `frame_path` — always `null` (binary fills it after extraction)
  - Missing `timestamp_fmt` or `frame_path` → Rust binary parse error
- [ ] Step 4: Re-run `watch2` with same args including `--out-dir <FIXED_DIR>` (video downloads + frames extracted at all moment timestamps)
- [ ] Step 5: `vision_analyze` 21+ representative frames (from the 50+ extracted) with specific questions from `key_moments.json`
  - Collect findings in memory — do NOT write intermediate JSON files (vision_results.json, corrections.json, etc.)
  - Each finding must be classified: confirmed, corrected, fabrication, unverified, partial
- [ ] Step 6: Cross-reference gate — transcript × vision × scene (INTERNAL — do NOT output this)
  - Read `report.json` for structured data (frames, key_moments, stats)
  - Cross-check every vision finding against transcript text
  - Classify internally: confirmed ✅, corrected 🔧, fabrication ❓, unverified ⚠️, partial 🔸
  - **RULE: Do NOT generate summary until all findings are classified**
  - **Do NOT output the cross-reference table** — it's for your accuracy check only
- [ ] Step 7: Generate grounded summary
  - Apply corrections to transcript mentally, then generate summary
  - If corrections exist and are significant (e.g., vision reveals a name misspelling, a number wrong), mention them briefly in the summary
  - If no meaningful corrections (common for podcast/talk show formats), skip corrections entirely
  - **Do NOT output a corrections section or cross-reference table** — the user wants comprehensive understanding of the video content, not a process transparency report

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

### Frame Count Verification Gate (MANDATORY)

**After ANY frame extraction method (watch2 automatic OR manual ffmpeg), BEFORE proceeding to vision analysis:**

1. Count extracted frames: `ls <workdir>/frames/*.jpg | wc -l`
2. **If count < 21**: STOP. Do NOT proceed with vision analysis on fewer than 21 frames.
3. Fix the extraction first:
   - If watch2 failed → use manual ffmpeg with calculated fps (duration ÷ 21)
   - If scene detection too few → switch to uniform extraction
   - If video is short (<3 min) → extract at every 5 seconds
4. Re-count. Only proceed when ≥21 frames confirmed.

**Why 21 minimum**: Fewer frames = blind spots in visual analysis. A 7-minute video needs at least one frame every 20 seconds to catch all visual context. Skipping this produces shallow, unreliable analysis.

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
| **av-scenechange** | All modes with video (mandatory) | `av-scenechange --min-scenecut 24` → JSON scene boundary list. Runs on balanced, efficient, token-burner, transcript-moments Phase 2 |
| **scene** | balanced fallback | ffmpeg `select='gt(scene,T)'` → extract at cuts. T is adaptive (0.12-0.25 based on duration) |
| **two-pass** | token-burner | Pass 1: av-scenechange (uncapped). Pass 2: uniform at 50% density. Merge + dedup |
| **keyframe** | efficient, ≥4 I-frames | ffmpeg `-skip_frame nokey` → I-frames only |
| **uniform** | fallback when scene/keyframe too few | Fixed fps extraction |
| **gap-fill** | balanced, large gaps between scenes | Uniform frames inserted in gaps >2× expected interval |
| **transcript-cue** | `--timestamps` flag | One frame per timestamp (pinned) |

**av-scenechange** output format: `{"scene_changes":[0,24,50,...]}` (frame numbers). Parsed into `SceneBoundary` structs with start/end timestamps, frame ranges, and durations.

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

## Stats Collection (Optional)

Stats are useful for debugging or when the user asks about processing time. By default, do NOT include stats in the output.

**When stats are needed:** User asks "how long did it take?", "how many frames?", or similar.

**How to get stats:**

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

## Output Reminder

When delivering results: **analysis content only**. Title + metadata + comprehensive summary. No process artifacts, no correction tables, no stats block (unless asked), no work dir footer.

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

**If ffprobe shows valid duration but watch2 reports 0:** This is a bug in `frames/metadata.rs`. Report it on GitHub. However, manual ffmpeg extraction IS acceptable as a workaround when:
1. ffprobe confirms valid duration
2. The video file exists and is not corrupt
3. You extract frames at specific timestamps from `key_moments.json`

**Manual extraction workaround (when duration bug blocks watch2):**
```bash
# Build a batch extraction script from key_moments.json
# Convert timestamps to seconds, extract one frame per moment
for each moment in key_moments.json:
  ffmpeg -y -ss <seconds> -i video.mp4 -frames:v 1 -q:v 2 frames/frame_NNNN.jpg
```
This produces equivalent output to watch2's transcript-moments mode. Always verify frame count ≥21 after extraction.

### Subtitle Detection (Fixed in v4.4.0+)

**Previously**: watch2 could say "no captions" even when `.json3` files existed in the download directory. Root cause was `Path::extension()` returning `"json3"` (no dot) but code comparing with `".json3"` (with dot) — the comparison always failed.

**Current status**: Fixed. `find_video()` and `find_subtitle()` now use correct extension patterns without dot prefix.

**Rust gotcha for future contributors:** `std::path::Path::extension()` returns the extension WITHOUT the dot (`"json3"`, not `".json3"`). Always compare against `"json3"`, never `".json3"`. This bug existed for months because the code "looked correct" — the dot prefix is a natural assumption from other languages (Python's `os.path.splitext` returns with dot).

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

**Pattern:**
```bash
# Run watch2
watch2 "https://youtu.be/abc" --detail efficient

# Analyze 21+ frames (MINIMUM — see Frame Count Verification Gate)
vision_analyze(frame_0001.jpg)  # First frame
vision_analyze(frame_0011.jpg)  # Middle
vision_analyze(frame_0021.jpg)  # End
# ... continue for all 21+ frames
```

**⚠️ NEVER analyze fewer than 21 frames.** The minimum exists because fewer frames = blind spots in visual analysis. See [[Frame Count Verification Gate]] and [[Agent Shortcut: Analyzing Fewer Than 21 Frames]] pitfalls.

### Agent Shortcut: Analyzing Fewer Than 21 Frames (COMMON MISTAKE)

**What happens**: watch2 fails (duration bug, extraction error), agent falls back to manual ffmpeg, extracts 15-20 frames, analyzes only 5-8 with `vision_analyze` to "save API calls", delivers shallow analysis.

**Why it's wrong**: The 21-frame minimum exists because fewer frames = blind spots. A 7-minute video needs ~1 frame per 20 seconds minimum to catch all visual context.

**Root cause chain** (from real session):
1. watch2 duration detection bug → 0 frames extracted
2. Agent manually extracts ad-hoc timestamps (not calculated) → <21 frames
3. Agent "saves cost" by analyzing only a subset → shallow analysis

**Prevention**:
- After ANY frame extraction (watch2 automatic OR manual ffmpeg), **VERIFY count ≥21** before proceeding
- If <21: calculate fps = duration / 21, extract uniform, re-count
- Never "sample strategically" below 21 — that's a cost-optimization shortcut that sacrifices accuracy
- See: [[Frame Count Verification Gate]] section above

**If this happens again**: STOP. Extract more frames. Do NOT deliver analysis with <21 frames.

### Don't Skip transcript-moments for Captioned Videos

**MISTAKE**: Running `--detail efficient` or `--detail balanced` on a video that has captions, then only analyzing 3-5 frames with `vision_analyze`. This misses the entire transcript-moments pipeline and produces shallow analysis.

**CORRECT**: Follow the [Workflow](#workflow) section — Transcript-First Mode. If the video has captions (check for `.json3` or `.vtt` files in the download directory), ALWAYS use `--detail transcript-moments` for deep analysis.

**Why this matters**: Auto-captions (especially non-English) contain errors — misspelled proper nouns, garbled names, incorrect claims. The transcript-moments pipeline catches these by combining transcript intelligence with visual verification.

**When transcript-moments falls through**: If watch2 reports "No subtitles found" despite `.json3` files existing (Bug #5 or similar), do NOT fall back to scene detection. Instead:
1. Parse the `.json3` transcript manually (use the JSON3 parser from the Manual Fallback Pipeline)
2. Analyze the transcript to identify 50+ key moments with timestamps
3. Extract frames at those specific timestamps using `--timestamps` flag or manual ffmpeg
4. Vision-analyze those targeted frames

This is the Manual Fallback Pipeline — it produces equivalent output to transcript-moments at the cost of more manual steps. Never shortcut to "scene detection → sample 21 evenly" when captions exist.

### Finding Top Moments in Transcript

After extracting the transcript (either from watch2 or manual JSON3 parsing), use `search_files` with regex to find the most dramatic/impactful moments:

```bash
search_files \
  --pattern "70% chance|extinction|dictator|scary|chilling|lost.*million" \
  --path /tmp/watch-XXX/transcript.txt \
  --output-mode content \
  --limit 60
```

Cross-reference with frame timestamps to confirm visual context, then compile top 10-15 moments as a table with: `# | Timestamp | Topic | Quote`.

### Vision Model Misidentifying Speakers

**Symptom**: `vision_analyze` confidently identifies speakers as famous people (Ryan Holiday, Grant Cardone, Graham Stephan, etc.) when the actual speakers are unknown podcast hosts/guests.

**Why it happens**: Vision models trained on internet images associate facial features and settings with known personalities. A man in a podcast setup with a microphone gets matched to famous podcasters.

**Impact**: Speaker identification from vision alone is unreliable. Do NOT use `vision_analyze` output for speaker identity claims.

**Workaround**: Rely on transcript context for speaker identity. The transcript's `>>` markers and conversation flow identify who's speaking. Use vision for:
- On-screen text/graphics verification
- Visual context (setting, props, gestures)
- Claim verification (numbers, products, logos shown)

**If you need speaker identification**: Use transcript metadata (video title, channel name, description) rather than visual recognition.

### Podcast/Interview Videos With No On-Screen Graphics

**Symptom**: Video is a pure conversation format — two people talking with microphones, no text overlays, no graphics, no visual aids.

**Impact**: The transcript-moments pipeline still extracts frames at key timestamps, but `vision_analyze` can only describe the speakers' expressions and setting. It CANNOT verify transcript claims visually (no numbers, text, or graphics to cross-reference).

**How to handle**:
1. Extract frames anyway (maintains ≥21 frame minimum for visual coverage)
2. Analyze frames for: speaker expressions, body language, setting details
3. Note in analysis: "No on-screen graphics — transcript claims unverified visually"
4. Focus analysis depth on transcript content rather than visual verification
5. If transcript contains specific claims (numbers, dates, names), flag them as "unverified — no visual confirmation possible"

### Don't Generate Redundant JSON Files (Agent Anti-Pattern)

**MISTAKE**: Agent uses `execute_code` (Python) to write intermediate JSON files during Phase 3+4:
- `vision_results.json` — redundant, findings should be in agent response
- `corrections.json` — redundant, corrections should be applied inline
- `synthesis_prompt.txt` — redundant, synthesis should be generated directly

**Why it's wrong**:
1. `report.json` from the Rust binary already contains ALL structured data (frames, key_moments, stats)
2. Writing intermediate files wastes tokens and creates confusion about source of truth
3. The Rust binary is pure Python-free — using Python to generate files defeats the purpose

**CORRECT workflow**:
```
Phase 3: vision_analyze → collect findings in memory → classify
Phase 4: cross-reference gate → generate corrections + summary inline
```

**Data flow**:
```
report.json (Rust binary) → agent reads → vision_analyze → cross-reference → summary
```

**NOT**:
```
report.json → agent reads → vision_analyze → Python writes vision_results.json → Python writes corrections.json → Python writes synthesis_prompt.txt → summary
```

**If you catch yourself writing `execute_code` to generate JSON during watch2 analysis — STOP. The data should flow through your response, not through files.**

### Verify Frame Filenames Before Vision Calls

**Symptom**: `vision_analyze` returns "file not found" because the frame filename doesn't match what was expected.

**Cause**: Frame naming includes timestamp (e.g., `frame_0025_21_50.jpg`), and it's easy to guess wrong when calling vision_analyze in batch.

**Prevention**: Always `ls` the frames directory first to get exact filenames:
```bash
ls /tmp/watch-XXX/frames/ | sort
```
Then use the exact filenames in `vision_analyze` calls. Don't construct filenames from memory.

## Visual Verification Rules

When analyzing frames — do this internally, don't output the process:

1. **Zero fabrication** — if you can't read text on screen, say "unreadable"
2. **See vs Infer** — distinguish what you SEE from what you INFER
3. **Flag uncertainty** — "appears to be" vs "is"
4. **No assumptions** — don't fill gaps with plausible guesses
5. **Contradictions** — if transcript says X but frame shows Y, incorporate the correction naturally into your summary

## Script Reference

| Script | Purpose | When to Use |
|--------|---------|-------------|
| `moments.rs` | Generate LLM prompt for moment detection | Phase 1 |
| `moment_frames.rs` | Match moments to extracted frames | Phase 2 |
| `vision.rs` | Vision analysis (single + batch, merged) | Phase 3 |
| `corrections.rs` | Apply corrections to transcript | Phase 4 |
| `synthesis.rs` | Generate grounded synthesis prompt | Phase 4 |
| `cache.rs` | Download caching (SHA256 keys, LRU eviction) | All runs |
