# Agent Workflow

## Mandatory Agent Workflow

The binary runs a **single-pass pipeline**:
1. Download video + subtitles
2. Parse transcript (JSON3/VTT)
3. Whisper fallback (if no captions + API key)
4. Bail if no transcript available
5. Scene detection (metadata in report.json)
6. Build report.json (transcript + scene boundaries, NO frames)
7. Cleanup

**Agent reads report.json via jq, then:**
- Detects language from transcript
- Selects 21-25 key moments using transcript + scene data
- Extracts frames at those timestamps via --timestamps flag (Pass 2)
- Vision analyzes all frames
- Cross-references transcript × visuals
- Generates comprehensive analysis

### Decision Tree

```
Has transcript (JSON3/VTT)?
├── YES → Pass 1: Run binary → Agent reads report.json via jq → selects moments → Pass 2: --timestamps extraction
└── NO  → Whisper fallback (if API key) → If still no transcript → binary exits with error
```

**Note**: The binary REQUIRES a transcript. It cannot analyze video without captions or Whisper. This is by design — transcript-first ensures accurate analysis.

### Pass 1: Get Transcript + Scene Data (NO frames)
```bash
# Pass 1: transcript + scene boundaries only (no frames extracted)
watch2 "URL" --out-dir /tmp/watch-XXX --output both

# Read metadata with jq
jq '{title, uploader, duration, language, engine, scene_count}' /tmp/watch-XXX/report.json

# Get transcript with word-level timing
jq -r '.transcript[] | "[\(.start) → \(.end)] \(.text)"' /tmp/watch-XXX/report.json

# Get scene boundaries
jq '.scene_boundaries[] | {start_sec, end_sec, duration_sec}' /tmp/watch-XXX/report.json
```
- Downloads video + subtitles (with retry, cache)
- Parses transcript (JSON3/VTT)
- Runs scene detection (av-scenechange) → scene_boundaries in report.json
- **NO frames extracted** — agent must select moments first

### Pass 2: Extract Frames at Agent-Selected Timestamps
```bash
# Pass 2: extract frames at agent-selected timestamps
watch2 "URL" --timestamps "00:30,01:15,02:45,..." --keep-video --out-dir /tmp/watch-XXX

# Verify frames extracted
ls /tmp/watch-XXX/frames/*.jpg | wc -l
```
- Binary extracts frames ONLY at these timestamps
- Each frame gets `reason: "transcript-cue"` metadata

### Step 3: LLM Detect Language (ISO 639-1 code)
- Read transcript text
- Identify language (e.g., "en", "id", "ja")

### Step 4: LLM Select Key Moments (using transcript + scene data)

Agent selects 21-25 key moments using this data:

```bash
# Moment Selection Prompt Template
MOMENT_SELECTION_PROMPT = """
You are analyzing a video transcript + scene changes to identify key moments for visual verification.

VIDEO METADATA:
- Title: {title}
- Uploader: {uploader}
- Duration: {duration}s ({duration_fmt})
- Language: {language}
- Scene Changes: {scene_count}

TRANSCRIPT:
{transcript_sample}

SCENE BOUNDARIES:
{scene_boundaries_sample}

YOUR TASK: Select 21-25 key moments where visual verification would improve accuracy.

MOMENT SELECTION CRITERIA:
1. **Proper nouns** — names, brands, titles that might be misspelled in auto-captions
2. **Claims/statistics** — numbers, prices, dates that need fact-checking
3. **Deictic references** — "this", "that", "here", "look at this" where speaker points
4. **Topic transitions** — moments where conversation shifts (use scene_boundaries)
5. **Key arguments** — important conclusions or controversial statements
6. **Visual context** — moments where understanding visuals changes interpretation
7. **Speaker identity** — moments where speaker changes or identity matters (`speaker_id`)
8. **Entity recognition** — brand names, product names, on-screen text (`entity`)

OUTPUT FORMAT: Return ONLY a JSON array of moments:
[
  {
    "timestamp": 54.0,
    "timestamp_fmt": "0:54",
    "word": "Ragnarok",
    "context": "Ya kan Ragnarok. Tahu Ragnarok?",
    "reason": "proper_noun",
    "question": "What game name is displayed on screen?",
    "priority": 1
  }
]

RULES:
timestamp: f64 seconds (NOT MM:SS string)
timestamp_fmt: MM:SS string (agent MUST provide)
- timestamp_fmt → pass to --timestamps flag (MM:SS format required by binary)
- timestamp → internal reference only (do NOT pass to --timestamps)
- reason: one of [proper_noun, claim, deictic, speaker_id, visual_context, entity, topic_transition, key_argument]
- priority: 1 (critical) to 5 (nice-to-have)
- Spread moments evenly across FULL duration
- Include moments from beginning, middle, AND end
- MINIMUM 21 moments required
"""
```

### Step 5: Extract frames at moment timestamps
```bash
# Pass timestamp_fmt values (MM:SS) to --timestamps flag
watch2 "URL" --timestamps "00:30,01:15,02:45,..." --keep-video --out-dir /tmp/watch-XXX
```
- Binary extracts frames ONLY at these timestamps
- Each frame gets `reason: "transcript-cue"` metadata

### Step 6: Vision analyze ALL frames (≥21 minimum, no exceptions)
- Analyze every extracted frame
- Use moment.question for each frame
- Cross-reference with transcript text

### Step 7: Cross-reference transcript × scenes × vision
- Compare transcript claims vs visual evidence
- Identify corrections (ASR errors, visual context)
- Flag unverified claims

### Step 8: Generate comprehensive analysis
- Combine transcript insights + scene context + visual evidence
- Deliver final summary (no process artifacts)

### Output Template

Always use this structure when delivering watch2 results:

```
🎬 **[Video Title]**
Channel: [Uploader] · Duration: [time]

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

### Example Outputs

**Example 1: Simple video summary**
🎬 **How to Build a REST API in 10 Minutes**
Channel: TechWithTim · Duration: 10:23

---
This video walks through building a REST API using Node.js and Express. The host covers route setup, middleware configuration, and error handling in a practical, step-by-step format.
---

**Example 2: Cross-reference finding**
The transcript mentions "Ragnarok" at 0:54, but the on-screen text shows "Ragnarök" (with umlaut). This is a common ASR error for Scandinavian names.

**Example 3: Error case (no transcript)**
⚠️ No transcript available for this video. Set GROQ_API_KEY or OPENAI_API_KEY for Whisper transcription.
