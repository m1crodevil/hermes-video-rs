# Workflow Details

## Workflow

### Two-Pass Pipeline

The binary uses a **two-pass workflow** — Pass 1 gets transcript + scene data, Pass 2 extracts frames at agent-selected timestamps.

```
Pass 1: Get transcript + scene data (NO frames)
├── watch2 "URL" --out-dir /tmp/watch-XXX --output both
├── Downloads video + subtitles (with retry, cache)
├── Parses transcript (JSON3/VTT/Whisper)
├── Scene detection (av-scenechange) → scene_boundaries
├── Builds report.json (transcript + scene_boundaries, NO frames)
└── Cleans up video

Agent reads report.json via jq:
├── Transcript (JSON3 with word-level timing + confidence)
├── Scene boundaries (av-scenechange data)
├── Metadata (title, uploader, duration, language)
└── Selects key moments (scale with duration, see moment-selection.md)

Pass 2: Extract frames at agent-selected timestamps
├── watch2 "URL" --timestamps "00:30,01:15,..." --keep-video --out-dir /tmp/watch-XXX
├── Same as Pass 1, but extracts at provided timestamps only
└── Each frame gets reason: "transcript-cue"
```

### Agent-Side Intelligence

No LLM calls from binary. All intelligence is done by the agent:

```
Agent reads report.json via jq:
├── Transcript (JSON3 with word-level timing + confidence)
├── Scene boundaries (av-scenechange data)
├── Frame list (paths + timestamps)
└── Metadata (title, uploader, duration, language)

Agent selects key moments via LLM:
├── Uses transcript context (proper nouns, claims, deictic refs)
├── Uses scene boundaries (topic transitions, visual shifts)
└── Outputs timestamps as comma-separated string

Agent extracts frames at selected timestamps:
└── watch2 --timestamps "00:30,01:15,..." --keep-video --out-dir /tmp/watch-XXX
```

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
4. Read report.json with jq
5. Select key moments
6. Run Pass 2 with --timestamps

### Fallback: No Captions

When no captions are available AND no Whisper API key is set, the binary bails. Options:
1. Set `GROQ_API_KEY` or `OPENAI_API_KEY` in `~/.config/watch/.env` for Whisper fallback
2. Use `yt-dlp` to download video, then `ffmpeg` for manual frame extraction
3. Skip the video (no transcript = no analysis)

**Note**: The binary does NOT fall back to scene-detection frame extraction. It requires a transcript.

### Frame Count Verification Gate (MANDATORY)

**After ANY frame extraction method (watch2 --timestamps OR manual ffmpeg), BEFORE proceeding to vision analysis:**

1. Count extracted frames: `ls <workdir>/frames/*.jpg | wc -l`
2. **If count < 15**: STOP. Do NOT proceed with vision analysis on fewer than 15 frames.
3. Fix the extraction first:
   - If watch2 failed → use manual ffmpeg with calculated fps (duration ÷ target_frames)
   - If agent selected too few moments → add more timestamps from scene boundaries
   - If video is short (<3 min) → extract at every 5 seconds
4. Re-count. Only proceed when ≥15 frames confirmed (scale with duration — see moment-selection.md).

**Why 15 minimum**: Fewer frames = blind spots in visual analysis. A 5-minute video needs at least one frame every 20 seconds to catch all visual context. Skipping this produces shallow, unreliable analysis.
