# Transcript-Moments Pipeline Architecture

> **v5.0.0 refactor (Jul 2026):** Pipeline simplified to single linear flow.
> Transcript-moments is now the DEFAULT behavior (no mode flag needed).
> Agent workflow: first run generates moments_prompt.txt, re-run extracts frames.

## Overview

The transcript-moments pipeline is now the **default and only** workflow.

```
User: watch2 <url> --detail transcript-moments --out-dir /tmp/watch-TM

Phase 1 (First Run):
  ├── fetch_captions() → transcript_segments
  ├── format_transcript_for_analysis() → "[MM:SS] text" format
  ├── generate_prompt() → LLM prompt with transcript + metadata
  ├── Writes moments_prompt.txt
  └── Exits (no video download needed)

Agent: reads moments_prompt.txt → identifies 50+ key moments → writes key_moments.json

Phase 2 (Re-run):
  ├── fetch_captions() → transcript_segments (same)
  ├── Detects key_moments.json exists → has_moments=True
  ├── download() → video_path
  ├── Reads key_moments.json, parses timestamps
  ├── extract_at_timestamps(video_path, timestamps) → frames (UNCAPPED)
  ├── update_moments_with_frames() → link moments to frames
  ├── Calculate KeyMomentStats (by_reason, by_priority)
  └── build_report() → WatchReport with key_moments data

Phase 3 (Agent-driven):
  ├── Read report.json for structured data
  ├── For each representative frame (21+ spread across 50+):
  │   └── vision_analyze(frame_path, moment.question)
  ├── Collect findings in memory (NOT as intermediate JSON files)
  ├── Classify each finding: confirmed, corrected, fabrication, unverified, partial
  └── Cross-reference gate: transcript × vision × scene

Phase 4 (Agent-driven):
  ├── Generate corrections inline from classified findings
  ├── Apply corrections to transcript mentally
  └── Produce grounded summary with cross-reference table
```

## Key Files

| File | Role | Phase |
|------|------|-------|
| `moments.rs` | Generate LLM prompt, parse response | 1 |
| `moment_frames.rs` | Match moments to extracted frames | 2 |
| `vision.rs` | Single-moment vision pipeline | 3 |
| `vision_batch.rs` | Batch vision (multiple frames per prompt) | 3 |
| `corrections.rs` | Apply corrections to transcript | 4 |
| `synthesis.rs` | Generate grounded synthesis prompt | 4 |

## Data Flow

```
moments_prompt.txt (Phase 1 output)
    ↓ Agent reads and processes
key_moments.json (Agent-written, Phase 2 input)
    ↓ Tool reads and extracts frames
frame_*.jpg files (Phase 2 output)
    ↓ Agent vision_analyze on each
report.json (Tool output — structured data source of truth)
    ↓ Agent cross-references transcript × vision
Grounded summary with corrections (final output)
```

**IMPORTANT**: Phase 3+4 are agent-driven. The Rust binary outputs `report.json` with all structured data. Agent reads `report.json` directly — no intermediate JSON files needed.

## key_moments.json Schema

```json
[
  {
    "timestamp": 54.0,             // f64 seconds (NOT MM:SS string!)
    "timestamp_fmt": "0:54",       // MM:SS string (agent MUST provide)
    "word": "breakthrough",        // Triggering word/phrase
    "context": "This changes everything. The breakthrough we've been waiting for.",
    "reason": "hook_moment",       // Detection category (see below)
    "question": "What product or discovery is shown on screen?",
    "priority": 1,                 // 1=critical, 5=nice-to-have
    "impact_score": 8,             // 1-10 impact rating (hook + value + verification + context)
    "standalone": true,            // Does this moment work without surrounding context?
    "hook_type": "surprising_statement",  // Hook classification
    "frame_path": null             // Always null (binary fills after extraction)
  }
]
```

**CRITICAL**: `timestamp` must be `f64` seconds, not a string. Convert: `MM:SS` → `M*60 + SS`. Missing `timestamp_fmt` or `frame_path` causes Rust binary parse error.

## Detection Categories

1. **hook_moment** — surprising claims, bold statements, emotional peaks
2. **key_argument** — important conclusions, controversial statements
3. **claim** — numbers, prices, dates, percentages
4. **entity** — brand names, product names, on-screen text, proper nouns
5. **topic_transition** — conversation shifts (correlate with scene_boundaries)
6. **visual_context** — understanding visuals changes interpretation
7. **deictic** — "this", "that", "look at this" where speaker points
8. **speaker_id** — unclear who is speaking

## Anti-Hallucination Rules

1. Cite timestamps — every claim must reference `[MM:SS]`
2. Zero fabrication — if you can't read text, say "unreadable"
3. Distinguish SEE vs INFER
4. Flag uncertainty — "appears to be" vs "is"
5. Cross-reference transcript against visual evidence
6. No assumptions — don't fill gaps with plausible guesses
7. Report contradictions — transcript says X but frame shows Y
8. Source every correction — "Frame at 2:15 shows 'OpenAI' not 'Open Ai'"

## Common Pitfalls

### Forgetting --out-dir
Each run creates a new temp directory. Without `--out-dir`, Phase 1's `key_moments.json` is lost in Phase 2. ALWAYS use `--out-dir`.

### Analyzing Too Few Frames
The pipeline extracts 50+ frames but you should analyze 21+ representative ones. Don't analyze all 50+ (expensive) or only 3 (misses context).

### Using efficient/balanced for Captioned Videos
If the video has captions, ALWAYS use `--detail transcript-moments`. Basic frame extraction gives random keyframes with no targeted questions.
