# watch2

Rust evidence extraction for video-analysis agents. `watch2` downloads media through `yt-dlp`, parses captions, detects scenes, and extracts only the frames requested by the agent. It does not summarize or inspect frames itself.

## Install

```bash
git clone https://github.com/m1crodevil/hermes-video-rs
cd hermes-video-rs
make install
```

Requires `yt-dlp`, `ffmpeg`, `ffprobe`, `av-scenechange`, and `jq` on `PATH`.

## Two-pass workflow

```bash
# Pass 1: captions + scene evidence; report.json is always written.
watch2 "https://youtu.be/VIDEO" --out-dir /tmp/watch --output json

# The agent reads report.json, selects timestamps, then extracts frames.
watch2 "https://youtu.be/VIDEO" \
  --out-dir /tmp/watch --keep-video \
  --timestamps "00:30,01:15,02:45" --output json
```

An agent may perform visual analysis only when:

```bash
jq '.analysis_capabilities.visual_verification' /tmp/watch/report.json
# true
```

It must inspect every extracted frame. `false` means no visual claims are supported.

## YouTube 403

YouTube may deny video-stream requests while allowing metadata and captions. `watch2` fails immediately on this deterministic error rather than retrying it.

```bash
# Explicitly produce a captions-only report; visual_verification remains false.
watch2 "URL" --allow-transcript-only --out-dir /tmp/watch --output json
```

Current yt-dlp guidance is to use a PO-token provider for affected video streams. `watch2` intentionally does not implement token generation; install and configure a supported yt-dlp provider in the yt-dlp environment. A cookie file is an alternative:

```bash
chmod 600 youtube-cookies.txt
watch2 "URL" --cookies-file youtube-cookies.txt --out-dir /tmp/watch
```

Cookie files are never copied into reports or caches.

## Development

```bash
make check
```

MIT License.
