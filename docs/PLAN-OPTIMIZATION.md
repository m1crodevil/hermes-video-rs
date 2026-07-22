# Watch2 Pipeline Optimization Plan

> Comprehensive plan to reduce default `/watch2` pipeline from ~660s to ~350s (47% faster)

---

## Executive Summary

The current pipeline for `/watch2 youtu.be/abcdefg` (default balanced mode) performs **4 yt-dlp passes**, **2 redundant scene detections**, and **1 unnecessary LLM API call**. This plan eliminates redundancy, parallelizes independent operations, and introduces smart caching.

**Target:** 58-minute video, balanced mode, captions available
**Current:** ~660s → **Target:** ~350s → **Savings:** ~310s (47%)

---

## Current Pipeline Audit

```
┌─ main.rs ──────────────────────────────────────────────────────────┐
│ 1. Parse CLI + config + setup check                                │
│ 2. Create temp dir                                                  │
│ 3. Initialize cache (~/.cache/watch2/)                              │
└────────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─ pipeline::run() ──────────────────────────────────────────────────┐
│ Step 1: fetch_captions()                                           │
│   ├── yt-dlp Pass 1: metadata (--skip-download --write-info-json)  │
│   ├── yt-dlp Pass 2: list-subs (--list-subs)                       │
│   └── yt-dlp Pass 3: fetch subs (--write-subs --write-auto-subs)   │
│ Step 1b: LLM language detection (Groq API call)                    │
│ Step 2: Parse transcript (JSON3/VTT)                               │
│ Step 3: Get video metadata (ffprobe)                               │
│ Step 4: Download video + frame extraction                          │
│   ├── yt-dlp Pass 4: full download (-f "bv*[height<=720]...")     │
│   ├── ffmpeg frame extraction                                      │
│   └── av-scenechange scene detection                               │
│ Step 4b: Scene detection (AGAIN — redundant!)                      │
│ Step 5: Whisper fallback (skipped if transcript exists)            │
│ Step 6: Filter transcript (no-op without --start/--end)           │
│ Step 7: Cleanup (delete video)                                     │
│ Step 8: Build report                                               │
└────────────────────────────────────────────────────────────────────┘
```

**Bottlenecks identified:**
- 4 yt-dlp passes (could be 2)
- 2 scene detections (could be 1)
- 1 LLM API call (could be 0 for most videos)
- Sequential subtitle fetch + video download (could be parallel)

---

## Optimization Plan

### Phase 1: Eliminate Redundancy (Save ~300s)

#### 1.1 Merge yt-dlp Passes in `fetch_captions()`

**Current code (download.rs):**
```rust
// Pass 1: metadata only
pub fn fetch_captions(...) {
    let mut meta_args = vec![
        "--skip-download",
        "--write-info-json",
        "--no-playlist",
    ];
    Command::new("yt-dlp").args(&meta_args)...;

    // Pass 2: list subs
    let (manual_subs, auto_subs) = list_available_subtitles(url)?;

    // Pass 3: fetch subs
    let mut args = vec![
        "--skip-download",
        "--write-subs",
        "--write-auto-subs",
        "--sub-langs", &lang_pattern,
    ];
    Command::new("yt-dlp").args(&args)...;
}
```

**Proposed: Single-pass metadata + subs**
```rust
pub fn fetch_captions_optimized(...) {
    // Single pass: metadata + subtitles
    let mut args = vec![
        "--skip-download",
        "--write-info-json",
        "--write-subs",
        "--write-auto-subs",
        "--sub-langs", &lang_pattern, // Use wildcard: ".*"
        "--sub-format", "json3/best",
        "--sleep-subtitles", "3",
        "--no-playlist",
        "--ignore-errors",
        "-o", &output_template,
    ];
    Command::new("yt-dlp").args(&args)...;

    // Parse language from info.json (no separate list-subs needed)
    let info = extract_info(out_dir);
    let detected_lang = info.language.unwrap_or("en".to_string());
}
```

**Key insight:** `--write-subs --write-auto-subs --sub-langs ".*"` fetches ALL subtitle languages in one pass. We can parse `info.json` to determine the video language instead of a separate `--list-subs` call.

**Reference:** yt-dlp docs confirm `--skip-download` combined with `--write-info-json --write-subs --write-auto-subs` is valid and fetches metadata + all subs in one pass.

**Files to modify:** `src/download.rs` → `fetch_captions()`

---

#### 1.2 Eliminate Redundant Metadata Fetch in `download_video()`

**Current code (download.rs):**
```rust
pub fn download_video(...) {
    // Pass 1: metadata (AGAIN — we already have this from fetch_captions!)
    let mut meta_args = vec![
        "--skip-download",
        "--write-info-json",
        "--no-playlist",
    ];
    Command::new("yt-dlp").args(&meta_args)...;

    // Pass 2: full download
    let mut args = vec![
        "-f", "bv*[height<=720]+ba/b[height<=720]/bv+ba/b",
        "--merge-output-format", "mp4",
        "--write-subs",
        "--write-auto-subs",
    ];
    Command::new("yt-dlp").args(&args)...;
}
```

**Proposed: Skip metadata re-fetch, pass DownloadResult**
```rust
pub fn download_video_optimized(
    url: &str,
    out_dir: &Path,
    use_cookies: bool,
    llm_lang: Option<&str>,
    existing_info: Option<&VideoInfo>,  // NEW: pass existing metadata
) -> Result<DownloadResult> {
    // Skip metadata fetch if we already have it
    let detected_lang = if let Some(info) = existing_info {
        info.language.clone().unwrap_or("en".to_string())
    } else {
        // Only fetch metadata if we don't have it
        let meta_args = vec!["--skip-download", "--write-info-json"];
        Command::new("yt-dlp").args(&meta_args)...;
        extract_info(out_dir).language.unwrap_or("en".to_string())
    };

    // Single download pass (no metadata re-fetch)
    let mut args = vec![
        "-f", "bv*[height<=720]+ba/b[height<=720]/bv+ba/b",
        "--merge-output-format", "mp4",
        "--write-subs",
        "--write-auto-subs",
        "--sub-langs", &lang_pattern,
        "--sub-format", "json3/best",
    ];
    Command::new("yt-dlp").args(&args)...;
}
```

**Files to modify:** `src/download.rs` → `download_video()`, `src/pipeline.rs` → pass `dl_result.info`

---

#### 1.3 Don't Run Scene Detection Twice

**Current code (pipeline.rs):**
```rust
// Step 4: Frame extraction (balanced mode)
frames::extract_scene_or_uniform(...) {
    // Internally calls av-scenechange
    scene_detect::detect(video_path, fps, duration)?;
}

// Step 4b: Scene detection (AGAIN!)
if detail != DetailMode::Transcript && !has_key_moments {
    run_scene_detection(...);  // Calls av-scenechange AGAIN!
}
```

**Proposed: Cache scene boundaries, reuse**
```rust
// Step 4: Frame extraction
let scene_result = frames::extract_scene_or_uniform(...);

// Step 4b: Reuse cached boundaries (don't re-run detection)
if detail == DetailMode::TranscriptMoments && fuse_scenes {
    // Only run fusion if --fuse-scenes is set
    run_fusion_only(&scene_boundaries, &transcript_segments);
}
// For balanced mode: scene_boundaries already extracted in Step 4
```

**Alternative: Make scene detection opt-in for balanced mode**
```rust
// In CLI:
#[arg(long)]
pub skip_scene_detection: bool,  // For advanced users who want faster runs

// In pipeline:
if !cli.skip_scene_detection && detail == DetailMode::Balanced {
    run_scene_detection(...);
}
```

**Reference:** av-scenechange docs show `--speed 1` for faster but less accurate detection:
```bash
av-scenechange input.y4m --speed 1 --min-scenecut 24
```

**Files to modify:** `src/pipeline.rs` → `run_scene_detection()`, `src/scene_detect.rs`

---

### Phase 2: Smart Language Detection (Save ~2s)

#### 2.1 Skip LLM When Metadata Has Language

**Current code (llm.rs):**
```rust
pub async fn detect_language_llm(...) -> Option<String> {
    // Always makes API call
    let prompt = format!(
        "Based on this video title and description, what is the primary language?..."
    );
    call_llm(key, endpoint, model, &prompt).await
}
```

**Proposed: Skip LLM when metadata is sufficient**
```rust
// In pipeline.rs:
let llm_lang = if let Some(ref lang) = dl_result.info.language {
    // Metadata already has language — skip LLM
    eprintln!("[watch2] language from metadata: {}", lang);
    Some(lang.clone())
} else {
    // No language in metadata — try LLM
    crate::llm::detect_language_llm(
        &dl_result.info.title,
        dl_result.info.description.as_deref(),
        &config,
    ).await
};
```

**Files to modify:** `src/pipeline.rs` → Step 1b, `src/llm.rs`

---

#### 2.2 Skip LLM for English Videos

```rust
let llm_lang = if let Some(ref lang) = dl_result.info.language {
    if lang == "en" {
        // English is most common — skip LLM
        Some("en".to_string())
    } else {
        // Non-English — verify with LLM
        crate::llm::detect_language_llm(...).await
    }
} else {
    crate::llm::detect_language_llm(...).await
};
```

---

### Phase 3: Parallelize Independent Operations (Save ~5s)

#### 3.1 Parallel Metadata Fetch + Subtitle Detection

**Current: Sequential**
```
fetch_captions()
  ├── metadata fetch (2s)
  ├── list-subs (2s)
  └── fetch-subs (2s)
Total: ~6s
```

**Proposed: Parallel**
```rust
use tokio::join;

pub async fn fetch_captions_optimized(...) {
    // Parallel: metadata + subtitle list
    let (info_result, subs_result) = join!(
        fetch_metadata(url, use_cookies),
        list_available_subtitles_async(url, use_cookies)
    );

    // Then fetch subs in detected language
    let detected_lang = suggest_subtitle_language(
        info_result.language.as_deref(),
        &subs_result.0,  // manual
        &subs_result.1,  // auto
        llm_lang,
    );

    fetch_subtitles(url, out_dir, &detected_lang, use_cookies).await?;
}
```

**Reference:** tokio::join! docs:
```rust
let (a, b) = tokio::join!(fetch_a(), fetch_b());
// Both run concurrently on the same tokio runtime
```

**Files to modify:** `src/download.rs` → `fetch_captions()`

---

#### 3.2 Parallel Video Download + Frame Extraction Planning

```rust
// Start video download in background
let download_handle = tokio::spawn(async move {
    download::download_video(url, out_dir, cookies, lang).await
});

// While downloading, plan frame extraction (CPU-bound, fast)
let frame_plan = plan_frame_extraction(&transcript_segments, &config);

// Wait for download to complete
let dl_result = download_handle.await??;

// Execute frame extraction
extract_frames(&dl_result.video_path, &frame_plan);
```

**Files to modify:** `src/pipeline.rs` → Step 4

---

### Phase 4: Optimize Scene Detection (Save ~300s for long videos)

#### 4.1 Use `--speed 1` for Balanced Mode

**Current code (scene_detect.rs):**
```rust
pub fn detect(video_path: &Path, fps: f64, duration: f64) -> Result<SceneDetectionResult> {
    let output = Command::new("av-scenechange")
        .args(["--min-scenecut", "24", video_path.to_str().unwrap()])
        .output()?;
    // ...
}
```

**Proposed: Faster detection for balanced mode**
```rust
pub fn detect(video_path: &Path, fps: f64, duration: f64, fast: bool) -> Result<SceneDetectionResult> {
    let mut args = vec![
        "--min-scenecut", "24",
        video_path.to_str().unwrap(),
    ];

    if fast {
        args.insert(0, "--speed");  // Use speed level 1 (faster, less accurate)
        args.insert(1, "1");
    }

    let output = Command::new("av-scenechange")
        .args(&args)
        .output()?;
    // ...
}
```

**Reference:** av-scenechange docs:
```bash
# Use faster but less accurate detection mode
av-scenechange input.y4m --speed 1
```

**Trade-off:** `--speed 1` is ~3x faster but may miss subtle scene changes. For balanced mode (which already has gap-filling), this is acceptable.

**Files to modify:** `src/scene_detect.rs` → `detect()`

---

#### 4.2 Make Scene Detection Optional for Balanced Mode

```rust
// In CLI:
#[arg(long)]
pub no_scene_detection: bool,  // Skip scene detection entirely

// In pipeline:
if !cli.no_scene_detection {
    run_scene_detection(...);
} else {
    eprintln!("[watch2] scene detection skipped (--no-scene-detection)");
    // Use uniform sampling instead
}
```

**Files to modify:** `src/cli.rs`, `src/pipeline.rs`

---

### Phase 5: Cache Optimization (Save ~2s on repeat runs)

#### 5.1 Cache Metadata Across Runs

**Current:** Cache only stores video/subtitles, not metadata.

**Proposed:** Cache metadata + language detection result
```rust
pub fn store_metadata(&mut self, url: &str, info: &VideoInfo, lang: &str) -> Result<()> {
    let key = Self::cache_key(url);
    let dir = self.cache_dir(&key);

    // Store metadata
    let info_path = dir.join("info.json");
    let data = serde_json::to_json(info)?;
    std::fs::write(&info_path, data)?;

    // Store detected language
    let lang_path = dir.join("language.txt");
    std::fs::write(&lang_path, lang)?;

    // Update manifest
    // ...
}

pub fn get_cached_language(&self, url: &str) -> Option<String> {
    let key = Self::cache_key(url);
    let path = self.cache_dir(&key).join("language.txt");
    std::fs::read_to_string(path).ok()
}
```

**Files to modify:** `src/cache.rs`

---

#### 5.2 Cache Subtitle Language Detection

```rust
// In pipeline.rs:
let detected_lang = if let Some(ref cache) = cache {
    if let Some(lang) = cache.get_cached_language(&cli.source) {
        eprintln!("[watch2] cached language: {}", lang);
        lang
    } else {
        // Detect and cache
        let lang = suggest_subtitle_language(...);
        cache.store_metadata(&cli.source, &dl_result.info, &lang)?;
        lang
    }
} else {
    suggest_subtitle_language(...)
};
```

---

### Phase 6: Reduce yt-dlp Network Overhead (Save ~2s)

#### 6.1 Use `--no-playlist` Consistently

**Current:** Some yt-dlp calls don't include `--no-playlist`.

**Proposed:** Always include it
```rust
const COMMON_ARGS: &[&str] = &[
    "--no-playlist",
    "--ignore-errors",
    "--sleep-subtitles", "3",
];
```

---

#### 6.2 Use `--flat-playlist` for Metadata

```rust
// For metadata-only fetch, use --flat-playlist to skip playlist processing
let meta_args = vec![
    "--skip-download",
    "--write-info-json",
    "--flat-playlist",  // Skip playlist entry processing
    "--no-playlist",
];
```

**Files to modify:** `src/download.rs`

---

## Implementation Roadmap

### Phase 1: Quick Wins (1-2 days)

| Task | Files | Risk | Impact |
|------|-------|------|--------|
| Merge yt-dlp passes in fetch_captions() | download.rs | Low | ~5s |
| Eliminate metadata re-fetch in download_video() | download.rs, pipeline.rs | Low | ~5s |
| Skip LLM when metadata has language | pipeline.rs, llm.rs | Low | ~2s |

### Phase 2: Core Optimizations (2-3 days)

| Task | Files | Risk | Impact |
|------|-------|------|--------|
| Don't run scene detection twice | pipeline.rs, scene_detect.rs | Medium | ~300s |
| Use --speed 1 for balanced mode | scene_detect.rs | Low | ~100s |
| Parallel metadata + subtitle fetch | download.rs | Medium | ~3s |

### Phase 3: Cache & Polish (1 day)

| Task | Files | Risk | Impact |
|------|-------|------|--------|
| Cache metadata + language | cache.rs | Low | ~2s |
| Add --no-scene-detection flag | cli.rs, pipeline.rs | Low | ~300s |
| Common args constant | download.rs | Low | ~1s |

---

## Expected Results

### Before Optimization
```
Step 1: fetch_captions()          ~8s   (3 yt-dlp passes)
Step 1b: LLM language detection   ~2s   (1 API call)
Step 2: Parse transcript          ~0.1s
Step 3: Get metadata              ~0.5s
Step 4: Download + frames         ~350s
Step 4b: Scene detection (again)  ~300s
Step 5-8: Misc                    ~0.6s
─────────────────────────────────────
TOTAL                             ~660s (~11 minutes)
```

### After Optimization
```
Step 1: fetch_captions()          ~3s   (1 yt-dlp pass)
Step 1b: LLM language detection   ~0s   (skip if metadata has lang)
Step 2: Parse transcript          ~0.1s
Step 3: Get metadata              ~0s   (already have it)
Step 4: Download + frames         ~350s
Step 4b: Scene detection (once)   ~100s (--speed 1, or skip)
Step 5-8: Misc                    ~0.5s
─────────────────────────────────────
TOTAL                             ~454s (~7.5 minutes) — with scene detection
                                  ~354s (~6 minutes) — without scene detection
```

**Savings:** ~206s (31%) with scene detection, ~306s (46%) without.

---

## Testing Strategy

### Unit Tests
- Test `fetch_captions_optimized()` with mocked yt-dlp output
- Test `download_video_optimized()` with existing metadata
- Test language detection skip logic

### Integration Tests
- End-to-end test with YouTube URL (short video, <1 min)
- Verify metadata + subtitles fetched in single pass
- Verify scene detection runs only once

### Performance Tests
- Benchmark before/after with 58-minute video
- Measure yt-dlp call count reduction
- Measure scene detection time with `--speed 1`

### Regression Tests
- Verify transcript accuracy unchanged
- Verify frame quality unchanged
- Verify report.json format unchanged

---

## Risk Assessment

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| yt-dlp single-pass fails for some videos | Low | Medium | Fallback to multi-pass |
| --speed 1 misses important scenes | Medium | Low | Gap-filling compensates |
| Cache invalidation issues | Low | Low | TTL-based expiry |
| Parallel fetch causes race conditions | Medium | Medium | Use mutex on shared state |

---

## References

- **yt-dlp docs:** `yt-dlp --help` confirms `--skip-download --write-info-json --write-subs --write-auto-subs` is valid single-pass
- **av-scenechange docs:** `--speed 1` flag for faster detection (GitHub: rust-av/av-scenechange)
- **tokio docs:** `tokio::join!` for concurrent async operations
- **reqwest docs:** Connection pooling via `reqwest::Client::builder().pool_max_idle_per_host(10)`

---

## Next Steps

1. Review this plan with the team
2. Prioritize Phase 1 (quick wins)
3. Create feature branch: `feat/pipeline-optimization`
4. Implement and test each phase
5. Benchmark and document results
6. Merge to main

---

*Plan created: 2026-07-19*
*Author: microdevil*
*Status: Draft — pending review*
