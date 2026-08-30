# Agent workflow

`watch2` is a two-pass evidence collector.

## Pass 1 — collect evidence

```bash
watch2 "URL_OR_PATH" --out-dir /tmp/watch-XXX --output json
```

Read the durable report:

```bash
jq '{title, uploader, language, duration, scene_count}' /tmp/watch-XXX/report.json
jq -r '.transcript[] | "[\(.start) → \(.end)] \(.text)"' /tmp/watch-XXX/report.json
jq '.scene_boundaries[] | {start_sec, end_sec, duration_sec}' /tmp/watch-XXX/report.json
```

The binary returns metadata, captions/Whisper transcript, and scene boundaries. It does not choose key moments.

## Select timestamps

Choose timestamps that cover:

- opening claim or hook;
- topic changes and demonstrations;
- named entities, numbers, or on-screen text that require visual confirmation;
- conclusion or call to action.

Use scene boundaries to avoid selecting many near-identical frames. Add enough timestamps for the duration; do not impose a fixed frame count when the evidence does not warrant it.

## Pass 2 — extract frames

```bash
watch2 "URL_OR_PATH" \
  --out-dir /tmp/watch-XXX \
  --keep-video \
  --timestamps "00:30,01:15,02:45" \
  --output json
```

`report.json` is refreshed automatically. Verify paths before visual analysis:

```bash
jq '.frames[] | {path, timestamp, reason}' /tmp/watch-XXX/report.json
```

Inspect every extracted image. Combine what is visible with the timestamped transcript. State uncertainty when an assertion cannot be visually verified.
