# API Key Optional Fix (v4.4.1)

> **v5.0.0 refactor (Jul 2026):** This fix is still valid and applied.
> Pipeline simplified but API key optional behavior preserved.

## Problem

watch2 hard-required GROQ_API_KEY or OPENAI_API_KEY at startup, exiting with code 3 without it. The `--no-whisper` flag existed but was never checked before the exit.
```rust
can_proceed: missing.is_empty() && has_key,  // ← has_key was required
```

**`main.rs` lines 18-29:**
```rust
if !setup_status.can_proceed {
    // ... prints warnings ...
    std::process::exit(3);  // ← exits before --no-whisper is checked
}
```

## Fix Applied

### setup.rs
```rust
// Before:
can_proceed: missing.is_empty() && has_key,

// After:
can_proceed: missing.is_empty(),
```

### main.rs
```rust
// Before:
if !setup_status.can_proceed {
    // ... all warnings ...
    std::process::exit(3);
}

// After:
if !setup_status.can_proceed {
    if !setup_status.missing_binaries.is_empty() {
        // ... missing binaries error ...
        std::process::exit(3);
    }
}
// API key check — warning only, not a blocker
if !setup_status.has_api_key && !cli.no_whisper {
    eprintln!("⚠️  No Whisper API key...");
    eprintln!("   Use --no-whisper to suppress this warning");
}
```

### pipeline.rs (Step 5b)
```rust
// New: explain when no transcript and no whisper
if transcript_segments.is_empty() {
    if cli.no_whisper {
        eprintln!("⚠️  No subtitles found for this video.");
        eprintln!("   --no-whisper was set, so Whisper fallback was skipped.");
    } else if !config.has_whisper_key() {
        eprintln!("⚠️  No subtitles found for this video.");
        eprintln!("   Whisper API key required for transcription.");
        eprintln!("   Set GROQ_API_KEY or OPENAI_API_KEY in ~/.config/watch/.env");
        eprintln!("   Or use --no-whisper to skip (no transcript available)");
    }
}
```

## Tests

6 tests in `tests/test_setup.rs`:
- `test_can_proceed_without_api_key_when_binaries_exist`
- `test_blocks_when_binaries_missing`
- `test_has_api_key_false_when_no_key`
- `test_first_run_detected`
- `test_integration_can_proceed_independent_of_api_key`
- `test_integration_blocks_without_binaries`

All 180 tests pass after fix.

## Rebuild

```bash
cd ~/hermes-video-rs && cargo build --release
sudo cp target/release/watch2 /usr/local/bin/
```
