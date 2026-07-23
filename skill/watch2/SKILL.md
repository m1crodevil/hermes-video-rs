---
name: watch2
version: "7.1.0"
description: "Watch a video (URL or local path). Rust-powered analysis — single linear pipeline, no mode selection needed."
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
- Video without audio and no captions → binary auto-selects keyframe extraction
- Need trim/merge/timeline editing → see [references/rust-video-editing.md](references/rust-video-editing.md) for OxiMedia (pure Rust) or FFmpeg CLI

## Output Philosophy

The user wants to understand what the video is about. Your job is to deliver a comprehensive, well-structured analysis of the video's content — like a thorough article review.

**DO**: Summarize key arguments, main findings, conclusions, important quotes, and context. Structure it for readability. Match the user's language (Indo/English mix is fine).

**DON'T**: Show your work process. No cross-reference tables, no correction sections, no frame-by-frame notes, no verification trails. The analytical rigor happens internally; the output is the result.

**Data flow**: `report.json` → agent reads → `vision_analyze` → cross-reference in memory → summary in response. All analysis flows through your response text, never through intermediate files.

**NEVER use `execute_code` or Python scripts** during watch2 analysis. The Rust binary is pure Python-free. Write findings in your response, not to JSON files.

## Mandatory Agent Workflow

**After running the binary, agent MUST complete ALL steps. Skipping steps produces shallow analysis.**

### Step-by-Step Checklist

| Step | Action | Required |
|------|--------|----------|
| □ 1 | Run binary: `watch2 "URL" --keep-video --out-dir /tmp/watch-XXX --output both` | ✅ |
| □ 2 | Read `report.json` — get transcript, scenes, metadata | ✅ |
| □ 3 | **LLM: Detect language** from transcript (ISO 639-1 code) | ✅ |
| □ 4 | **LLM: Select 15-20 key moments** from transcript + scene data | ✅ |
| □ 5 | **Extract frames at moment timestamps**: `watch2 "URL" --timestamps "00:30,01:15,..." --out-dir /tmp/watch-XXX --keep-video` | ✅ |
| □ 6 | **Vision analyze ALL frames** (≥21 minimum, no exceptions) | ✅ |
| □ 7 | **Cross-reference**: transcript claims × visual evidence | ✅ |
| □ 8 | **Generate comprehensive analysis** — combining all data sources | ✅ |

### Why Each Step Matters

- **Step 3 (Language)**: Auto-captions often misspell proper nouns. Language context helps accurate moment selection.
- **Step 4 (Moments)**: Uniform sampling misses key moments. LLM selects moments that need visual verification.
- **Step 5 (Additional frames)**: Baseline uniform frames + moment-specific frames = comprehensive visual coverage.
- **Step 6 (Vision)**: Every frame must be analyzed. Skipping frames = blind spots.
- **Step 7 (Cross-reference)**: Transcript says "X shows Y" → vision confirms/denies. This is the core value of the pipeline.
- **Step 8 (Analysis)**: Final output must combine transcript insights, scene context, and visual evidence.

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
1. Check error output from watch2, diagnose the specific failure
2. Use `ffprobe`/`ffmpeg` CLI directly for metadata checks (these are system tools, not Python)
3. If the Rust binary has a bug, report it — don't work around it

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

**av-scenechange** is mandatory for all video processing modes. The binary auto-selects the appropriate pipeline — no manual mode flags needed.

Installation:
```bash
cargo install av-scenechange --features ffmpeg
# Verify:
which av-scenechange
```

## Quick Start

```bash
# Binary auto-selects analysis mode based on video properties
watch2 "https://youtu.be/abc" --out-dir /tmp/watch-XXX --output both

# Local file
watch2 ~/Videos/recording.mp4 --output both

# Custom resolution
watch2 "https://youtu.be/abc" --resolution 720 --out-dir /tmp/watch-XXX
```

## Decision Tree

```
Binary auto-selects analysis mode:
├── Video has captions → transcript-moments (auto-detected)
├── Video without captions → scene detection (auto-detected)
└── Agent reads report.json → vision_analyze on extracted frames
```

## Workflow

### New Architecture (v7.0.0)

The binary handles data extraction only. All intelligence (LLM calls, moment selection, analysis) is done by the agent.

```
Binary (Rust) — NO LLM calls:
  1. Download video + subtitles
  2. Parse transcript (JSON3)
  3. Scene detection (av-scenechange)
  4. If no transcript + no GROQ_API_KEY → STOP with error
  5. Extract uniform frames (baseline)
  6. Output: report.json (transcript + scenes + metadata + frame paths)
  7. Cleanup video

Agent (Hermes) — ALL intelligence:
  1. Read report.json
  2. Language detection via LLM
  3. Select key moments via LLM (using transcript + scene data)
  4. Extract frames at moment timestamps (binary via --timestamps)
  5. Vision analyze all frames (≥21 minimum)
  6. Cross-reference: transcript × scenes × vision
  7. Generate comprehensive final analysis
```

### Single-Run Pipeline (Binary Only)

watch2 performs data extraction in **one pass**:

```bash
# Binary extracts data — agent handles intelligence
watch2 "https://youtu.be/abc" --keep-video --out-dir /tmp/watch-XXX --output both
```

1. **Run watch2** — binary handles everything:
   - Downloads video + subtitles
   - Runs scene detection (av-scenechange)
   - Parses transcript
   - Extracts uniform frames (baseline)
   - Builds report.json
   - **Keeps video** (via `--keep-video`) for additional frame extraction

2. **Agent reads report.json** — contains transcript, scenes, metadata, frame paths

3. **Agent selects key moments via LLM** — using transcript + scene data:
   - Language detection
   - Moment selection (what needs visual verification)
   - Returns timestamps (e.g. "00:30,01:15,02:45,03:30,...")

4. **Agent extracts frames at moment timestamps**:
   ```bash
   watch2 "URL" --timestamps "00:30,01:15,02:45,..." --keep-video --out-dir /tmp/watch-XXX --output both
   ```

5. **Agent vision_analyze all frames** (uniform + moment frames, ≥21 minimum)

6. **Agent cross-references** transcript × scenes × vision

7. **Agent generates comprehensive analysis** — combining all data sources

### Background Mode (Long Videos >10 min)

For videos longer than 10 minutes, use background mode to avoid terminal timeout:

```bash
# Long video — ALWAYS background
terminal(
  command='watch2 "https://youtu.be/abc" --out-dir /tmp/watch-XXX --output both',
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
# Binary auto-selects — just run it
watch2 "<source>" --out-dir /tmp/watch-XXX --output both
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

Binary auto-selects analysis mode internally. No manual mode flags needed.

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

> **Note**: The binary internally selects the optimal analysis pipeline based on captions availability, video duration, and scene complexity. Check the `engine` field in `report.json` to see which pipeline was used.

> **`--timestamps` usage**: When set, the binary extracts frames ONLY at these timestamps (skips uniform extraction). Use this after agent-side moment selection to get frames at key moments.

## Analysis Modes (Internal)

The binary auto-selects the optimal pipeline. Check `report.json` → `engine` field to see which was used:

| Engine | When Selected | Description |
|--------|---------------|-------------|
| `transcript-moments` | Captions available | LLM selects key moments from transcript, frames extracted at those timestamps |
| `scene` | No captions, scene-rich | Scene detection via av-scenechange, frames at scene boundaries |
| `keyframe` | No captions, few scenes | I-frame extraction via ffmpeg |
| `uniform` | Fallback | Fixed fps extraction when other methods yield too few frames |

The agent receives the result via `report.json` which includes frames, transcript, and key moment metadata.

## Frame Engines

| Engine | When Used | How It Works |
|--------|-----------|--------------|
| **av-scenechange** | All modes with video (mandatory) | Rust library API (`Decoder::from_file` + `detect_scene_changes`). Provides scene boundaries with scoring data. |
| **scene** | Fallback when av-scenechange fails | ffmpeg `select='gt(scene,T)'` with adaptive thresholds |
| **keyframe** | No captions, ≥4 I-frames | ffmpeg `-skip_frame nokey` → I-frames only |
| **uniform** | Fallback | Fixed fps extraction when scene/keyframe yield too few frames |
| **transcript-cue** | LLM-selected moments | One frame per key moment timestamp |

**av-scenechange** is now used as a Rust library (not CLI subprocess). The library API returns `DetectionResults` with `scene_changes: Vec<usize>` AND `scores: BTreeMap<usize, ScenecutResult>` containing per-frame scoring data. Uses `SceneDetectionSpeed::Fast` (pixel-wise comparison, 3x faster than Standard mode).

## Transcript Features

- **Range filtering**: `--start`/`--end` filters both frames AND transcript segments
- **Word-level timing**: JSON3 subtitles include per-word timestamps + ASR confidence
- **Language detection**: Auto-selects best subtitle language (26 languages supported)
- **Auto mode selection**: Binary automatically selects optimal pipeline based on transcript availability

## Output Formats

```bash
# Markdown (default)
watch2 video.mp4

# JSON (for programmatic use)
watch2 video.mp4 --output json | jq .

# Both (markdown to stdout + report.json file)
watch2 video.mp4 --output both
```

## Reading report.json

`report.json` contains all structured data. Use `jq` for extraction:

```bash
# Quick metadata
rtk jq '{title, uploader, language, engine}' report.json

# Frame list with timestamps
rtk jq '.frames[] | {path, timestamp, reason}' report.json

# Transcript with timestamps
rtk jq -r '.transcript[] | "[\(.start) → \(.end)] \(.text)"' report.json

# Key moments (if available)
rtk jq '.key_moments[] | {timestamp, reason}' report.json
```

**Avoid**: `cat report.json | python3 -c "..."` — violates Rust-only rule. Use `jq` instead.

## Stats Collection (Optional)

Stats are useful for debugging or when the user asks about processing time. By default, do NOT include stats in the output.

**When stats are needed:** User asks "how long did it take?", "how many frames?", or similar.

**How to get stats from report.json:**

```bash
# Quick metadata
rtk jq '{title, uploader, language, engine}' /tmp/watch-XXX/report.json

# Frame count
rtk jq '.frames | length' /tmp/watch-XXX/report.json

# Transcript segments count
rtk jq '.transcript | length' /tmp/watch-XXX/report.json

# Key moments count
rtk jq '.key_moments | length' /tmp/watch-XXX/report.json

# Duration (if available)
rtk jq '.duration_seconds' /tmp/watch-XXX/report.json
```

**Fallback when report.json missing:**
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

Config file: `~/.config/watch/.env`
```
GROQ_API_KEY=gsk_...        # Required for Whisper fallback only
OPENAI_API_KEY=sk-...        # Alternative Whisper provider
SETUP_COMPLETE=true
```

### API Key (Optional — Whisper Only)

API keys are only needed for Whisper audio transcription (when subtitles are unavailable).

- **With API key**: Whisper fallback available for videos without subtitles
- **Without API key**: Only works with videos that have auto/manual captions
- **`--no-whisper`**: Suppresses the "no API key" warning, skips Whisper entirely

When no subtitles are found and no API key is set, watch2 stops with an error:
```
No transcript available. Set GROQ_API_KEY or OPENAI_API_KEY for Whisper transcription.
```

## YouTube 2026 Support

Auto-detects and uses:
- **deno** — JS runtime for YouTube challenge solving
- **curl_cffi** — Browser impersonation (anti-bot)
- **Chrome cookies** — Authenticated sessions (opt-in via --cookies, breaks android_vr)

No manual flags needed — just ensure deps are installed.

## LLM Features (Agent-Side)

**No direct LLM calls from binary.** All intelligence is handled by the agent.

- **Language detection** — Agent detects language via LLM from transcript
- **Moment selection** — Agent selects key moments via LLM using transcript + scene data
- **Analysis** — Agent generates comprehensive analysis combining all data

**Whisper fallback** — Binary calls Groq/OpenAI API only for audio transcription when subtitles unavailable. Requires `GROQ_API_KEY` or `OPENAI_API_KEY` in `~/.config/watch/.env`.

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

### Subtitle Download Strategy (v4.5.0+)

**How it works:** watch2 detects the video language first via yt-dlp metadata (`--print language`), then downloads only matching subtitles (`--sub-langs "id.*"` instead of `--sub-langs ".*"`). This reduces subtitle requests from ~157 to 1-2 per video.

**Language detection chain:**
1. Quick metadata call: `yt-dlp --skip-download --write-info-json --print language`
2. If language detected → download only matching subs (e.g., `id.*`)
3. If detection fails → fallback to downloading all languages (`".*"` )

**Why targeted download:**
- YouTube rate-limits English auto-captions for non-English videos (HTTP 429)
- Detecting language first → only 1-2 subtitle requests instead of 157
- Faster: ~3-5 sec subtitle download instead of ~8 min
- Lower risk of YouTube 429 rate-limiting

**Tradeoff:** 1 extra metadata request (~1 sec) to detect language before full download.

**If subtitles still fail:**
```bash
# 1. Check what subtitle files exist
ls -la /tmp/watch-XXX/download/*.json3 /tmp/watch-XXX/download/*.vtt

# 2. If files exist, try transcript mode directly
# Binary auto-selects — just run it
watch2 "URL" --out-dir /tmp/watch-XXX --output both

# 3. If transcript mode also fails, report as bug
```

### Subtitle Detection (Fixed in v4.4.0+)

**Previously**: watch2 could say "no captions" even when `.json3` files existed in the download directory. Root cause was `Path::extension()` returning `"json3"` (no dot) but code comparing with `".json3"` (with dot) — the comparison always failed.

**Current status**: Fixed. `find_video()` and `find_subtitle()` now use correct extension patterns without dot prefix.

**Rust gotcha for future contributors:** `std::path::Path::extension()` returns the extension WITHOUT the dot (`"json3"`, not `".json3"`). Always compare against `"json3"`, never `".json3"`. This bug existed for months because the code "looked correct" — the dot prefix is a natural assumption from other languages (Python's `os.path.splitext` returns with dot).

### Video Not Cleaned Up After Processing

**Symptom**: Downloaded video (potentially GBs) remains on disk after watch2 finishes.

**Check**: `--keep-video` flag was passed? If not, cleanup logic in `main.rs` should auto-delete.

### Vision Analysis is Agent-Driven

**Important**: watch2 outputs frame paths, NOT analyzed images. The agent must call `vision_analyze` on each frame to see the content. Do NOT expect watch2 to return image descriptions.

**Pattern:**
```bash
# Run watch2
watch2 "https://youtu.be/abc" --out-dir /tmp/watch-XXX --output both

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

### Don't Skip Agent-Side Moment Selection

**MISTAKE**: Running watch2 and only analyzing the uniform baseline frames without doing LLM-based moment selection. This misses key moments that need visual verification.

**CORRECT**: After watch2 outputs report.json:
1. Agent reads transcript + scene data
2. Agent uses LLM to select key moments (proper nouns, claims, deictic references)
3. Agent extracts frames at those timestamps
4. Agent vision-analyzes all frames
5. Agent cross-references transcript × scenes × vision

**Why this matters**: Auto-captions (especially non-English) contain errors — misspelled proper nouns, garbled names, incorrect claims. The agent-side moment selection catches these by combining transcript intelligence with visual verification.

**When no subtitles are found**: watch2 will report the issue and suggest setting `GROQ_API_KEY` or `OPENAI_API_KEY` for Whisper fallback. Do NOT fall back to scene detection when captions exist but weren't detected.

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

### av-scenechange Fallback to ffmpeg

When av-scenechange library API fails (VariableFormat, VariableFramerate, unsupported codec), the binary gracefully falls back to ffmpeg scene detection with adaptive thresholds. Warning is printed but no crash.

**Fallback behavior:** No scoring data available in fallback mode (ffmpeg scene detection doesn't provide scores). Frame selection falls back to uniform sampling.

### CJK/Unicode Character Safety

String truncation uses `chars().take(N)` instead of byte slicing (`[..N]`). Multi-byte characters (Korean 3 bytes, Chinese 3 bytes, Emoji 4 bytes) would panic with byte slicing if the cut falls mid-character.

**Rule:** Never use `str[..N]` for truncation on user-provided text. Always use `chars().take(N).collect::<String>()`.

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

**MISTAKE**: Agent uses `execute_code` (Python) to write intermediate JSON files during analysis:
- `vision_results.json` — redundant, findings should be in agent response
- `corrections.json` — redundant, corrections should be applied inline
- `synthesis_prompt.txt` — redundant, synthesis should be generated directly

**Why it's wrong**:
1. `report.json` from the Rust binary already contains ALL structured data (frames, key_moments, stats)
2. Writing intermediate files wastes tokens and creates confusion about source of truth
3. The Rust binary is pure Python-free — using Python to generate files defeats the purpose

**CORRECT workflow**:
```
watch2 (single run) → report.json → agent reads → vision_analyze → cross-reference → summary
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

| Script | Purpose |
|--------|---------|
| `pipeline.rs` | Single-run pipeline orchestrator (no LLM calls) |
| `moments.rs` | Moment detection prompt template + parsing (used by agent) |
| `moment_frames.rs` | Match moments to extracted frames |
| `transcript.rs` | Parse subtitle files (JSON3, VTT) |
| `whisper.rs` | Groq/OpenAI Whisper API client (transcription only) |
| `frames.rs` | Frame extraction engine |
| `scene_detect.rs` | Scene detection via av-scenechange library |
| `output.rs` | Build report (markdown, JSON) |
| `download.rs` | Video download + caching (SHA256 keys, LRU eviction) |
| `config.rs` | Configuration from env + `.env` file |
