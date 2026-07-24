# Workflow Details

## Workflow

### Single-Pass Pipeline

The binary runs a **single pass** — download, parse, extract, report. All in one shot.

```
Single Run (default — uniform frames):
├── watch2 "URL" --out-dir /tmp/watch-XXX --output both
├── Downloads video + subtitles (with retry, cache)
├── Parses transcript (JSON3/VTT/Whisper)
├── Scene detection (av-scenechange) → scene_boundaries
├── Extracts 21 uniform frames
├── Builds report.json (transcript + scene_boundaries + frames)
└── Cleans up video

Single Run (agent-selected timestamps):
├── watch2 "URL" --timestamps "00:30,01:15,..." --keep-video --out-dir /tmp/watch-XXX
├── Same as above, but extracts at provided timestamps only
└── Each frame gets reason: "transcript-cue"
```

### Agent-Side Intelligence

No LLM calls from binary. All intelligence is done by the agent:

```
Agent reads report.json:
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
4. Proceed with vision analysis on extracted frames

### Fallback: No Captions

When no captions are available AND no Whisper API key is set, the binary bails. Options:
1. Set `GROQ_API_KEY` or `OPENAI_API_KEY` in `~/.config/watch/.env` for Whisper fallback
2. Use `yt-dlp` to download video, then `ffmpeg` for manual frame extraction
3. Skip the video (no transcript = no analysis)

**Note**: The binary does NOT fall back to scene-detection frame extraction. It requires a transcript.

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

