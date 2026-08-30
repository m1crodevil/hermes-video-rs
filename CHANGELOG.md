# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [8.2.0] - 2026-08-30

### Breaking changes
- `WatchReport` now contains extraction evidence only; removed unused key-moment, scene-score, engine, detail, and dropped-frame fields.
- Removed the inactive `--no-dedup` CLI flag, `fetch_captions()` API, language cache API, and local Whisper feature.

### Changed
- `report.json` is now written for every output format and refreshed on both workflow passes.
- Simplified scene output to boundaries and removed unused score-file generation.
- Replaced the Whisper provider trait/factory with a fixed backend enum; removed `async-trait` and `whisper-rs`.
- Rewrote README and skill documentation around the agent-owned two-pass evidence workflow.

## [8.1.0] - 2026-08-02

### Bug Fixes
- Resolve all 26 clippy warnings — 0 warnings now
- *(ci)* Relax clippy to warn-only, remove strict lint config
- *(ci)* Remove security.yml — was never deleted, only copied
- *(skill)* Remove 21-25 moment cap — minimum 21, no maximum
- Make two-pass workflow explicit in Quick Start
- Add explicit frame analysis rule to SKILL.md core


### Chore
- Bump version to 8.1.0, add --version flag and fix CLI name
- Remove CI workflow (billing locked — red X gone)
- Add CI status note
- *(ci)* Disable security audit workflow
- Cleanup orphaned files and unused modules


### Documentation
- Update README for modular SKILL.md architecture
- Efficiency refactor SKILL.md — progressive disclosure + examples
- Fix 5 output prompt/template issues in SKILL.md
- Fix SKILL.md to match binary v7.2.0 reality
- Refactor README for SEO + agent ecosystem visibility


### Features
- [**breaking**] V8.0.0 — remove uniform frame extraction, agent-driven timestamps only
- *(skill)* V8.0.0 — remove uniform frames, jq-only parsing


### Other
- Add CI/CD and tooling configuration
- Add 15 edge case tests (248 → 263)
- Add 37 mocked integration tests (211 → 248)
- Add 35 filesystem-based unit tests (176 → 211)
- Add 53 pure logic unit tests (123 → 176)
- Remove dead code + cargo fmt


### Refactor
- Split SKILL.md into core + references (progressive disclosure)

## [7.2.0] - 2026-07-24

### Bug Fixes
- *(pipeline)* Increase frame count cap from 20 to 21
- *(pipeline)* Move video cleanup after report building
- *(skill)* Remove 12 fake CLI flags, align SKILL.md with actual binary
- Download ALL subtitle languages to avoid YouTube 429 rate-limit (v4.5.0)
- Subtitle language code normalization (en-US → en)
- CJK character safety in string truncation (Korean panic)
- Av-scenechange graceful fallback to ffmpeg on decoder failure
- Remove Python dependency from curl_cffi detection
- Duration fallback to metadata when ffprobe fails
- Make av-scenechange mandatory, fix runtime args, add to setup checks
- Enforce 21+ frame minimum in fallback pipeline
- Path::extension() returns without dot — find_video/find_subtitle compared with dot prefix
- Eliminate Python fallback from watch2 SKILL.md
- Subtitle detection — LLM language detection + deterministic file selection
- Cap video download at 720p to prevent huge files


### CI/CD
- Add cargo-audit workflow for dependency vulnerability scanning


### Chore
- Bump Cargo.toml version to 7.2.0
- Remove Bradley Bonanno from LICENSE copyright


### Documentation
- Update README to reflect v7.x agent-side architecture
- Update README for single-run pipeline
- Rewrite README for v5.0.0 simplified pipeline
- SKILL.md — note Fast mode (3x speed) for av-scenechange
- SKILL.md v4.9.0 — CJK safety + av-scenechange fallback
- SKILL.md v4.8.0 — av-scenechange library API update
- SKILL.md v4.6.0 — scene-aware Phase 1 workflow
- Move anti-Python rule to Output Philosophy (primacy effect)
- Fix README contradictions and typos
- SKILL.md v4.5.0 — restructure for agent compliance
- Full README refactor — headline, pipeline docs, accuracy fixes
- Add key_moments.json schema + update Rust-Only Rule
- Add verification guardrail to Step 1 — never shortcut when captions exist
- Update SKILL.md — subtitle detection section reflects Bug #5 fix
- Add Bug #5 to subtitle-detection-bugs.md — Path::extension() dot prefix mismatch
- Refactor README to match actual codebase


### Features
- *(watch2)* Transcript-first workflow with JSON3 + scene data for moment selection
- Add --timestamps flag for agent-side moment frame extraction
- Single-run pipeline — no more Phase 1/2 re-runs
- Materialize-first pipeline architecture
- Wire scene scores into pipeline (score_based_select, fused prompt, WatchReport)
- Av-scenechange library API integration with scene scoring
- Scene-aware moment detection in Phase 1
- Phase 6 — reduce yt-dlp network overhead
- Cache optimization + --no-scene-detection flag
- Optimize scene detection — skip redundant runs, use --speed 1
- Optimize pipeline — merge yt-dlp passes, skip redundant metadata, skip LLM when possible
- *(pipeline)* Integrate fusion into main pipeline flow
- *(moments)* Add fused prompt with scene context and ASR confidence
- *(fusion)* Add scene-transcript fusion engine
- *(scene_detect)* Add av-scenechange wrapper with ffmpeg fallback
- Better fall-through messaging — detect stale .json3 files and warn user
- SKILL.md v4.3.0 — workflow-first with transcript-moments as primary path


### Other
- Sanitize error messages to prevent information disclosure
- Validate max-frames and resolution to prevent resource exhaustion
- Use RAII cleanup for temp directory by default
- Remove CWD .env loading to prevent config injection


### Performance
- *(download)* Detect language before download, target subtitles
- Switch av-scenechange to Fast mode (3x speed improvement)


### Refactor
- Remove LLM calls from binary, agent handles intelligence
- Simplify pipeline to single linear flow
- Major architecture overhaul (Phases 1-5 + caching)


### Revert
- Phase 1 back to transcript-only (fix performance regression)


### Testing
- Add unit tests for find_video, find_subtitle, clean_stale_subtitles, subtitle_lang_pattern

## [4.2.0] - 2026-07-15

### Documentation
- Rewrite README to match Python hermes-video structure


### Features
- Add transcript-moments detail mode (v4.2.0)

## [4.1.0] - 2026-07-15

### Features
- *(v4.1.0)* Polish & security hardening

## [4.0.0] - 2026-07-15

### Features
- *(v4.0.0)* LLM features — full pipeline

## [3.2.0] - 2026-07-15

### Features
- *(v3.2.0)* CLI & robustness improvements

## [3.1.0] - 2026-07-15

### Features
- *(v3.1.0)* Phase 1 — pipeline quick wins

## [3.0.0] - 2026-07-12

### Bug Fixes
- Improved error handling and edge cases


### Documentation
- Add README with install, usage, architecture


### Features
- Add word-level timing to JSON3 parser
- V3.0.0 — full feature parity with Python hermes-video
- Integration tests
- SKILL.md for Hermes integration
- Complete pipeline — all modules wired together
- Transcript parser
- Setup module
- Frame deduplication
- Whisper module
- Output module
- Project scaffold with dependencies


### Other
- Watch-rs → watch2 (shorter slash command)

