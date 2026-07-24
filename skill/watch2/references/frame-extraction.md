# Frame Extraction

## Frame Extraction

The binary uses one frame engine: `extract_at_timestamps` (src/frames/timestamp.rs).

- **Default (no --timestamps)**: Generates 21 uniform timestamps across video duration
- **With --timestamps**: Extracts at agent-provided timestamps only
- Each frame gets `reason: "transcript-cue"` metadata

Scene detection (av-scenechange) runs separately and populates `scene_boundaries` in report.json — it does NOT control frame extraction.

