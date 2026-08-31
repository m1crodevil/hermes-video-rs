# YouTube access

`watch2` delegates YouTube access to `yt-dlp`. It does not generate Proof-of-Origin (PO) tokens.

## Why HTTP 403 happens

YouTube can allow metadata and captions while denying the Google Video Server stream. When yt-dlp needs GVS attestation, install a PO-token provider. The recommended client is `mweb`.

## Local BgUtils provider

Install the yt-dlp plugin into the same Python environment as `yt-dlp`:

```bash
python -m pip install -U bgutil-ytdlp-pot-provider
```

Run its HTTP provider locally. Docker keeps the JavaScript component outside the Rust project:

```bash
docker run -d --name watch2-bgutil-provider --restart unless-stopped \
  -p 127.0.0.1:4416:4416 \
  brainicism/bgutil-ytdlp-pot-provider:deno
```

Verify discovery and a real stream—not `--simulate` alone:

```bash
yt-dlp -v URL 2>&1 | grep 'PO Token Providers:'
RUN_YOUTUBE_SMOKE=1 YOUTUBE_SMOKE_URL=URL make youtube-smoke
```

The first command must list `bgutil:http`; the smoke target downloads a 144p stream, verifies it with `ffprobe`, and removes its temporary files.

## Cookie fallback

A Netscape-format `cookies.txt` can help with session-restricted content but does not guarantee a PO-token bypass.

```bash
chmod 600 youtube-cookies.txt
watch2 URL --cookies-file youtube-cookies.txt --out-dir /tmp/watch
```

Use a dedicated/incognito YouTube session. Never commit cookie files; `watch2` never writes them to reports.

## Failure semantics

- Default stream 403: `watch2` fails immediately.
- `--allow-transcript-only`: writes captions-only evidence with `visual_verification=false`.
- Visual conclusions require `report.json.analysis_capabilities.visual_verification=true` and review of every extracted frame.
