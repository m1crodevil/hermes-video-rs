# Manual Fallback Pipeline

When watch2 fails (exit code 3 from missing API keys, or other issues), use this manual pipeline to achieve equivalent results.

## Complete Script

```bash
#!/bin/bash
# Manual video analysis fallback — bypasses watch2 entirely
# Usage: ./manual-fallback.sh "https://youtu.be/VIDEO_ID"

set -euo pipefail

SOURCE="$1"
WORKDIR="/tmp/watch-$(date +%s)"
mkdir -p "$WORKDIR/download" "$WORKDIR/frames"

echo "=== Manual Fallback Pipeline ==="
echo "Source: $SOURCE"
echo "Workdir: $WORKDIR"

# 1. Get video info
echo ""
echo "--- Video Info ---"
TITLE=$(yt-dlp --print "%(title)s" "$SOURCE" 2>/dev/null)
DURATION=$(yt-dlp --print "%(duration)s" "$SOURCE" 2>/dev/null)
echo "Title: $TITLE"
echo "Duration: ${DURATION}s"

# 2. List available subtitles
echo ""
echo "--- Available Subtitles ---"
yt-dlp --list-subs "$SOURCE" 2>&1 | grep -E "^(en|id|English|Indonesian)" || true

# 3. Download subtitles (try multiple strategies)
echo ""
echo "--- Downloading Subtitles ---"
yt-dlp --write-sub --write-auto-sub --sub-lang "id-orig,id,en" \
  --sub-format json3 --skip-download \
  -o "$WORKDIR/download/video" "$SOURCE" 2>&1 || true

# Check what we got
echo "Downloaded subtitle files:"
ls -la "$WORKDIR/download/"*.json3 2>/dev/null || echo "No subtitle files found"

# 4. Parse transcript from JSON3
echo ""
echo "--- Parsing Transcript ---"
python3 << PYEOF
import json, glob, os
outdir = "$WORKDIR"
files = sorted(glob.glob(f"{outdir}/download/video.*.json3"))
# Prefer *-orig.json3 (manual/accurate), then any available
json3 = next((f for f in files if "orig" in f), files[0] if files else None)
if not json3:
    print("WARNING: No JSON3 subtitle files found")
    print("Falling back to English auto-captions...")
    # Try English as last resort
    json3 = next((f for f in files if ".en." in f), None)
if not json3:
    raise SystemExit("No usable subtitle files found")
print(f"Using: {json3}")
with open(json3) as f:
    data = json.load(f)
lines = []
for event in data.get("events", []):
    if "segs" in event:
        text = "".join(s.get("utf8","") for s in event["segs"] if "utf8" in s).strip()
        if text:
            t = event["tStartMs"] // 1000
            lines.append(f"[{t//60:02d}:{t%60:02d}] {text}")
outpath = f"{outdir}/transcript.txt"
with open(outpath, "w") as f:
    f.write("\n".join(lines))
print(f"Transcript: {len(lines)} lines → {outpath}")
PYEOF

# 5. Download video (720p max)
echo ""
echo "--- Downloading Video ---"
yt-dlp -f "bv*[height<=720]+ba/b[height<=720]/bv+ba/b" \
  --merge-output-format mp4 \
  -o "$WORKDIR/download/video.mp4" "$SOURCE"

# 6. Extract frames — scene detection first, fall back to uniform
#    CRITICAL: Minimum 21 frames required for adequate visual coverage
echo ""
echo "--- Extracting Frames ---"
mkdir -p "$WORKDIR/frames"
MIN_FRAMES=21

ffmpeg -i "$WORKDIR/download/video.mp4" \
  -vf "select='gt(scene,0.25)',scale=512:-1" \
  -vsync vfr -q:v 3 "$WORKDIR/frames/frame_%04d.jpg" -y 2>/dev/null

FRAME_COUNT=$(ls "$WORKDIR/frames/"*.jpg 2>/dev/null | wc -l)
echo "Scene detection: $FRAME_COUNT frames"

if [ "$FRAME_COUNT" -lt "$MIN_FRAMES" ]; then
  rm -f "$WORKDIR/frames/"*.jpg
  DURATION=$(ffprobe -v quiet -show_entries format=duration -of default=noprint_wrappers=1:nokey=1 "$WORKDIR/download/video.mp4" 2>/dev/null || echo "300")
  FPS=$(echo "scale=2; $MIN_FRAMES / $DURATION" | bc 2>/dev/null || echo "0.1")
  # Clamp fps: min 0.1 (1/10s), max 2.0
  if [ "$(echo "$FPS < 0.1" | bc 2>/dev/null)" = "1" ]; then FPS="0.1"; fi
  if [ "$(echo "$FPS > 2.0" | bc 2>/dev/null)" = "1" ]; then FPS="2.0"; fi
  echo "Too few scene changes — switching to uniform (${FPS} fps)"
  ffmpeg -i "$WORKDIR/download/video.mp4" \
    -vf "fps=${FPS},scale=512:-1" \
    -vsync vfr -q:v 3 "$WORKDIR/frames/frame_%04d.jpg" -y 2>/dev/null
  FRAME_COUNT=$(ls "$WORKDIR/frames/"*.jpg 2>/dev/null | wc -l)
  echo "Uniform extraction: $FRAME_COUNT frames"
fi

# ENFORCEMENT: Still below minimum? Report clearly.
if [ "$FRAME_COUNT" -lt "$MIN_FRAMES" ]; then
  echo "⚠️  WARNING: Only $FRAME_COUNT frames (minimum: $MIN_FRAMES)"
  echo "    Use --timestamps flag or manual ffmpeg at specific timestamps"
fi

# 7. Clean up video file (no longer needed)
rm -f "$WORKDIR/download/video.mp4"
echo "Video file cleaned up"

# 8. Summary
echo ""
echo "=== Pipeline Complete ==="
echo "Transcript: $WORKDIR/transcript.txt"
echo "Frames: $WORKDIR/frames/"
echo "Frame count: $FRAME_COUNT"
echo ""
echo "Next steps:"
echo "1. Read transcript.txt to understand content"
echo "2. Use search_files to find key moments"
echo "3. Analyze representative frames with vision_analyze"
```

## Key Differences from watch2

| Feature | watch2 | Manual Pipeline |
|---------|--------|-----------------|
| API keys required | Yes (GROQ/OPENAI) | No |
| Transcript parsing | Automatic | Manual (JSON3) |
| Frame extraction | Automatic | Manual (ffmpeg) |
| Moment detection | LLM-driven | Agent-driven |
| `moments_prompt.txt` | Generated | Not generated |
| `key_moments.json` | Generated | Not generated |
| Vision analysis | Agent-driven | Agent-driven |

## When to Use

- watch2 fails with exit code 3 (missing API keys)
- watch2 fails for any other reason
- You want more control over the pipeline
- You need to analyze a video quickly without setting up API keys

## Subtitle Strategy Priority

1. **Manual Indonesian** (`id-orig`): Best quality, but may not exist
2. **Auto Indonesian** (`id`): Usually available for Indonesian videos
3. **Auto English** (`en`): Always available, but may lose nuance

Always try `--write-sub --write-auto-sub --sub-lang "id-orig,id,en"` to get the best available.
