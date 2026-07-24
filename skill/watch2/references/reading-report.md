# Reading Report

## Reading report.json

`report.json` contains all structured data. Use `jq` for extraction:

```bash
# Quick metadata
rtk jq '{title, uploader, language, engine}' report.json

# Frame list with timestamps
rtk jq '.frames[] | {path, timestamp, reason}' report.json

# Transcript with timestamps
rtk jq -r '.transcript[] | "[\(.start) → \(.end)] \(.text)"' report.json

# Key moments (if available)
rtk jq '.key_moments[] | {timestamp, reason}' report.json
```

**Avoid**: `cat report.json | python3 -c "..."` — violates Rust-only rule. Use `jq` instead.

