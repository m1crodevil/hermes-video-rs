# hermes-video-rs Pipeline Architecture Review

> Review date: 2026-07-15 | Source: deep code review of ~/hermes-video-rs/src/
> **Updated**: 2026-07-25 after v8.0.0 refactor (removed uniform frames, jq-only parsing)
> **v8.0.0 refactor (Jul 2026):** MAJOR breaking change. Removed uniform frame extraction.
> Frames now require agent-selected timestamps via --timestamps flag.
> Report parsing uses jq only — Python is banned.

## Module Map

See current pipeline.rs and src/ directory for up-to-date architecture.

## Data Flow

```
URL → download.rs → video.mp4 + video.*.json3
                        ↓
              transcript.rs → Vec<TranscriptSegment>
                        ↓
              scene_detection.rs → scene_boundaries
                        ↓
              report.json (transcript + scene_boundaries, NO frames)
                        ↓
              Agent reads report.json via jq
              Agent selects key moments (minimum 21, no maximum)
                        ↓
              watch2 --timestamps "00:30,01:15,..."
              frames/timestamp.rs → Vec<FrameInfo>
                        ↓
              Agent vision_analyze all frames
              Agent cross-reference transcript × visuals
                        ↓
              Agent generates comprehensive analysis
```

## Frame Extraction

| Engine | Function | LOC | Trigger |
|--------|----------|-----|---------|
| timestamp | `extract_at_timestamps()` | 100 | --timestamps flag (agent-selected) |
| metadata | `get_metadata()` | 52 | ffprobe wrapper |
| mod.rs | re-exports + helpers | 91 | shared constants, fps calc |

**Removed in v8.0.0:**
- ~~keyframe~~ (extract_keyframes) — replaced by agent-selected timestamps
- ~~scene~~ (extract_scene_or_uniform) — replaced by agent-selected timestamps
- ~~uniform~~ (extract_frames) — removed entirely
- ~~two-pass~~ (extract_two_pass) — replaced by agent-selected timestamps
- ~~gap-fill~~ (fill_gaps_with_uniform) — removed entirely

## Remaining Improvement Areas

### 1. Vision Consolidation (Medium Impact)

vision.rs (648) + vision_batch.rs (522) overlap in:
- Frame loading
- Prompt construction
- Response parsing
- Finding aggregation

Consolidate into single vision.rs with internal dispatch.

### 2. Caching Layer (Medium Impact)

No caching for repeated video analysis. Add:
```
~/.cache/watch2/
├── <sha256(url)>/
│   ├── video.mp4
│   ├── subtitles/
│   └── frames/
```

### 3. Whisper Abstraction (Low Impact)

Current: hardcoded match on backend string.
Target: trait-based providers.
```rust
#[async_trait]
trait WhisperProvider {
    async fn transcribe(&self, audio: &Path) -> Result<Vec<TranscriptSegment>>;
}
```

### 4. Shared Type Organization (Low Impact)

`output.rs` is a grab-bag of shared types. Consider splitting into `types/` submodule for better organization.

## Dependencies

```toml
clap = "4"           # CLI
tokio = "1"          # Async runtime
serde/serde_json = "1" # Serialization
reqwest = "0.12"     # HTTP
async-openai = "0.41" # OpenAI API
groq-api-rust = "0.3" # Groq API
whisper-rs = "0.16"   # Optional local whisper
anyhow/thiserror      # Error handling
tempfile/dirs/which   # Utilities
```

## Score: 8.5/10 (up from 8.2)

| Aspect | Before | After | Notes |
|--------|--------|-------|-------|
| Architecture | 9/10 | **9/10** | Clean two-pass flow |
| Separation of Concerns | 8/10 | **9/10** | Agent handles moment selection |
| Reusability | 7/10 | 7/10 | No caching yet |
| Testability | 9/10 | 9/10 | Pipeline module testable independently |
| Error Handling | 9/10 | 9/10 | anyhow+thiserror solid |
| Performance | 7/10 | **8/10** | No wasted uniform frame extraction |
| Maintainability | 9/10 | **9/10** | Simpler codebase, no uniform logic |
| Extensibility | 8/10 | 8/10 | Add frame engine = new file in frames/ |
