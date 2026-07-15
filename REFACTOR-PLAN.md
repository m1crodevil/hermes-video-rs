# hermes-video-rs Refactoring Plan

> **Version:** 5.0.0-refactor  
> **Date:** 2026-07-15  
> **Status:** Draft — awaiting review  
> **Current:** 21 files, 6,076 LOC  
> **Target:** ~25 files, ~6,500 LOC (net +400 for new abstractions)

---

## Executive Summary

Refactor hermes-video-rs from a flat 21-module structure into a layered architecture with clear boundaries. Primary goals:

1. **Split `frames.rs` (736 LOC) → `frames/` submodule** — 6 extraction engines into separate files
2. **Extract `pipeline.rs` from `main.rs`** — reduce main.rs from 593 → ~150 LOC
3. **Add caching layer** — avoid re-downloading same videos
4. **Consolidate vision modules** — merge overlapping logic
5. **Trait-ify Whisper providers** — extensible provider system

---

## Current Architecture

### Dependency Graph

```
                    ┌─────────────┐
                    │  main.rs    │
                    │  (593 LOC)  │
                    └──────┬──────┘
                           │ imports
        ┌──────────────────┼──────────────────┐
        │                  │                  │
        ▼                  ▼                  ▼
   ┌─────────┐      ┌──────────┐      ┌───────────┐
   │  cli    │      │ download │      │  setup    │
   │ (146)   │      │  (476)   │      │  (76)     │
   └─────────┘      └────┬─────┘      └───────────┘
                         │ uses
              ┌──────────┼──────────┐
              ▼          ▼          ▼
         ┌────────┐ ┌────────┐ ┌────────┐
         │ config │ │ error  │ │output  │
         │ (175)  │ │ (27)   │ │ (150)  │
         └────────┘ └────────┘ └───┬────┘
                                   │ defines types
                    ┌──────────────┼──────────────┐
                    │              │              │
                    ▼              ▼              ▼
              ┌──────────┐  ┌──────────┐  ┌──────────┐
              │ frames   │  │transcript│  │ moments  │
              │  (736)   │  │  (144)   │  │  (474)   │
              └──────────┘  └──────────┘  └──────────┘
```

### Core Types (output.rs)

| Type | Used By |
|------|---------|
| `FrameInfo` | frames, dedup, moment_frames |
| `TranscriptSegment` | corrections, synthesis, transcript, vision_batch, moments, whisper |
| `WatchReport` | main |
| `KeyMomentStats` | main |
| `WordTiming` | transcript |

### Module Sizes (LOC)

| Module | LOC | Risk |
|--------|-----|------|
| frames.rs | 736 | 🔴 God module |
| vision.rs | 648 | 🟡 Large but focused |
| main.rs | 593 | 🟡 Too much orchestration |
| vision_batch.rs | 522 | 🟡 Overlaps with vision.rs |
| download.rs | 476 | 🟢 Acceptable |
| moments.rs | 474 | 🟢 Acceptable |
| stats.rs | 457 | 🟢 Acceptable |
| synthesis.rs | 417 | 🟢 Acceptable |
| corrections.rs | 368 | 🟢 Acceptable |
| moment_frames.rs | 266 | 🟢 Acceptable |

---

## Refactoring Plan

### Phase 1: Split `frames.rs` → `frames/` submodule

**Impact:** 🔴 High | **Effort:** 🟡 Medium | **Risk:** 🟡 Medium

**Current:** 736 LOC single file with 6 extraction strategies

**Target:** `frames/` directory with 7 files

```
src/frames/
├── mod.rs          (~80 LOC)  — Re-exports, shared types, auto_fps
├── metadata.rs     (~50 LOC)  — get_metadata(), VideoMetadata
├── keyframe.rs     (~120 LOC) — extract_keyframes() — I-frame extraction
├── scene.rs        (~150 LOC) — extract_scene_or_uniform() — scene detection engine
├── uniform.rs      (~80 LOC)  — extract_uniform() — fixed fps extraction
├── two_pass.rs     (~120 LOC) — extract_two_pass() — scene + uniform merge
├── timestamp.rs    (~100 LOC) — extract_at_timestamps() — cue frame extraction
└── gap_fill.rs     (~60 LOC)  — gap-fill logic (shared by scene + two_pass)
```

**Key changes:**
- `mod.rs` re-exports all public functions (no breaking changes)
- `gap_fill.rs` extracts duplicated gap-fill logic from scene.rs and two_pass.rs
- `scene.rs` (new) replaces the old `scene.rs` which was only 48 LOC (move adaptive_threshold + detect_scene_changes into new scene engine)
- Old `scene.rs` (48 LOC) → merge into `frames/scene.rs`

**Migration steps:**
1. Create `src/frames/` directory
2. Move `VideoMetadata`, `FrameMeta`, `get_metadata()`, `auto_fps()`, `auto_fps_focus()` → `mod.rs`
3. Move `extract_frames()` (internal helper) → `uniform.rs`
4. Move `extract_keyframes()` → `keyframe.rs`
5. Move `extract_scene_or_uniform()` → `scene.rs`
6. Move `extract_two_pass()` → `two_pass.rs`
7. Move `extract_at_timestamps()` → `timestamp.rs`
8. Extract gap-fill logic → `gap_fill.rs`
9. Merge old `scene.rs` (48 LOC) into `frames/scene.rs`
10. Update `lib.rs`: `pub mod frames;` (no change needed — Rust resolves to `frames/mod.rs`)
11. Update all `use crate::frames::*` imports (should be transparent via re-exports)

**Verification:**
- `cargo build` — no errors
- `cargo test` — all existing tests pass
- `watch2 <url> --detail balanced` — same output as before

---

### Phase 2: Extract `pipeline.rs` from `main.rs`

**Impact:** 🟡 Medium | **Effort:** 🟢 Low | **Risk:** 🟢 Low

**Current:** main.rs (593 LOC) handles CLI parsing + all pipeline phases

**Target:** main.rs (~150 LOC) + pipeline.rs (~450 LOC)

```
src/main.rs      (~150 LOC) — CLI parsing, call pipeline
src/pipeline.rs  (~450 LOC) — All pipeline phase logic
```

**What moves to pipeline.rs:**
- `run_transcript_moments_phase1()` — lines 98-161
- `run_transcript_moments_phase2()` — lines 406-491
- `run_whisper_fallback()` — lines 343-382
- `dispatch_frame_engine()` — lines 258-319
- `cleanup_video()` — lines 493-510
- `build_report()` — lines 512-594

**pipeline.rs public API:**
```rust
pub struct PipelineContext {
    pub cli: Cli,
    pub config: WatchConfig,
    pub work_dir: PathBuf,
    pub download_dir: PathBuf,
    pub frames_dir: PathBuf,
}

pub async fn run(ctx: PipelineContext) -> anyhow::Result<WatchReport>;
```

**Migration steps:**
1. Create `src/pipeline.rs`
2. Move orchestration logic from main.rs
3. main.rs becomes: parse CLI → create context → call `pipeline::run(ctx)` → output result
4. Update `lib.rs`: add `pub mod pipeline;`

---

### Phase 3: Add Caching Layer

**Impact:** 🟡 Medium | **Effort:** 🟡 Medium | **Risk:** 🟢 Low

**New module:** `src/cache.rs` (~150 LOC)

```
~/.cache/watch2/
├── videos/
│   ├── <sha256(url)>.mp4
│   └── <sha256(url)>.mp4.info.json
├── subtitles/
│   ├── <sha256(url)>.en-orig.json3
│   └── <sha256(url)>.en.json3
└── metadata/
    └── <sha256(url)>.info.json
```

**cache.rs API:**
```rust
pub struct VideoCache {
    root: PathBuf,  // ~/.cache/watch2/
    max_size_gb: f64,
}

impl VideoCache {
    pub fn new() -> Self;
    pub fn get_video_path(&self, url: &str) -> Option<PathBuf>;
    pub fn store_video(&self, url: &str, path: &Path) -> Result<PathBuf>;
    pub fn get_subtitle_path(&self, url: &str, lang: &str) -> Option<PathBuf>;
    pub fn store_subtitle(&self, url: &str, lang: &str, path: &Path) -> Result<PathBuf>;
    pub fn invalidate(&self, url: &str);
    pub fn cleanup_old(&self, max_age_days: u64);
    pub fn cache_size(&self) -> u64;
}
```

**Integration points:**
- `download.rs`: check cache before downloading
- `main.rs`: pass cache to pipeline
- Add `--no-cache` CLI flag
- Add `--cache-dir` CLI flag

**Cache key:** SHA256 of normalized URL (strip `si` param for YouTube)

**Eviction:** LRU by file mtime, max 10GB default, `cleanup_old()` removes files >30 days

---

### Phase 4: Consolidate Vision Modules

**Impact:** 🟡 Medium | **Effort:** 🟡 Medium | **Risk:** 🟡 Medium

**Current:**
- `vision.rs` (648 LOC) — single-moment analysis
- `vision_batch.rs` (522 LOC) — batch analysis

**Problem:** Both define overlapping types and logic:
- `VerifiedMoment` defined in BOTH (different structs!)
- `extract_corrections` in BOTH
- `apply_corrections_to_transcript` in BOTH
- Frame loading logic duplicated

**Target:** Single `src/vision.rs` (~700 LOC)

```
src/vision.rs (~700 LOC)
├── Types: VisionRequest, VisionResult, VerifiedMoment, VisionFinding, VisionMoment
├── Single: generate_vision_questions(), process_vision_results()
├── Batch: generate_batch_prompt(), process_batch_results()
├── Shared: find_frame_at_timestamp(), list_frames_needed()
└── Corrections: extract_corrections(), apply_corrections_to_transcript()
```

**Key changes:**
- Merge `VisionFinding` (batch) and `VisionResult` (single) into unified type
- Single `VerifiedMoment` type (currently duplicated)
- Single `extract_corrections()` function
- Single `apply_corrections_to_transcript()` function
- Internal dispatch: `analyze_single()` vs `analyze_batch()`

**Migration steps:**
1. Create new `src/vision.rs` with merged types
2. Move unique logic from `vision_batch.rs` into new file
3. Remove `vision_batch.rs`
4. Update `lib.rs`: remove `pub mod vision_batch;`
5. Update imports in main.rs/pipeline.rs

---

### Phase 5: Trait-ify Whisper Providers

**Impact:** 🟢 Low | **Effort:** 🟢 Low | **Risk:** 🟢 Low

**Current:** Hard-coded match on backend string
```rust
match backend {
    "groq" => whisper::transcribe_groq(&audio_path, key).await,
    _ => whisper::transcribe_openai(&audio_path, key).await,
}
```

**Target:** Trait-based provider system
```rust
#[async_trait]
pub trait WhisperProvider: Send + Sync {
    async fn transcribe(&self, audio: &Path) -> Result<Vec<TranscriptSegment>>;
    fn name(&self) -> &str;
}

pub struct GroqProvider { api_key: String }
pub struct OpenAIProvider { api_key: String }

#[async_trait]
impl WhisperProvider for GroqProvider { ... }
#[async_trait]
impl WhisperProvider for OpenAIProvider { ... }

pub fn create_provider(backend: &str, api_key: &str) -> Box<dyn WhisperProvider> {
    match backend {
        "groq" => Box::new(GroqProvider { api_key: api_key.to_string() }),
        _ => Box::new(OpenAIProvider { api_key: api_key.to_string() }),
    }
}
```

**New dependency:** `async-trait` crate (or use native async fn in traits if MSRV allows)

---

### Phase 6: Shared Type Organization

**Impact:** 🟢 Low | **Effort:** 🟢 Low | **Risk:** 🟢 Low

**Current:** `output.rs` is a grab-bag of all shared types

**Target:** Split into `types/` submodule

```
src/types/
├── mod.rs          — Re-exports
├── frames.rs       — FrameInfo, FrameMeta, VideoMetadata
├── transcript.rs   — TranscriptSegment, WordTiming
├── moments.rs      — KeyMoment, KeyMomentStats
├── vision.rs       — VisionRequest, VisionResult, VerifiedMoment, VisionFinding
├── report.rs       — WatchReport
└── error.rs        — WatchError, Result (move from error.rs)
```

**Rationale:** Follows Rust convention of grouping related types. Each file is small (~30-50 LOC). Reduces cognitive load when searching for types.

---

## Target Architecture

```
src/
├── main.rs           (~150 LOC) — CLI entry point
├── lib.rs            (~30 LOC)  — Module declarations
├── pipeline.rs       (~450 LOC) — Pipeline orchestration
├── cli.rs            (~150 LOC) — CLI args (clap)
├── config.rs         (~180 LOC) — Configuration
├── setup.rs          (~80 LOC)  — Preflight checks
├── cache.rs          (~150 LOC) — NEW: Video/subtitle caching
│
├── types/            (~250 LOC) — NEW: Shared types
│   ├── mod.rs
│   ├── frames.rs
│   ├── transcript.rs
│   ├── moments.rs
│   ├── vision.rs
│   └── report.rs
│
├── download.rs       (~480 LOC) — yt-dlp wrapper
├── frames/           (~650 LOC) — Frame extraction (split from 736)
│   ├── mod.rs
│   ├── metadata.rs
│   ├── keyframe.rs
│   ├── scene.rs
│   ├── uniform.rs
│   ├── two_pass.rs
│   ├── timestamp.rs
│   └── gap_fill.rs
│
├── transcript.rs     (~150 LOC) — JSON3/VTT parsing
├── timestamp.rs      (~40 LOC)  — Time parsing
├── moments.rs        (~480 LOC) — Moment detection
├── moment_frames.rs  (~270 LOC) — Moment↔frame linking
│
├── vision.rs         (~700 LOC) — Merged vision analysis
├── corrections.rs    (~370 LOC) — Transcript corrections
├── synthesis.rs      (~420 LOC) — Grounded synthesis
│
├── whisper.rs        (~200 LOC) — Whisper providers (trait-based)
├── stats.rs          (~460 LOC) — Analysis statistics
└── error.rs          (~30 LOC)  — Error types
```

**Summary:**
- Before: 21 files, 6,076 LOC
- After: ~28 files, ~6,500 LOC (+424 for new abstractions)
- main.rs: 593 → 150 LOC (-75%)
- frames.rs: 736 → 8 files averaging 80 LOC each
- vision.rs + vision_batch.rs: 1,170 → 700 LOC (-40%)
- New: cache.rs (150), pipeline.rs (450), types/ (250)

---

## Implementation Order

| Phase | Description | Depends On | Estimated LOC Change |
|-------|-------------|------------|---------------------|
| 1 | Split frames.rs → frames/ | None | +50 (gap_fill extraction) |
| 2 | Extract pipeline.rs | None | 0 (pure refactor) |
| 3 | Add caching layer | Phase 2 | +150 (new module) |
| 4 | Consolidate vision | None | -470 (merge two modules) |
| 5 | Trait-ify Whisper | None | +30 (trait boilerplate) |
| 6 | Shared type organization | None | +100 (reorganization) |

**Parallelizable:** Phases 1, 2, 4, 5, 6 can be done in parallel (no dependencies). Phase 3 depends on Phase 2.

**Recommended order:** 2 → 1 → 4 → 5 → 6 → 3

---

## Risk Assessment

| Phase | Risk | Mitigation |
|-------|------|------------|
| 1 (frames split) | Breaking imports | Re-exports in mod.rs maintain API |
| 2 (pipeline extract) | Logic errors | Move code verbatim, no changes |
| 3 (caching) | Disk space | Max size limit + LRU eviction |
| 4 (vision merge) | Type conflicts | Unified types from ground up |
| 5 (whisper trait) | Async complexity | Use async-trait crate |
| 6 (types org) | Import paths | Re-exports in types/mod.rs |

---

## Testing Strategy

### Unit Tests (existing: 161)
- All existing tests must pass after each phase
- Add tests for new modules:
  - `cache.rs`: cache hit/miss, eviction, cleanup
  - `pipeline.rs`: phase routing, error handling

### Integration Tests
- `watch2 <youtube-url> --detail balanced` — full pipeline
- `watch2 <youtube-url> --detail transcript-moments` — 2-phase workflow
- `watch2 <local-file>` — local file processing
- `watch2 <url> --detail efficient` — keyframe extraction

### Regression Tests
- Compare output before/after refactor for same video
- Frame count should be identical
- Transcript should be identical
- Stats should be identical

---

## Success Criteria

1. ✅ All 161 existing tests pass
2. ✅ `cargo build` clean (no warnings)
3. ✅ main.rs < 200 LOC
4. ✅ No single file > 500 LOC
5. ✅ All modules have clear single responsibility
6. ✅ Caching works (second run faster)
7. ✅ Vision modules consolidated (no duplicate types)
8. ✅ Whisper providers extensible via trait
9. ✅ Binary size < 6MB
10. ✅ Performance regression < 5%

---

## Appendix A: Current vs Target Module Mapping

| Current | Target | Action |
|---------|--------|--------|
| main.rs | main.rs + pipeline.rs | Split |
| frames.rs | frames/mod.rs + 7 files | Split |
| vision.rs | vision.rs (merged) | Merge with vision_batch |
| vision_batch.rs | vision.rs (merged) | Delete |
| output.rs | types/ | Move to submodule |
| error.rs | types/error.rs or keep | Optional move |
| scene.rs | frames/scene.rs | Merge into frames |
| cli.rs | cli.rs | Keep |
| config.rs | config.rs | Keep |
| setup.rs | setup.rs | Keep |
| download.rs | download.rs + cache.rs | Add caching |
| transcript.rs | transcript.rs | Keep |
| timestamp.rs | timestamp.rs | Keep |
| moments.rs | moments.rs | Keep |
| moment_frames.rs | moment_frames.rs | Keep |
| corrections.rs | corrections.rs | Keep |
| synthesis.rs | synthesis.rs | Keep |
| whisper.rs | whisper.rs (trait-based) | Refactor |
| stats.rs | stats.rs | Keep |
| dedup.rs | dedup.rs or frames/dedup.rs | Optional move |

---

## Appendix B: New Dependencies

| Crate | Version | Purpose | Size |
|-------|---------|---------|------|
| `async-trait` | 0.1 | Whisper provider trait | ~0 (proc macro) |
| `sha2` | 0.10 | Cache key generation | ~0 |
| `dirs` | 6 | Cache directory (already used) | 0 |

Net new binary size: ~0 (proc macros + sha2 are tiny)

---

## Appendix C: Migration Checklist

### Phase 1: frames/ split
- [ ] Create `src/frames/` directory
- [ ] Create `src/frames/mod.rs` with re-exports
- [ ] Move `VideoMetadata`, `FrameMeta`, `get_metadata()`, `auto_fps()`, `auto_fps_focus()` → `mod.rs`
- [ ] Move `extract_frames()` → `uniform.rs`
- [ ] Move `extract_keyframes()` → `keyframe.rs`
- [ ] Move `extract_scene_or_uniform()` → `scene.rs`
- [ ] Move `extract_two_pass()` → `two_pass.rs`
- [ ] Move `extract_at_timestamps()` → `timestamp.rs`
- [ ] Extract gap-fill logic → `gap_fill.rs`
- [ ] Merge old `scene.rs` (48 LOC) into `frames/scene.rs`
- [ ] Delete old `src/scene.rs`
- [ ] Update `lib.rs` (should auto-resolve)
- [ ] `cargo build` ✓
- [ ] `cargo test` ✓

### Phase 2: pipeline extraction
- [ ] Create `src/pipeline.rs`
- [ ] Move `run_transcript_moments_phase1()` logic
- [ ] Move `run_transcript_moments_phase2()` logic
- [ ] Move `run_whisper_fallback()` logic
- [ ] Move `dispatch_frame_engine()` logic
- [ ] Move `cleanup_video()` logic
- [ ] Move `build_report()` logic
- [ ] main.rs: parse CLI → create context → `pipeline::run(ctx)` → output
- [ ] Update `lib.rs`: add `pub mod pipeline;`
- [ ] `cargo build` ✓
- [ ] `cargo test` ✓

### Phase 3: caching
- [ ] Create `src/cache.rs`
- [ ] Implement `VideoCache` struct
- [ ] Add `--no-cache` and `--cache-dir` CLI flags
- [ ] Integrate with `download.rs`
- [ ] Add cache cleanup on startup
- [ ] `cargo build` ✓
- [ ] `cargo test` ✓
- [ ] Manual test: run same URL twice, verify second is faster

### Phase 4: vision consolidation
- [ ] Create new `src/vision.rs` with unified types
- [ ] Move unique logic from `vision_batch.rs`
- [ ] Remove `vision_batch.rs`
- [ ] Update `lib.rs`: remove `pub mod vision_batch;`
- [ ] Update all imports
- [ ] `cargo build` ✓
- [ ] `cargo test` ✓

### Phase 5: whisper trait
- [ ] Add `async-trait` to Cargo.toml
- [ ] Define `WhisperProvider` trait
- [ ] Implement for `GroqProvider` and `OpenAIProvider`
- [ ] Add `create_provider()` factory function
- [ ] Update pipeline to use trait
- [ ] `cargo build` ✓
- [ ] `cargo test` ✓

### Phase 6: types organization
- [ ] Create `src/types/` directory
- [ ] Move shared types to appropriate files
- [ ] Update all imports
- [ ] `cargo build` ✓
- [ ] `cargo test` ✓
