# Phase 6 + 3: Detailed Implementation Plan

> **Date:** 2026-07-15
> **Status:** Draft — awaiting approval
> **Prerequisites:** Phases 1, 2, 4, 5 complete ✅

---

## Current State After Phase 1-5

| File | LOC | Role |
|------|-----|------|
| pipeline.rs | 771 | Orchestrator |
| vision.rs | 1026 | Vision analysis (merged) |
| download.rs | 476 | yt-dlp wrapper |
| moments.rs | 474 | Moment detection |
| stats.rs | 457 | Statistics |
| synthesis.rs | 417 | Grounded synthesis |
| corrections.rs | 368 | Transcript corrections |
| moment_frames.rs | 266 | Moment↔frame linking |
| whisper.rs | 264 | Whisper providers (trait-based) |
| config.rs | 175 | Configuration |
| frames/keyframe.rs | 159 | Keyframe extraction |
| output.rs | 150 | **Shared types** |
| cli.rs | 146 | CLI args |
| transcript.rs | 144 | JSON3 parsing |
| frames/scene.rs | 139 | Scene extraction |
| dedup.rs | 138 | Frame dedup |
| frames/timestamp.rs | 100 | Cue extraction |
| main.rs | 91 | Entry point |
| frames/mod.rs | 91 | Frame module root |
| frames/gap_fill.rs | 79 | Gap fill |
| scene.rs | 48 | Scene detection |
| frames/metadata.rs | 52 | Video metadata |
| frames/uniform.rs | 56 | Uniform extraction |
| frames/two_pass.rs | 73 | Two-pass extraction |
| error.rs | 27 | Error types |
| timestamp.rs | 39 | Time parsing |
| lib.rs | 19 | Module declarations |

**Total:** ~6,700 LOC across 27 files

---

## Phase 6: Shared Type Organization

### Decision: SKIP ❌

After analysis, Phase 6 is **not worth the churn** at this stage.

**Reasons:**

1. **`output.rs` is only 150 LOC** — already well-organized, clear naming
2. **16 modules import from it** — reorganizing means updating 16 import paths
3. **Types are logically grouped:**
   - `FrameInfo` — frame metadata
   - `TranscriptSegment` + `WordTiming` — transcript data
   - `WatchReport` + `KeyMomentStats` — output reports
4. **ROI is low:** Moving 150 LOC into `types/` submodule gains marginal discoverability but costs 16 import updates + risk of breakage
5. **The original plan estimated +100 LOC** for reorganization — net negative value

**Recommendation:** Revisit only if output.rs grows past 300 LOC or if we add 5+ new shared types.

---

## Phase 3: Caching Layer — DETAILED PLAN

### Why This Matters

- YouTube videos: 100MB-3GB per download
- Re-analyzing same video = re-download everything
- Subtitle anti-429 sleep (3s × 2 = 6s wasted)
- User's internet: Biznet Jakarta (~10MB/s) → 300MB video = 30s wasted

### Architecture

```
~/.cache/watch2/
├── index.json                    ← cache manifest (URL → metadata mapping)
├── <sha256>/                     ← per-video directory
│   ├── video.mp4                 ← downloaded video (optional, cleaned up)
│   ├── info.json                 ← video metadata
│   ├── video.en-orig.json3       ← original subtitles
│   └── video.en.json3            ← auto-generated subtitles
```

### Why `index.json` Instead of Scanning Directories

- Fast lookup (O(1) vs O(n) directory scan)
- Stores metadata (URL, timestamp, size) without reading files
- Enables LRU eviction by tracking access time
- Single source of truth for cache state

### Cache Key Design

```
key = SHA256(normalized_url)
```

**Normalization rules:**
- Strip YouTube `si` tracking param: `?si=xxx` removed
- Strip `&list=xxx` (playlist context)
- Keep `?v=xxx` (video identity)
- Normalize to `https://www.youtube.com/watch?v=xxx`

**Example:**
```
Input:  https://youtu.be/_g4l7YkDQwA?si=fyWWkLYS_qhYPdd-
Key:    a1b2c3d4...  (SHA256 of "https://www.youtube.com/watch?v=_g4l7YkDQwA")
Dir:    ~/.cache/watch2/a1b2c3d4.../
```

### Dependencies

| Crate | Version | Purpose | Binary Size |
|-------|---------|---------|-------------|
| `sha2` | 0.10 | SHA256 hashing | ~0 (Rust native) |
| (dirs) | 6 | Cache dir location | already used |

**Net new binary size:** ~0 (sha2 is pure Rust, no C deps)

### Module: `src/cache.rs` (~200 LOC)

```rust
use std::path::PathBuf;
use std::collections::HashMap;

/// Cache manifest entry.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct CacheEntry {
    url: String,
    key: String,           // SHA256 hash
    cached_at: u64,        // Unix timestamp
    accessed_at: u64,      // Unix timestamp (for LRU)
    size_bytes: u64,       // Total size of cached files
    has_video: bool,       // Whether video.mp4 exists (usually cleaned up)
    has_subtitles: bool,   // Whether subtitle files exist
    title: Option<String>, // Video title for display
}

/// File-based cache for video downloads and subtitles.
pub struct VideoCache {
    root: PathBuf,         // ~/.cache/watch2/
    manifest: HashMap<String, CacheEntry>,  // key → entry
    max_size_bytes: u64,   // Default 10GB
}

impl VideoCache {
    /// Create or open the cache at the default location.
    pub fn new() -> Result<Self>;

    /// Create or open the cache at a custom location.
    pub fn with_dir(root: PathBuf) -> Result<Self>;

    /// Generate cache key from URL.
    pub fn cache_key(url: &str) -> String;

    /// Get the cache directory for a given key.
    pub fn cache_dir(&self, key: &str) -> PathBuf;

    /// Check if subtitles are cached for this URL.
    pub fn has_subtitles(&self, url: &str) -> bool;

    /// Get cached subtitle path if available.
    pub fn get_subtitles(&self, url: &str, lang: &str) -> Option<PathBuf>;

    /// Store subtitle file in cache.
    pub fn store_subtitles(&mut self, url: &str, lang: &str, path: &Path) -> Result<PathBuf>;

    /// Check if video is cached for this URL.
    pub fn has_video(&self, url: &str) -> bool;

    /// Get cached video path if available.
    pub fn get_video(&self, url: &str) -> Option<PathBuf>;

    /// Store video file in cache.
    pub fn store_video(&mut self, url: &str, path: &Path) -> Result<PathBuf>;

    /// Get cached metadata (info.json) if available.
    pub fn get_info(&self, url: &str) -> Option<VideoInfo>;

    /// Store metadata in cache.
    pub fn store_info(&mut self, url: &str, info: &VideoInfo) -> Result<()>;

    /// Remove a specific URL from cache.
    pub fn invalidate(&mut self, url: &str) -> Result<()>;

    /// Evict oldest entries until total size is under max_size_bytes.
    pub fn evict(&mut self) -> Result<Vec<String>>;

    /// Get total cache size in bytes.
    pub fn total_size(&self) -> u64;

    /// Get number of cached entries.
    pub fn entry_count(&self) -> usize;

    /// Print cache stats to stderr.
    pub fn print_stats(&self);

    /// Save manifest to disk.
    fn save_manifest(&self) -> Result<()>;

    /// Load manifest from disk.
    fn load_manifest(root: &Path) -> Result<HashMap<String, CacheEntry>>;
}
```

### Integration Points

#### 1. `download.rs` — Check cache before downloading

```rust
// In fetch_captions():
pub fn fetch_captions(url: &str, out_dir: &Path, use_cookies: bool, cache: Option<&VideoCache>) -> Result<DownloadResult> {
    // Check cache for subtitles first
    if let Some(c) = cache {
        if c.has_subtitles(url) {
            if let Some(sub_path) = c.get_subtitles(url, "en") {
                // Copy to out_dir and return early
                // ... skip yt-dlp subtitle download
            }
        }
    }
    // ... existing logic
}

// In download_video():
pub fn download_video(url: &str, out_dir: &Path, use_cookies: bool, cache: Option<&VideoCache>) -> Result<DownloadResult> {
    // Check cache for video
    if let Some(c) = cache {
        if c.has_video(url) {
            if let Some(vid_path) = c.get_video(url) {
                // Copy to out_dir and return early
                // ... skip yt-dlp video download
            }
        }
    }
    // ... existing logic
}
```

#### 2. `pipeline.rs` — Pass cache through pipeline

```rust
pub struct PipelineContext {
    // ... existing fields
    pub cache: Option<VideoCache>,  // NEW
}

// In run():
// After download, store in cache:
if let Some(ref mut c) = cache {
    if dl_result.downloaded {
        if let Some(ref vp) = video_path {
            let _ = c.store_video(&cli.source, vp);
        }
    }
    if let Some(ref sp) = dl_result.subtitle_path {
        let _ = c.store_subtitles(&cli.source, "en", sp);
    }
    let _ = c.store_info(&cli.source, &dl_result.info);
}
```

#### 3. `cli.rs` — Add cache flags

```rust
/// Disable download cache
#[arg(long)]
pub no_cache: bool,

/// Custom cache directory
#[arg(long)]
pub cache_dir: Option<String>,
```

#### 4. `main.rs` — Initialize cache

```rust
let cache = if cli.no_cache {
    None
} else {
    let cache_dir = cli.cache_dir.as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| dirs::cache_dir()
            .unwrap_or_default()
            .join("watch2"));
    VideoCache::with_dir(cache_dir).ok()
};

// Pass to pipeline context
let ctx = PipelineContext {
    // ...
    cache,
};
```

### CLI Flags

| Flag | Description | Default |
|------|-------------|---------|
| `--no-cache` | Disable download cache entirely | false |
| `--cache-dir DIR` | Custom cache directory | `~/.cache/watch2/` |

### Eviction Policy

- **Max size:** 10GB (configurable via constant)
- **Eviction:** LRU by `accessed_at` timestamp
- **Trigger:** On each `store_*()` call, check total size
- **Behavior:** Remove oldest entries until under limit
- **Video cleanup:** Videos are cleaned up after processing (existing behavior), so cache mainly holds subtitles and metadata

### What Gets Cached

| Item | Size | TTL | Cached? |
|------|------|-----|---------|
| Video (.mp4) | 100MB-3GB | Cleaned after processing | Temporarily |
| Subtitles (.json3) | 1-10KB | 30 days | ✅ Yes |
| Metadata (.info.json) | 1-5KB | 30 days | ✅ Yes |
| Frames (.jpg) | 5-50MB | Session only | ❌ No |
| Report (.json) | 1-10KB | Session only | ❌ No |

### Tests to Add

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_cache_key_normalization();     // YouTube URL variants → same key
    #[test]
    fn test_cache_key_deterministic();     // Same URL → same key
    #[test]
    fn test_cache_store_and_retrieve();    // Round-trip store/get
    #[test]
    fn test_cache_invalidation();          // Remove entry
    #[test]
    fn test_cache_eviction();              // LRU eviction under size limit
    #[test]
    fn test_cache_stats();                 // Size/count reporting
    #[test]
    fn test_cache_manifest_persistence();  // Save/load manifest
    #[test]
    fn test_cache_max_size();              // Respects max_size_bytes
}
```

### Migration Path

1. Create `src/cache.rs` with full implementation
2. Add `sha2` to Cargo.toml
3. Add `--no-cache` and `--cache-dir` to CLI
4. Initialize cache in `main.rs`
5. Pass cache to `PipelineContext`
6. Modify `download.rs` to accept optional cache
7. Store results after download in `pipeline.rs`
8. Add tests
9. Build + test + manual verification

### Estimated LOC

| File | LOC |
|------|-----|
| `cache.rs` (new) | ~200 |
| `download.rs` (changes) | +20 |
| `pipeline.rs` (changes) | +30 |
| `cli.rs` (changes) | +10 |
| `main.rs` (changes) | +15 |
| **Total new** | **~275** |

### Expected Performance Impact

| Scenario | Before | After |
|----------|--------|-------|
| Re-analyze same video | 30s download + 6s subtitle sleep | **<1s cache hit** |
| Re-analyze 10 videos | 300s + 60s | **<10s total** |
| Cache miss (new video) | Same as before | Same + 1ms cache write |

---

## Implementation Order

Since Phase 6 is skipped, only Phase 3 remains:

1. Add `sha2` to Cargo.toml
2. Create `src/cache.rs`
3. Add CLI flags to `cli.rs`
4. Initialize cache in `main.rs`
5. Modify `download.rs` to accept cache
6. Wire cache through `pipeline.rs`
7. Add tests
8. Build + test + verify

---

## Success Criteria

1. ✅ All existing 162 tests pass
2. ✅ New cache tests pass (8+ tests)
3. ✅ `watch2 <url>` first run: downloads as normal, stores in cache
4. ✅ `watch2 <url>` second run: hits cache, skips download
5. ✅ `--no-cache` disables cache entirely
6. ✅ `--cache-dir /tmp/test-cache` uses custom directory
7. ✅ Cache eviction works (total size stays under limit)
8. ✅ Binary size increase < 100KB
