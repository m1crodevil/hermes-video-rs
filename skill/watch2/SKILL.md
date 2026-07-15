---
name: watch2
version: "4.3.0"
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

Rust-powered video analysis. Faster startup (~5ms), smaller memory (~5-15MB), single binary (5.2MB).

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
├── YES → Use --detail transcript-moments (deep analysis) ← ALWAYS FIRST
├── NO but has audio → Use --detail balanced (scene detection)
└── NO, no audio → Use --detail transcript (text only)
```

## When to Use Each Mode

| Mode | When | Speed | Accuracy |
|------|------|-------|----------|
| **transcript-moments** | ✅ Video with captions (YouTube auto/manual) | ~15s phase1 + frame extraction | ⭐⭐⭐ Highest |
| **balanced** | Video without captions, need visual coverage | ~300s for 58min | ⭐⭐ Good |
| **efficient** | Quick overview, hard cuts, short videos | ~10-20s | ⭐ Basic |
| **transcript** | Dialogue-heavy, no visual needed | ~5s | ⭐⭐ (audio only) |
| **token-burner** | Max fidelity, short videos | ~500s+ | ⭐⭐⭐ Highest |
| **screenshot-first** | Long videos with captions, speed priority | ~35s | ⭐⭐ Good |

**Rule of thumb:** For videos >10 min with captions, ALWAYS use `transcript-moments`. It's the most accurate path — auto-captions (especially non-English) contain errors that only visual verification catches.

## Workflow

### Transcript-First Mode (recommended for videos with captions)

When captions are available, this is the **fastest and most accurate** approach:

- [ ] Step 1: Run `watch2 "<source>" --detail transcript-moments --min-moments 50 --out-dir <FIXED_DIR>`
  - First run: generates `moments_prompt.txt` (no video download, ~15s)
  - **CRITICAL: Use `--out-dir` to pin the working directory.** Without it, each run creates a new `/tmp/watch-XXXX` and `key_moments.json` from run 1 is lost on run 2.
- [ ] Step 2: Read `<workdir>/moments_prompt.txt`, analyze transcript, identify 50+ key moments
- [ ] Step 3: Write moments as JSON to `<workdir>/key_moments.json`
- [ ] Step 4: Re-run `watch2` with same args including `--out-dir <FIXED_DIR>` (video downloads + frames extracted at all moment timestamps)
- [ ] Step 5: `vision_analyze` 21+ representative frames (from the 50+ extracted) with specific questions from `key_moments.json`
- [ ] Step 6: Apply corrections to transcript, generate grounded summary

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
| `--min-moments N` | Minimum moments to detect | 50 |
| `--stats` | Show analysis statistics | false |
| `--stats-format` | Stats format: telegram or compact | telegram |
| `--whisper groq\|openai` | Force Whisper backend | auto |
| `--no-whisper` | Disable Whisper fallback | false |
| `--no-dedup` | Keep duplicate frames | false |
| `--out-dir DIR` | Working directory | tmp |
| `--no-cache` | Disable download cache | false |
| `--cache-dir DIR` | Custom cache directory | ~/.cache/watch2/ |

## Detail Modes

- `transcript` — No frames, transcript only (fastest)
- `transcript-moments` — LLM-driven moment detection + frame extraction (deep analysis)
- `efficient` — Keyframes only (I-frames via `-skip_frame nokey`), cap 50
- `balanced` — Scene-aware (detect cuts → extract per-scene), cap 100 (recommended)
- `token-burner` — Two-pass: scene detection + uniform gap-filling (max fidelity, uncapped)

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
GROQ_API_KEY=gsk_...
OPENAI_API_KEY=sk-...
WATCH_DETAIL=balanced
SETUP_COMPLETE=true
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

## Script Reference

| Script | Purpose | When to Use |
|--------|---------|-------------|
| `moments.rs` | Generate LLM prompt for moment detection | Phase 1 |
| `moment_frames.rs` | Match moments to extracted frames | Phase 2 |
| `vision.rs` | Vision analysis (single + batch, merged) | Phase 3 |
| `corrections.rs` | Apply corrections to transcript | Phase 4 |
| `synthesis.rs` | Generate grounded synthesis prompt | Phase 4 |
| `cache.rs` | Download caching (SHA256 keys, LRU eviction) | All runs |

## Pitfalls

### Video Downloads at Full Quality (No 720p Cap)

**Symptom**: watch2 downloads a 3GB+ video file for a 57-minute YouTube video, timing out or filling disk.

**Cause**: The Rust `download_video()` was missing the `-f` format flag that Python has. Without it, yt-dlp downloads best quality (4K = 3GB for long videos).

**Fix** (v4.2.1+): `download.rs` now passes `-f bv*[height<=720]+ba/b[height<=720]/bv+ba/b` and `--merge-output-format mp4`, matching the Python version.

**Always verify**: After updating Rust version, cross-check `download.rs` against Python `download.py` for format string parity. See `references/python-vs-rust-differences.md`.

### Video Not Cleaned Up After Processing

**Symptom**: Downloaded video (potentially GBs) remains on disk after watch2 finishes.

**Cause**: Two possibilities:
1. `--keep-video` flag was passed (intentional)
2. Cleanup logic in `main.rs` didn't trigger

**Expected behavior**: Both Python and Rust versions auto-delete the downloaded video after processing unless `--keep-video` is passed. The video file is only needed for frame extraction — once frames are extracted, the video is waste.

**Verification**: Check `main.rs` lines 494-500 for the cleanup block:
```rust
if !cli.keep_video {
    if let Some(ref vp) = video_path {
        if dl_result.downloaded {
            std::fs::remove_file(vp).ok();
        }
    }
}
```

### Duration Detection Fails (Any Format)

**Symptom**: watch2 reports `"Video has zero or negative duration (0.00s)"` and produces an empty report (`"No frames or transcript available"`), even though:
- The video downloaded successfully (check `download/video.mp4` or `download/video.webm`)
- Subtitles exist (check `download/video.*.json3`)
- **Frames were NOT extracted** — the `frames/` directory won't exist when duration=0

**Cause**: Rust's `ffprobe` duration parsing sometimes fails for certain containers from YouTube. Originally documented for webm/AV1, but has also hit mp4 files (e.g., 339MB mp4 from a 2-hour Diary of a CEO episode). When duration=0, watch2 skips frame extraction entirely.

**Workaround** — manual frame extraction + transcript parsing:
```bash
OUTDIR="/tmp/watch-XXX"  # Use the --out-dir you passed to watch2

# 1. Verify download and subtitles exist
ls -la "$OUTDIR/download/video.mp4" "$OUTDIR/download/"*.json3

# 2. Get real duration via ffprobe
ffprobe -v quiet -show_entries format=duration \
  -of default=noprint_wrappers=1:nokey=1 "$OUTDIR/download/video.mp4"

# 3. Extract frames with scene detection (matches watch2 balanced mode)
mkdir -p "$OUTDIR/frames"
ffmpeg -i "$OUTDIR/download/video.mp4" \
  -vf "select='gt(scene,0.25)',scale=512:-1" \
  -vsync vfr -q:v 3 \
  "$OUTDIR/frames/frame_%04d.jpg" -y

# 4. Extract transcript from JSON3
#    JSON3 naming: video.en-orig.json3 (manual subs) or video.en.json3 (auto)
python3 << 'PYEOF'
import json, glob
files = glob.glob("/tmp/watch-XXX/download/video.*.json3")
json3 = next((f for f in files if "en-orig" in f), files[0] if files else None)
if not json3:
    raise SystemExit("No JSON3 subtitle files found")
print(f"Using: {json3}")
with open(json3) as f:
    data = json.load(f)
lines = []
for event in data.get("events", []):
    if "segs" in event:
        text = "".join(s.get("utf8","") for s in event["segs"] if "utf8" in s).strip()
        if text:
            t = event["tStartMs"] // 1000
            lines.append(f"[{t//60:02d}:{t%60:02d}] {text}")
with open("/tmp/watch-XXX/transcript.txt", "w") as f:
    f.write("\n".join(lines))
print(f"Transcript: {len(lines)} lines, ~{lines[-1][:5] if lines else 'N/A'}")
PYEOF

# 5. Analyze frames — sample strategically (see below)
```

**Frame sampling strategy for long videos (60+ min)**:
- Don't analyze all frames (hundreds of API calls = expensive + slow)
- Sample ~21 frames evenly across the duration
- Formula: `frame_index = int((i / 21) * total_frames)` for i in 0..20
- This gives roughly one frame every 3-6 minutes for a 2-hour video
- Use `execute_code` to compute indices and print frame paths, then batch `vision_analyze` calls (4 at a time)

### Vision Analysis is Agent-Driven

**Important**: watch2 outputs frame paths, NOT analyzed images. The agent must call `vision_analyze` on each frame to see the content. Do NOT expect watch2 to return image descriptions.

**Efficient pattern**:
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

**ROOT CAUSE**: Agent sees "Quick Start" or default `balanced` and skips the decision tree. The Quick Start now puts transcript-moments FIRST for captioned videos.

**CORRECT**: Follow the [Workflow](#workflow) section — Transcript-First Mode. If the video has captions (check for `.json3` or `.vtt` files in the download directory), ALWAYS use `--detail transcript-moments` for deep analysis. The transcript-moments pipeline:

1. Generates an LLM prompt identifying 50+ moments needing visual verification
2. Extracts frames at ALL those timestamps
3. Enables you to vision_analyze 21+ representative frames with specific questions

**Why this matters**: Auto-captions (especially non-English) contain errors — misspelled proper nouns, garbled names, incorrect claims. The transcript-moments pipeline catches these by combining transcript intelligence with visual verification. Basic frame extraction (efficient/balanced) only gives you random keyframes with no targeted questions.

### Finding Top Moments in Transcript

After extracting the transcript (either from watch2 or manual JSON3 parsing), use `search_files` with regex to find the most dramatic/impactful moments.

**Step 1 — Build a keyword pattern** combining high-impact phrases:
```bash
search_files \
  --pattern "70% chance|extinction|dictator|scary|chilling|lost.*million|resigned|afraid|most important|entire economy|superintelligence|recursive self|immortality|pause|ban|shutdown|warn" \
  --path /tmp/watch-XXX/transcript.txt \
  --output-mode content \
  --limit 60
```

**Step 2 — Build a second pass** for structural moments (transitions, reveals, conclusions):
```bash
search_files \
  --pattern "species|ruling the world|humanity|collapse|end of|weapon|nuclear|war|automate.*everything|space|ocean|longevity|choose.*die|Plan A|regulate|equitable" \
  --path /tmp/watch-XXX/transcript.txt \
  --output-mode content \
  --limit 30
```

**Step 3 — Cross-reference with frame timestamps** to confirm visual context:
```
Frame 0001 → ~00:00  (cold open)
Frame 0041 → ~05:34  (early context)
Frame 0867 → ~120:41 (ending)
```

**Step 4 — Compile top 10-15 moments** as a table with: `# | Timestamp | Topic | Quote`

This technique works for any long-form video (podcasts, interviews, lectures) where you need to quickly identify the most impactful segments without watching the full video.

### Pipeline Architecture Review (hermes-video-rs)

For project maintainers reviewing the Rust pipeline architecture (post-refactoring v5.0):

**Current component count**: 28 files, ~6,830 LOC total

**Key modules by size** (code-only, excluding tests):
| Module | LOC | Role |
|--------|-----|------|
| pipeline.rs | 785 | Pipeline orchestrator (extracted from main.rs) |
| vision.rs | 588 | Vision analysis (merged single + batch) |
| download.rs | 476 | yt-dlp wrapper |
| moments.rs | 474 | Moment detection + prompt gen |
| cache.rs | 463 | Download caching (SHA256, LRU) |
| stats.rs | 457 | Analysis statistics |
| synthesis.rs | 417 | Grounded synthesis prompt |
| corrections.rs | 368 | Transcript corrections |

**Refactoring completed** (v5.0):
1. ✅ `frames.rs` (736 LOC) → split into `frames/` (8 files, avg 106 LOC)
2. ✅ `main.rs` (593 LOC) → extracted to `pipeline.rs`, main.rs now 114 LOC
3. ✅ `vision.rs` + `vision_batch.rs` (1,170 LOC) → merged into single `vision.rs` (588 LOC)
4. ✅ Whisper providers trait-ified (`WhisperProvider` trait + `GroqProvider`/`OpenAIProvider`)
5. ✅ Caching layer added (`cache.rs` with SHA256 keys, LRU eviction, 10GB max)
6. ⏭️ Shared type organization — skipped (output.rs at 150 LOC, not worth churn)

**Current score**: 8.5/10 overall. Clean architecture, modular, extensible.
