# Script Reference

## Script Reference

| Script | Purpose |
|--------|---------|
| `pipeline.rs` | Single-run pipeline orchestrator (no LLM calls) |
| `moments.rs` | Moment detection prompt template + parsing (used by agent) |
| `moment_frames.rs` | Match moments to extracted frames |
| `transcript.rs` | Parse subtitle files (JSON3, VTT) |
| `whisper.rs` | Groq/OpenAI Whisper API client (transcription only) |
| `frames.rs` | Frame extraction engine |
| `scene_detect.rs` | Scene detection via av-scenechange library |
| `output.rs` | Build report (markdown, JSON) |
| `download.rs` | Video download + caching (SHA256 keys, LRU eviction) |
| `config.rs` | Configuration from env + `.env` file |
