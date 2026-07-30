# hermes-video-rs — Comprehensive Testing Plan (T0–T5)

**Date:** 2026-07-30
**Current State:** 123 tests, ~30% logic path coverage
**Target:** ~255 tests, ~80% logic path coverage
**Repo:** /home/microdevil/hermes-video-rs/

---

## Research Sources

| Source | URL | Key Findings |
|--------|-----|-------------|
| clap docs | https://docs.rs/clap/latest/clap/ | `try_parse_from` for unit tests, `assert_cmd` for integration |
| serde unit testing | https://serde.rs/unit-testing.html | `serde_test` crate, roundtrip pattern, `json!` macro |
| reqwest | https://docs.rs/reqwest/latest/reqwest/ | `wiremock` for HTTP mocking, `#[tokio::test]` required |
| async-trait | https://docs.rs/async-trait/latest/async_trait/ | `mockall` with `#[automock]` for trait mocking |
| tempfile | https://docs.rs/tempfile/latest/tempfile/ | RAII cleanup, drop handles before TempDir scope end |
| av-scenechange | https://github.com/rust-av/av-scenechange | `detect_scene_changes::<u8>()`, `Decoder::from_file()`, requires `ffmpeg` feature |
| sha2/hex | https://docs.rs/sha2/latest/sha2/ | `Sha256::update → finalize → hex::encode()` |
| thiserror | https://docs.rs/thiserror/latest/thiserror/ | `#[error("...")]` for Display, `#[from]` for auto-conversion |
| anyhow | https://docs.rs/anyhow/latest/anyhow/ | `bail!()`, `.context()`, `?` operator |
| which | https://docs.rs/which/latest/which/ | `which("bin").is_ok()` for binary detection |
| mockall | https://docs.rs/mockall/latest/mockall/ | `#[automock]` trait mocking, predicate matching |
| wiremock | https://docs.rs/wiremock/latest/wiremock/ | HTTP mock server, random port assignment |
| assert_cmd | https://docs.rs/assert_cmd/latest/assert_cmd/ | CLI integration testing |
| serde_test | https://docs.rs/serde_test/latest/serde_test/ | Token-level serialization assertions |
| tarpaulin | https://github.com/xd009642/tarpaulin | Coverage with `--engine llvm --fail-under 80` |
| Rust testing book | https://doc.rust-lang.org/book/ch11-03-test-organization.html | Unit vs integration vs doc tests |
| clippy lints | https://doc.rust-lang.org/rustc/lints/listing/allowed-by-default.html | Pedantic lints for strict mode |

---

## Pre-Phase: New Dev Dependencies

Add to `Cargo.toml [dev-dependencies]`:

```toml
[dev-dependencies]
# Existing (keep)
tempfile = "3"
# assert_cmd = "2"  # Keep — will use in T3
# predicates = "3"  # Keep — will use in T3

# New (add)
wiremock = "0.6"           # HTTP mock server for reqwest tests
mockall = "0.13"           # Trait mocking (CommandRunner, WhisperProvider)
serde_test = "1"           # Token-level serialization assertions
```

**Rationale:** wiremock for whisper.rs API tests, mockall for subprocess abstraction, serde_test for precise serde edge cases.

---

## Phase T0: Dead Code Cleanup

### T0.1: Remove Unused Crates
- **File:** `Cargo.toml`
- **Remove:** `async-openai` (listed but never imported), `groq-api-rust` (listed but never imported), `chrono` (listed but never imported), `regex` (listed but never imported)
- **Verify:** `cargo check` passes, `cargo test` all 123 pass
- **Expected:** ~2MB binary size reduction

### T0.2: Remove Dead Types/Variants
- **File:** `cli.rs` — Remove `WhisperBackend` enum (defined but never used)
- **File:** `error.rs` — Remove `NoCaptions` variant (defined but never constructed)
- **Verify:** `cargo check` + `cargo test`

### T0.3: Clean Dead Functions
- **File:** `scene_detect.rs` — `detect_with_ffmpeg()` is a stub that always returns error
- **Action:** Remove entirely (dead code confuses contributors)
- **Verify:** `cargo check` + `cargo test`

### T0.4: Handle `filter_by_range()`
- **File:** `transcript.rs` — Public function, never called
- **Action:** Keep + add tests in T2 (will be useful for --start/--end filtering)

### T0.5: Verify Clean Build
- `cargo test` — all 123 tests pass
- `cargo clippy` — zero warnings (current state)
- `cargo fmt --check` — formatting clean

---

## Phase T1: Pure Logic Unit Tests (No I/O)

**Strategy:** Test all pure functions that don't touch filesystem, network, or subprocesses.
**Pattern:** `#[cfg(test)] mod tests` inline in each source file.
**Reference:** Rust testing book — unit tests go inside module with `#[cfg(test)]`

### T1.1: `frames/mod.rs` — Sampling Logic

```rust
// Pattern from docs: pure function testing
#[test]
fn even_indices_basic() {
    assert_eq!(even_indices(10, 3), vec![0, 4, 9]);
}
```

| Test Case | Input | Expected |
|-----------|-------|----------|
| Empty count | `even_indices(0, 5)` | `vec![]` |
| Single element | `even_indices(1, 1)` | `vec![0]` |
| n >= count | `even_indices(3, 10)` | `vec![0, 1, 2]` |
| First+last guaranteed | `even_indices(10, 3)` | `vec![0, 4, 9]` |
| Even spacing | `even_indices(100, 10)` | 10 evenly spaced |
| n=1 | `even_indices(10, 1)` | `vec![0]` |

**scale_filter tests:**
| Test Case | Expected |
|-----------|----------|
| Contains resolution | Output contains "512" |
| Contains MAX_READ_DIMENSION | Output contains "1998" |
| Valid ffmpeg syntax | Output matches `scale=w=...` |

**Est. LOC:** ~90, **Tests:** ~9

### T1.2: `scene_detect.rs` — Scoring & Classification

```rust
// Pattern from av-scenechange docs: test scoring math
#[test]
fn classify_at_cut() {
    let boundary = SceneBoundary::new(10.0, 15.0, 0, 0);
    assert_eq!(boundary.classify_position(10.0), "AtCut");
}
```

**classify_position_str tests:**
| Test Case | Timestamp | Expected |
|-----------|-----------|----------|
| Exact start | `start_sec` | "AtCut" |
| 2s after start | `start_sec + 2.0` | "EarlyScene" |
| 2s before end | `end_sec - 2.0` | "LateScene" |
| Middle | `(start+end)/2` | "MidScene" |
| Zero duration | `start == end` | "AtCut" |

**significance tests:**
| Test Case | Expected |
|-----------|----------|
| All None | 0.0 |
| Only inter_cost | `inter_cost` value |
| All scores set | Weighted sum |
| Negative scores | Handles gracefully |

**timestamps_to_boundaries tests:**
| Test Case | Expected |
|-----------|----------|
| Empty | Empty vec |
| Single timestamp | Single boundary |
| Multiple sorted | Multiple boundaries |
| Unsorted input | Sorted output |

**Est. LOC:** ~130, **Tests:** ~14

### T1.3: `error.rs` — Error Handling

```rust
// Pattern from thiserror docs: test Display and From conversions
#[test]
fn watch_error_display() {
    let err = WatchError::Download("bad url".into());
    assert_eq!(err.to_string(), "yt-dlp error: bad url");
}
```

| Test Case | Expected |
|-----------|----------|
| Download Display | "yt-dlp error: {msg}" |
| Ffmpeg Display | "ffmpeg error: {msg}" |
| Whisper Display | "Whisper API error: {msg}" |
| Config Display | "Config error: {msg}" |
| From io::Error | `WatchError::Io` variant |
| From serde_json::Error | `WatchError::Json` variant |
| sanitize_path normal | `/home/user/video.mp4` → `"video.mp4"` |
| sanitize_path root | `/` → `"/"` |
| sanitize_path no slash | `no-slash` → `"no-slash"` |
| sanitize_path unicode | Unicode filename extracted |

**Est. LOC:** ~80, **Tests:** ~10

### T1.4: `download.rs` — URL & Pattern Logic

```rust
// Pattern: pure function testing for URL manipulation
#[test]
fn sanitize_url_strips_control_chars() {
    let url = "https://example.com\x00/video";
    assert_eq!(sanitize_url(url), "https://example.com/video");
}
```

| Test Case | Input | Expected |
|-----------|-------|----------|
| Control char \x00 | `"https://x.com\x00/v"` | Stripped |
| Normal URL | `"https://youtube.com"` | Unchanged |
| Newline/tab | `"https://x.com\n\t/v"` | Stripped |
| Empty string | `""` | `""` |
| is_url http | `"http://example.com"` | true |
| is_url https | `"https://example.com"` | true |
| is_url local | `"/local/path"` | false |
| is_url ftp | `"ftp://server"` | false |
| is_url dash | `"-flag"` | false |
| subtitle_lang en | `"en"` | `"en.*"` |
| subtitle_lang en-US | `"en-US"` | `"en.*"` |
| subtitle_lang zh-Hans | `"zh-Hans"` | `"zh.*"` |

**Est. LOC:** ~100, **Tests:** ~12

### T1.5: `cache.rs` — URL Normalization

```rust
// Pattern: test URL normalization before hashing
#[test]
fn normalize_youtu_be_shorturl() {
    let normalized = normalize_url("https://youtu.be/abc123");
    assert!(normalized.contains("youtube.com/watch?v=abc123"));
}
```

| Test Case | Input | Expected |
|-----------|-------|----------|
| youtu.be short | `"youtu.be/abc"` | `youtube.com/watch?v=abc` |
| si= tracking | URL with `&si=xyz` | Stripped |
| list= playlist | URL with `&list=xyz` | Stripped |
| Non-YouTube | `"https://vimeo.com/123"` | Unchanged |
| Multiple tracking | URL with `&si=&list=` | Both stripped |
| Fragment # | URL with `#section` | Preserved (not stripped) |

**Est. LOC:** ~60, **Tests:** ~6

### T1.6: `config.rs` — Placeholder Detection

| Test Case | Input | Expected |
|-----------|-------|----------|
| Placeholder "your_" | `"your_api_key_here"` | true |
| Placeholder "sk-" | `"sk-your-key"` | true |
| Real key | `"gsk_abc123def456ghi"` | false |
| Short no-space | `"abc"` | true |
| Empty | `""` | true |

**Est. LOC:** ~40, **Tests:** ~5

### T1.7: `output.rs` — Report Serialization

```rust
// Pattern from serde docs: roundtrip testing
#[test]
fn watch_report_roundtrip() {
    let report = make_test_report();
    let json = report.to_json();
    let parsed: WatchReport = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.title, report.title);
}
```

| Test Case | Expected |
|-----------|----------|
| Empty report → JSON | Valid JSON, no panic |
| Report with frames | `frames` array present |
| skip_serializing_if | None fields omitted from JSON |
| Roundtrip | to_json → from_str → same data |

**Est. LOC:** ~50, **Tests:** ~4

**Total T1: ~550 LOC, ~60 tests**

---

## Phase T2: Structured Data Unit Tests (Filesystem)

**Strategy:** Use `tempfile::tempdir()` for isolation. Test file I/O, parsing, serialization.
**Pattern:** `#[test]` with tempdir setup/teardown.
**Reference:** tempfile docs — "Drop all file handles BEFORE TempDir goes out of scope"

### T2.1: `download.rs` — Metadata Extraction

```rust
// Pattern: tempdir + write fixture + test extraction
#[test]
fn extract_info_valid() {
    let tmp = tempfile::tempdir().unwrap();
    let info_path = tmp.path().join("video.info.json");
    std::fs::write(&info_path, r#"{"title":"Test","duration":120}"#).unwrap();
    let info = extract_info(tmp.path(), "test").unwrap();
    assert_eq!(info.title, "Test");
}
```

| Test Case | Expected |
|-----------|----------|
| Valid info.json | All fields parsed |
| Missing optional fields | Defaults applied |
| Description >500 chars | Truncated with "…" |
| Duration as i64 | Parsed correctly |
| Empty info.json | Default VideoInfo |
| Non-JSON file | Error returned |

**resolve_local tests:**
| Test Case | Expected |
|-----------|----------|
| Valid .mp4 | Path returned |
| Unsupported .xyz | Error |
| No extension | Error |
| Non-existent path | Error |
| Symlink to valid | Path returned |

**Est. LOC:** ~120, **Tests:** ~11

### T2.2: `transcript.rs` — Subtitle Parsing

```rust
// Pattern: test JSON3 parsing with word-level timing
#[test]
fn parse_json3_word_timing() {
    let json = r#"{"events":[{"tStartMs":0,"dDurationMs":2000,"segs":[{"utf8":"Hello","tOffsetMs":0,"acAsrConf":95}]}]}"#;
    let segs = parse_json3(json).unwrap();
    assert_eq!(segs[0].words.as_ref().unwrap()[0].confidence, Some(95));
}
```

**parse_json3 tests:**
| Test Case | Expected |
|-----------|----------|
| Word-level timing | Words with tOffsetMs parsed |
| Events with no `segs` | Skipped |
| Missing `tStartMs` | Default 0 |
| Whitespace-only text | Filtered out |
| Large file (1000+ events) | No panic |

**parse_vtt tests:**
| Test Case | Expected |
|-----------|----------|
| Comma separator | `00:00:01,500` parsed |
| No WEBVTT header | Still parses |
| Empty cue blocks | Skipped |
| Timestamp-only lines | No crash |

**filter_by_range tests:**
| Test Case | Expected |
|-----------|----------|
| Overlapping segments | Filtered correctly |
| Exact boundary match | Included |
| Both None bounds | All segments returned |
| No segments match | Empty result |
| Single match | Returned |

**parse_subtitle_file dispatch:**
| Test Case | Expected |
|-----------|----------|
| .json3 extension | JSON3 parser used |
| .vtt extension | VTT parser used |
| .srt extension | Error (unsupported) |

**Est. LOC:** ~150, **Tests:** ~14

### T2.3: `cache.rs` — Cache Operations

```rust
// Pattern: tempdir + roundtrip + edge cases
#[test]
fn store_get_video_roundtrip() {
    let tmp = tempfile::tempdir().unwrap();
    let cache = VideoCache::with_dir(tmp.path().to_path_buf()).unwrap();
    let url = "https://youtube.com/watch?v=test123";
    // Create dummy video file
    let key = VideoCache::cache_key(url);
    let cache_dir = tmp.path().join(&key);
    std::fs::create_dir_all(&cache_dir).unwrap();
    std::fs::write(cache_dir.join("video.mp4"), b"fake video").unwrap();
    // Store and retrieve
    cache.store_video(url, &cache_dir.join("video.mp4")).unwrap();
    assert!(cache.has_video(url));
    assert!(cache.get_video(url).is_some());
}
```

**store_video/get_video tests:**
| Test Case | Expected |
|-----------|----------|
| Round-trip | Store then retrieve |
| No video cached | None returned |
| File missing after store | Graceful handling |
| Different URLs | Different cache entries |

**store_subtitles/get_subtitles tests:**
| Test Case | Expected |
|-----------|----------|
| Round-trip with "en" | Stored and retrieved |
| Language matching "en" → "en-US" | Match found |
| `-orig` suffix preference | Preferred |
| No subtitles cached | None |
| Multiple languages | Retrieve specific |

**save_manifest persistence:**
| Test Case | Expected |
|-----------|----------|
| Save → reopen | Data persists |
| Corrupt manifest | Graceful recovery |

**evict LRU:**
| Test Case | Expected |
|-----------|----------|
| Over limit | Oldest evicted |
| Under limit | No eviction |
| Access updates | Recency updated |

**Est. LOC:** ~150, **Tests:** ~11

### T2.4: `scene_detect.rs` — Scene Scores JSON

| Test Case | Expected |
|-----------|----------|
| Valid boundaries + frames | JSON written correctly |
| Empty boundaries | Empty scenes array |
| Write permission error | Error returned |

**Est. LOC:** ~40, **Tests:** ~3

**Total T2: ~460 LOC, ~39 tests**

---

## Phase T3: Mocked Integration Tests

**Strategy:** Mock external services (HTTP, subprocesses) for deterministic tests.
**Pattern:** wiremock for HTTP, mockall for subprocess trait.
**Reference:** mockall docs — `#[automock]` trait, predicate matching

### T3.1: New Abstraction Layer — `CommandRunner` Trait

```rust
// src/command.rs (new file)
use mockall::automock;

#[automock]
pub trait CommandRunner {
    fn execute(&self, cmd: &str, args: &[&str]) -> Result<CommandOutput, std::io::Error>;
}

pub struct CommandOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub exit_code: i32,
}

pub struct RealCommandRunner;

impl CommandRunner for RealCommandRunner {
    fn execute(&self, cmd: &str, args: &[&str]) -> Result<CommandOutput, std::io::Error> {
        let output = std::process::Command::new(cmd)
            .args(args)
            .output()?;
        Ok(CommandOutput {
            stdout: output.stdout,
            stderr: output.stderr,
            exit_code: output.status.code().unwrap_or(-1),
        })
    }
}
```

### T3.2: `whisper.rs` — API Integration (wiremock)

```rust
// Pattern from wiremock docs: mock HTTP server
use wiremock::{MockServer, Mock, ResponseTemplate};
use wiremock::matchers::{method, path, header};

#[tokio::test]
async fn transcribe_success() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/audio/transcriptions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "segments": [{"start": 0.0, "end": 2.0, "text": "Hello"}]
        })))
        .mount(&mock_server)
        .await;

    let provider = OpenAIProvider { endpoint: mock_server.uri() };
    let result = provider.transcribe(&audio_path, "test-key").await;
    assert!(result.is_ok());
}
```

| Test Case | Expected |
|-----------|----------|
| Successful transcription | Segments returned |
| Rate limit (429) + Retry-After | Retry succeeds |
| Rate limit exhausted | Error after 4 retries |
| Non-success (500) | Error returned |
| extract_audio success | audio.mp3 created |
| extract_audio ffmpeg not found | Error |
| parse_response empty segments | Empty vec |
| parse_response missing fields | Graceful handling |

**Est. LOC:** ~150, **Tests:** ~8

### T3.3: `pipeline.rs` — Orchestration (mockall)

| Test Case | Expected |
|-----------|----------|
| build_report long video (>600s) | Warning included |
| build_report empty transcript | Warning included |
| build_report fallback engine | Warning included |
| ensure_resources cache hit | No download |
| ensure_resources cache miss | Download + store |
| ensure_resources download fail | Retry × 3 |

**Est. LOC:** ~120, **Tests:** ~6

### T3.4: `scene_detect.rs` — With Test Fixture

| Test Case | Expected |
|-----------|----------|
| is_available true | Detection runs |
| is_available false | Graceful fallback |
| Small .mp4 fixture | Scenes detected |

**Est. LOC:** ~60, **Tests:** ~3

### T3.5: Test Infrastructure

- Create `tests/common/mod.rs` with shared `TestContext` helper
- Create `tests/fixtures/` with small test files
- Gate binary-dependent tests with `#[cfg(feature = "integration_tests")]`

**Est. LOC:** ~50, **Tests:** —

**Total T3: ~380 LOC, ~17 tests**

---

## Phase T4: Edge Cases & Regression

### T4.1: Unicode & Encoding
- CJK transcript segments (Japanese, Chinese, Korean)
- Emoji in video titles
- BOM in subtitle files
- Very long Unicode text (>10k chars)

### T4.2: Large Input Handling
- >10,000 transcript lines
- >100 frame timestamps
- Cache with 1000+ entries

### T4.3: Concurrency
- Cache access from multiple threads
- Race condition in manifest save

### T4.4: Timeout & Network
- reqwest timeout handling
- ffmpeg subprocess timeout

**Total T4: ~140 LOC, ~11 tests**

---

## Phase T5: CI/CD & Tooling

### T5.1: Coverage — cargo-tarpaulin

**Install:**
```bash
cargo install --locked cargo-tarpaulin
```

**Cargo.toml config:**
```toml
[package.metadata.tarpaulin]
exclude-files = ["tests/*", "examples/*", "src/main.rs"]
include-files = ["src/**/*.rs"]
out = ["html", "lcov"]
fail-under = 80
```

**Run command:**
```bash
cargo tarpaulin \
    --all-features \
    --engine llvm \
    --out xml \
    --fail-under 80 \
    --exclude-files "tests/integration/*.rs" \
    --timeout 120
```

**Key flags:**
- `--engine llvm` — More accurate than ptrace on Linux
- `--fail-under 80` — Enforce minimum coverage gate
- `--exclude-files` — Skip tests requiring real binaries
- `--timeout 120` — Video processing tests can be slow

### T5.2: GitHub Actions CI

**File:** `.github/workflows/ci.yml`

```yaml
name: CI

on:
  push:
    branches: [master]
  pull_request:
    branches: [master]

env:
  CARGO_TERM_COLOR: always

jobs:
  fmt:
    name: Format
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions-rust-lang/setup-rust-toolchain@v1
        with:
          components: rustfmt
      - uses: actions-rust-lang/rustfmt@v1

  clippy:
    name: Clippy
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions-rust-lang/setup-rust-toolchain@v1
        with:
          components: clippy
      - run: cargo clippy --all-targets --all-features -- -D clippy::all -D clippy::pedantic

  test:
    name: Test
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions-rust-lang/setup-rust-toolchain@v1
      - name: Install ffmpeg
        run: sudo apt-get update && sudo apt-get install -y ffmpeg
      - run: cargo test --all-features

  coverage:
    name: Coverage
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions-rust-lang/setup-rust-toolchain@v1
      - name: Install ffmpeg
        run: sudo apt-get update && sudo apt-get install -y ffmpeg
      - name: Install tarpaulin
        run: cargo install --locked cargo-tarpaulin
      - name: Run coverage
        run: cargo tarpaulin --all-features --engine llvm --out xml --fail-under 80
```

### T5.3: Clippy Strict Mode

**Cargo.toml `[lints.clippy]`:**
```toml
[lints.clippy]
cast_possible_truncation = "deny"
cast_sign_loss = "deny"
unwrap_used = "warn"
expect_used = "warn"
panic = "deny"
module_name_repetitions = "allow"
must_use_candidate = "allow"
missing_errors_doc = "allow"
```

**clippy.toml:**
```toml
avoid-breaking-exported-api = false
check-inconsistent-struct-field-initializers = true
lint-commented-code = true
```

### T5.4: Format Check
- `cargo fmt --check` in CI
- Auto-format on commit (optional)

---

## Summary

| Phase | New Tests | New LOC | New Dev Deps | Dependencies |
|-------|-----------|---------|--------------|--------------|
| T0: Cleanup | — | -15 | — | None |
| T1: Pure logic | ~60 | ~550 | serde_test | T0 |
| T2: Filesystem | ~39 | ~460 | — | T0 |
| T3: Mocked | ~17 | ~380 | wiremock, mockall | T0+T1+T2 |
| T4: Edge cases | ~11 | ~140 | — | T1+T2 |
| T5: CI/CD | — | Config | cargo-tarpaulin | All above |
| **TOTAL** | **~127** | **~1,515** | | |

**Progression:**
- T0 → clean codebase, remove dead weight
- T1 → fast feedback loop (pure functions, no I/O)
- T2 → filesystem operations with temp isolation
- T3 → external service mocking (wiremock + mockall)
- T4 → adversarial input testing
- T5 → automated quality gates

**Current:** 123 tests → **Target:** ~250 tests
**Current coverage:** ~30% logic paths → **Target:** ~80%

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| av-scenechange needs real video files | Gate with `#[cfg(feature = "integration_tests")]`, use tiny fixtures |
| wiremock async complexity | Use `#[tokio::test]` macro, mockall for simpler cases |
| tempfile cleanup race conditions | Drop file handles before TempDir scope end |
| Clippy pedantic too noisy | Start with `clippy::all`, add `pedantic` incrementally |
| Coverage false positives | Exclude `src/main.rs` and `tests/*` from tarpaulin |
