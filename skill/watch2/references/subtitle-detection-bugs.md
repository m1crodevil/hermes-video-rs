# Subtitle Detection Bugs in hermes-video-rs

> **v5.0.0 refactor (Jul 2026):** Pipeline simplified to single linear flow.
> Many of these bugs are no longer relevant — old pipeline code deleted.
> Kept for historical reference.

## Summary

## Bug #1: `find_subtitle()` Non-Deterministic (Critical) — FIXED

**Location:** `download.rs:466-476`

**Problem:** `read_dir()` return order is filesystem-dependent. In a directory with `video.ja-orig.json3`, `video.ja.json3`, AND `video.en.json3` (from pass 1), it could return the English file instead of the detected language.

**Fix:** `find_subtitle(dir, preferred_lang)` now takes a language parameter and prioritizes files matching the detected language pattern (`.ja.` or `.ja-` in filename).

## Bug #2: Dual-pass Download Conflict — FIXED

**Location:** `download.rs:309-397` (`download_video()`)

**Problem:** Pass 1 hardcoded `"en.*"` subtitle download before language detection ran. Created stale English subtitle files that `find_subtitle()` could pick up.

**Fix:** Pass 1 is now metadata-only (`--skip-download --write-info-json`). No subtitle download in pass 1.

## Bug #3: No Language Matching in `find_subtitle()` — FIXED

**Problem:** Only checked extension (`.json3`/`.vtt`), never checked language code.

**Fix:** New implementation sorts candidates by language match priority: preferred language first, then any subtitle file.

## Bug #4: Pipeline Double Download — MITIGATED

**Problem:** `fetch_captions()` downloads subs, then `download_video()` re-downloads.

**Mitigation:** `clean_stale_subtitles()` called before Pass 2 in `download_video()` to remove any stale files.

## Bug #5: `Path::extension()` Dot Prefix Mismatch — FIXED (2026-07-18)

**Location:** `download.rs` — `find_video()` line 462, `find_subtitle()` line 490

**Problem:** Both functions compared `path.extension()` against patterns with a dot prefix:
```rust
// BEFORE (broken):
for ext in &[".mp4", ".mkv", ".webm", ".mov", ".m4a", ".mp3"] {  // find_video
for ext in &[".json3", ".vtt"] {                                   // find_subtitle
```

Rust's `Path::extension()` returns the extension WITHOUT the dot (`"json3"`, not `".json3"`). So the comparison `"json3" == ".json3"` is always `false`. Both functions ALWAYS returned `None`.

**Impact:** This broke the entire transcript-moments pipeline. Subtitles were downloaded successfully but `find_subtitle()` could never find them. watch2 reported "No subtitles found" and fell through to balanced mode. Also affected `find_video()` — video path was always `None` after `fetch_captions()`.

**Why it wasn't caught earlier:** The video was eventually found by `download_video()` which calls `find_video()` — but only AFTER the transcript-moments fall-through. And `fetch_captions()` returned `Ok` with `subtitle_path: None`, which pipeline.rs treated as "no subtitles" rather than an error.

**Fix:**
```rust
// AFTER (fixed):
for ext in &["mp4", "mkv", "webm", "mov", "m4a", "mp3"] {  // find_video
for ext in &["json3", "vtt"] {                                // find_subtitle
```

**Verification:**
```bash
# Test: Path::extension() returns without dot
rustc -e 'use std::path::Path; let p = Path::new("video.json3"); println!("{:?}", p.extension());'
# Output: Some("json3")

# Test: fixed find_subtitle finds files
watch2 "https://youtu.be/VIDEO_ID" --detail transcript-moments --min-moments 50 --out-dir /tmp/test
# Should show: "parsing subtitles from ..." and "Phase 1: Generating moment detection prompt..."
```

## Applied Fix: `download.rs`

```rust
// NEW: Language-aware subtitle finder (v4.3.2)
fn find_subtitle(dir: &Path, preferred_lang: &str) -> Option<PathBuf> {
    let mut candidates: Vec<(bool, PathBuf)> = Vec::new();
    for ext in &["json3", "vtt"] {  // NOTE: no dot prefix — Path::extension() returns without dot
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |e| e == *ext) {
                let name = path.file_name().unwrap().to_string_lossy();
                let is_preferred = name.contains(&format!(".{}.", preferred_lang))
                    || name.contains(&format!(".{}-", preferred_lang));
                candidates.push((is_preferred, path));
            }
        }
    }
    candidates.sort_by(|a, b| b.0.cmp(&a.0)); // preferred first
    candidates.into_iter().next().map(|(_, p)| p)
}

// NEW: Find video — also fixed dot prefix (post-v4.4.0)
fn find_video(dir: &Path) -> Option<PathBuf> {
    for ext in &["mp4", "mkv", "webm", "mov", "m4a", "mp3"] {  // no dot prefix
        for entry in std::fs::read_dir(dir).ok()? {
            let entry = entry.ok()?;
            if entry.path().extension().map_or(false, |e| e == *ext) {
                return Some(entry.path());
            }
        }
    }
    None
}

// NEW: Clean stale subtitles between passes
fn clean_stale_subtitles(dir: &Path) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if ext == "json3" || ext == "vtt" {
                std::fs::remove_file(&path).ok();
            }
        }
    }
}
```

## Reproduction Recipe

```bash
# 1. Find a YouTube video with non-English auto-captions
watch2 "https://youtu.be/XUJwv-4iUrQ" --detail transcript-moments --min-moments 50 --out-dir /tmp/watch-test

# 2. Check if subtitles were downloaded
ls -la /tmp/watch-test/download/video.*.json3

# 3. Before fix: files exist but watch2 said "No subtitles found"
# 4. After fix: watch2 correctly picks Indonesian subtitle file
```

## LLM Language Detection (Implemented v4.3.2+)

Language detection now uses LLM (Groq → OpenAI fallback) to analyze video title + description. See `llm.rs` module and `pipeline.rs` Step 1b.

**Fallback chain**: LLM detection → yt-dlp `info.language` → available subs → English → Whisper fallback.

**User requirement**: "This skill must be global/universal — no hardcoded language keywords. Use LLM to determine which subtitles to download, and where the fallback goes." — No hardcoded language keywords. LLM determines content language from title + description.

## Related Code Paths

- `download.rs:160` — `subtitle_lang_pattern()`: builds glob pattern like `id.*`
- `download.rs:171` — `list_available_subtitles()`: parses `yt-dlp --list-subs`
- `download.rs:226` — `fetch_captions()`: subtitle-only download
- `download.rs:313` — `download_video()`: full download with subtitles
- `download.rs:461` — `find_video()`: video file selection (fixed dot prefix)
- `download.rs:473` — `find_subtitle()`: language-aware subtitle file selection (fixed dot prefix)
- `download.rs:498` — `clean_stale_subtitles()`: remove stale files between passes
- `config.rs:46` — `suggest_subtitle_language()`: language priority logic
- `pipeline.rs:96` — transcript-moments fall-through check
