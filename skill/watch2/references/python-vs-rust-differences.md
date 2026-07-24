# Python hermes-video vs Rust hermes-video-rs

Reference for keeping Rust version in sync with Python version.

> **v5.0.0 refactor (Jul 2026):** Rust version simplified to single linear pipeline.
> Removed: 6 detail modes, 7 frame engines, fusion, corrections, synthesis modules.
> See pipeline.rs for current architecture.

## Download Behavior

| Feature | Python (`download.py`) | Rust (`download.rs`) | Status |
|---------|----------------------|---------------------|--------|
| Resolution cap | `bv*[height<=720]+ba/b[height<=720]/bv+ba/b` | Same (fixed v4.2.1) | ✅ |
| Merge format | `--merge-output-format mp4` | Same (fixed v4.2.1) | ✅ |
| Subtitle anti-429 | `--sleep-subtitles 3` + skip re-download | Same | ✅ |
| Network opts | deno + curl_cffi + opt-in cookies | Same | ✅ |
| YouTube 2026 | `android_vr,web_creator` player client | Same | ✅ |

## Cleanup Behavior

Both versions delete the video after processing unless `--keep-video` is passed.

**Python** (`pipeline.py:29-46`):
```python
def _cleanup_video(video_path, downloaded, keep):
    if not video_path or not downloaded or keep:
        return
    p.unlink()
```

**Rust** (`main.rs:494-500`):
```rust
if !cli.keep_video {
    if let Some(ref vp) = video_path {
        if dl_result.downloaded {
            std::fs::remove_file(vp).ok();
        }
    }
}
```

Both also clean up temp audio files (Python: atexit rmtree; Rust: explicit audio.mp3 removal).

## Key Differences

### Frame Extraction
- Python uses `watch/frames.py` with scene detection via ffmpeg `select='gt(scene,T)'`
- Rust uses `src/frames.rs` with same ffmpeg approach

### Transcript Parsing
- Both parse JSON3 format from yt-dlp subtitles
- Same language detection logic (`suggest_subtitle_language`)

### Detail Modes
Both support: transcript, transcript-moments, efficient, balanced, token-burner, screenshot-first

### Stats
- Python: `watch/stats.py` with `StatsTimer` class
- Rust: `src/stats.rs` with same timing logic

## SKILL.md Workflow Parity (Fixed Jul 2026)

Both Python and Rust SKILL.md now share identical workflow structure:

### ✅ Parity Checklist
| Element | Python | Rust | Status |
|---------|--------|------|--------|
| Quick Start: transcript-moments first | ✅ | ✅ | ✅ |
| Workflow section with checklist (Step 1-6) | ✅ | ✅ | ✅ |
| Decision tree (captions → transcript-moments) | ✅ | ✅ | ✅ |
| "When to Use" mode comparison table | ✅ | ✅ | ✅ |
| Anti-hallucination rules | ✅ | ✅ | ✅ |
| `--out-dir` critical warning | ✅ | ✅ | ✅ |
| `vision_analyze` 21+ frames guidance | ✅ | ✅ | ✅ |

### Workflow Steps (identical in both)
1. Run with `--detail transcript-moments --min-moments 50 --out-dir <FIXED_DIR>`
2. Read `moments_prompt.txt`, analyze transcript, identify 50+ key moments
3. Write `key_moments.json` to same `--out-dir`
4. Re-run with same args (video downloads + frames extracted)
5. `vision_analyze` 21+ representative frames with specific questions
6. Apply corrections, generate grounded summary

**Lesson**: SKILL.md is the agent's behavior contract. If the workflow isn't prescriptive (checklist with explicit steps), agents will default to the simplest path (balanced mode) even when a better path exists.

## When Updating Rust Version

Always cross-check Python `download.py` for:
1. Format string (`-f` flag) — must cap at 720p
2. `--merge-output-format mp4` — must be present
3. Network opts (deno, curl_cffi, cookies)
4. Subtitle sleep timing

Check Python `pipeline.py` for:
1. Cleanup logic — must match Python behavior
2. Detail mode routing
3. Error handling patterns
