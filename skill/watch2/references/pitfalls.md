# Pitfalls

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
This produces equivalent output to watch2's timestamp extraction mode. Always verify frame count ≥21 after extraction.

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

# 2. If files exist, try running binary (will bail if no transcript)
watch2 "URL" --out-dir /tmp/watch-XXX --output both

# 3. If binary also fails, report as bug
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

### Don't Skip Agent-Side Moment Selection (CRITICAL)

**MISTAKE**: Running watch2 and only analyzing the uniform baseline frames without doing LLM-based moment selection. This misses key moments that need visual verification.

**CORRECT workflow:**
```
Step 1: watch2 (single pass — gets transcript + scene data + uniform frames)
Step 2: Agent reads report.json → selects 21-25 key moments via LLM
Step 3: watch2 --timestamps "00:30,01:15,..." --keep-video (extract at moments)
Step 4: vision_analyze all frames → cross-reference → analysis
```

**WHY THIS MATTERS:**

1. **JSON3 word confidence**: Low confidence words (< 0.5) indicate potential ASR errors — these moments NEED visual verification
2. **Scene boundary costs**: High-cost scene changes (> 30) indicate major visual shifts — these moments show new graphics/text/context
3. **Uniform sampling misses key moments**: A 57-minute video with uniform sampling every ~153s will miss most proper nouns, claims, and topic transitions
4. **LLM moment selection catches errors**: Auto-captions (especially non-English) contain misspelled proper nouns, garbled names, incorrect claims

**DATA FLOW (serde serialization):**
```
Binary outputs report.json (serde):
├── transcript[]: JSON3 segments with word-level timing
│   └── words[].confidence: ASR confidence score (0-1)
├── scene_boundaries[]: av-scenechange detection
│   ├── start_sec, end_sec: timing
│   ├── duration_sec: scene length
│   └── inter_cost: scene change cost (>30 = major shift)
└── metadata: title, uploader, duration, language

Agent reads report.json → selects moments → passes timestamps to binary
Binary extracts frames at LLM-selected timestamps → agent vision_analyzes
```

**When no subtitles are found**: watch2 will report the issue and suggest setting `GROQ_API_KEY` or `OPENAI_API_KEY` for Whisper fallback. Do NOT fall back to scene detection when captions exist but weren't detected.

### Finding Top Moments in Transcript (JSON3 + Scene Data)

After extracting the transcript (from watch2 report.json), use JSON3 word-level data + scene boundaries to identify key moments:

**Step 1: Extract transcript with word-level timing**
```bash
# Get JSON3 transcript segments
rtk jq '.transcript[] | {start, end, text, words}' /tmp/watch-XXX/report.json

# Get scene boundaries (av-scenechange data)
rtk jq '.scene_boundaries[] | {start_sec, end_sec, duration_sec, inter_cost}' /tmp/watch-XXX/report.json
```

**Step 2: Identify moments using JSON3 confidence scores**
```bash
# Find low-confidence words (potential ASR errors)
rtk jq '.transcript[].words[] | select(.confidence < 0.5) | {word, start, confidence}' /tmp/watch-XXX/report.json

# Find high-cost scene changes (major visual shifts)
rtk jq '.scene_boundaries[] | select(.inter_cost > 30) | {start_sec, end_sec, inter_cost}' /tmp/watch-XXX/report.json
```

**Step 3: Agent selects 21-25 key moments via LLM**
- Use MOMENT_SELECTION_PROMPT template
- Include JSON3 transcript sample + scene_boundaries sample
- Select moments based on:
  - Low confidence words (potential misspellings)
  - High-cost scene changes (visual transitions)
  - Proper nouns, claims, deictic references
  - Topic transitions (scene changes)

**Step 4: Extract frames at selected timestamps**
```bash
watch2 "URL" --timestamps "00:30,01:15,02:45,..." --keep-video --out-dir /tmp/watch-XXX
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

**Impact**: The pipeline still extracts frames at key timestamps, but `vision_analyze` can only describe the speakers' expressions and setting. It CANNOT verify transcript claims visually (no numbers, text, or graphics to cross-reference).

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

**CORRECT workflow** (two-pass):
```
Binary (pass 1): watch2 "URL" → report.json (transcript + scenes + uniform frames)
Agent: reads report.json → selects 21-25 key moments via LLM
Binary (pass 2): watch2 "URL" --timestamps "..." → extracts frames at key moments
Agent: vision_analyze all frames → cross-reference → summary
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

