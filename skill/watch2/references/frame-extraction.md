# Frame Extraction

## Frame Extraction

The binary uses one frame engine: `extract_at_timestamps` (src/frames/timestamp.rs).

- **Without --timestamps**: No frames extracted. Only transcript + scene boundaries in report.json.
- **With --timestamps**: Extracts at agent-provided timestamps only. Each frame gets `reason: "transcript-cue"` metadata.

Scene detection (av-scenechange) runs separately and populates `scene_boundaries` in report.json — it does NOT control frame extraction.

## Two-Pass Workflow

```
Pass 1: watch2 URL --out-dir /tmp/watch-XXX --output both
         → Transcript + scene boundaries (NO frames)

Agent reads transcript via jq → selects key moments (minimum 21, no maximum)

Pass 2: watch2 URL --timestamps "01:45,03:30,..." --keep-video --out-dir /tmp/watch-XXX
         → Frames extracted at agent-selected timestamps only
```

## Why No Uniform Frames?

Uniform sampling wastes resources and misses key moments:
- A 38-minute video with 21 uniform frames = 1 frame every ~108 seconds
- Speaker transitions, topic changes, and visual demonstrations happen at irregular intervals
- Agent-selected moments are targeted at transcript cues + scene boundaries

## Frame Count Requirement

Minimum 21 frames (MANDATORY). If agent selects fewer than 21 moments, pad with additional timestamps from scene boundaries.
