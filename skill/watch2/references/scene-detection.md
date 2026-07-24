# Scene Detection

## Scene Detection

**av-scenechange** (Rust library API) detects scene boundaries for report.json metadata.
- Uses `SceneDetectionSpeed::Fast` (pixel-wise comparison)
- Returns `SceneBoundary` structs with scoring data (inter_cost, imp_block_cost, etc.)
- Scene boundaries help agent identify topic transitions for moment selection
- Fallback: ffmpeg scene detection (stub — currently disabled)

**Note**: Scene detection does NOT control frame extraction. Frames are only extracted when agent provides timestamps via --timestamps flag (Pass 2).
