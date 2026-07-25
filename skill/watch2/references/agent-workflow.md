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
- Selects key moments using transcript + scene data (scale with duration, see moment-selection.md)
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

Agent selects key moments using a tiered priority system (see moment-selection.md for full criteria).

**Quantity scaling:**
- Short (<5 min): minimum 15 moments
- Medium (5–20 min): minimum 21 moments
- Long (20+ min): scale with density (1 per 30–60s of transcript)

**Selection process:**
1. Fill Tier 1 slots first (hook moments, key arguments, claims/stats)
2. Fill Tier 2 slots (entities, topic transitions, visual context)
3. Fill Tier 3 slots (deictic references, speaker identity)
4. Apply standalone check — reject or expand windows for non-standalone moments
5. Apply anti-pattern filter — reject intros, outros, filler, sponsor reads
6. Score each moment (impact_score 1–10) based on hook, value, verification need, context
7. Spread evenly across FULL duration (beginning, middle, end)

```bash
# Moment Selection Prompt Template
MOMENT_SELECTION_PROMPT = """
You are an expert video analyst selecting key moments for visual verification.

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

YOUR TASK: Select key moments where visual verification would improve analysis accuracy.
Scale with duration — longer videos need more moments for comprehensive coverage.

SELECTION CRITERIA (in priority order):

TIER 1 — HIGH IMPACT (always select):
1. HOOK MOMENTS — surprising claims, bold statements, emotional peaks,
   "did you know" moments. First 2-3 seconds grab attention.
2. KEY ARGUMENTS — conclusions, controversial statements, actionable advice
3. CLAIMS/STATISTICS — numbers, prices, dates, percentages

TIER 2 — MEDIUM IMPACT (select if slots remain):
4. ENTITY RECOGNITION — brand names, product names, on-screen text,
   proper nouns that ASR might misspell
5. TOPIC TRANSITIONS — conversation shifts (correlate with scene_boundaries)
6. VISUAL CONTEXT — moments where understanding visuals changes interpretation

TIER 3 — LOW IMPACT (fill remaining slots):
7. DEICTIC REFERENCES — "this", "that", "look at this" where speaker points
8. SPEAKER IDENTITY — speaker changes, identity unclear from transcript

STANDALONE CHECK (mandatory):
Each moment MUST make sense without surrounding context. REJECT moments that
reference earlier content ("as I mentioned earlier"), start mid-argument
("and therefore"), or are ambiguous without the previous sentence.

ANTI-PATTERNS (reject even if they match other criteria):
- Intros: "hey guys", "welcome to my channel", "what's up everyone"
- Outros: "don't forget to subscribe", "thanks for watching"
- Filler: "so anyway", "basically", "you know"
- Sponsor reads, repetitive statements, low-density tangents

HOOK ASSESSMENT (for each candidate):
Evaluate the FIRST 2-3 SECONDS:
- Surprising statement → +2 impact
- Question → +1 impact
- Number/statistic → +1 impact
- Direct address ("you", "everyone") → +1 impact
- Filler ("so", "um", "well") → −2 impact

SCORING (impact_score 1-10 for each selected moment):
- hook (30%): Does the opening grab attention?
- value (30%): Is the content insightful, surprising, or actionable?
- verification (25%): Does this benefit from visual cross-referencing?
- context (15%): Does this clarify or contradict the transcript?

OUTPUT FORMAT: Return ONLY a JSON array of moments:
[
  {
    "timestamp": 54.0,
    "timestamp_fmt": "0:54",
    "word": "breakthrough",
    "context": "This changes everything. The breakthrough we've been waiting for.",
    "reason": "hook_moment",
    "question": "What product or discovery is shown on screen?",
    "priority": 1,
    "impact_score": 8,
    "standalone": true,
    "hook_type": "shocking_claim"
  }
]

RULES:
- timestamp: f64 seconds (NOT MM:SS string)
- timestamp_fmt: MM:SS string (agent MUST provide — pass to --timestamps)
- reason: one of [hook_moment, key_argument, claim, entity, visual_context,
  topic_transition, deictic, speaker_id]
- hook_type: one of [surprising_statement, question, number, direct_address,
  emotional_peak, none]
- impact_score: 1-10 (see scoring rubric above)
- standalone: true/false (mandatory check)
- priority: 1 (critical) to 5 (nice-to-have)
- Spread moments evenly across FULL duration
- Include moments from beginning, middle, AND end
"""
```

### Step 5: Extract frames at moment timestamps
```bash
# Pass timestamp_fmt values (MM:SS) to --timestamps flag
watch2 "URL" --timestamps "00:30,01:15,02:45,..." --keep-video --out-dir /tmp/watch-XXX
```
- Binary extracts frames ONLY at these timestamps
- Each frame gets `reason: "transcript-cue"` metadata

### Step 6: Vision analyze ALL frames (no exceptions — analyze every extracted frame)
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
The transcript mentions "OpenAI" at 0:54, but the on-screen text shows "Open AI" (two words). This is a common ASR capitalization variance.

**Example 3: Error case (no transcript)**
⚠️ No transcript available for this video. Set GROQ_API_KEY or OPENAI_API_KEY for Whisper transcription.
