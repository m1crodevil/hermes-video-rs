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
yt-dlp --list-subs "$SOURCE" 2>&1 | head -30 || true

# 3. Download subtitles (try multiple strategies)
echo ""
echo "--- Downloading Subtitles ---"
yt-dlp --write-sub --write-auto-sub --sub-lang "all" \
  --sub-format json3 --skip-download \
  -o "$WORKDIR/download/video" "$SOURCE" 2>&1 || true

# Check what we got
echo "Downloaded subtitle files:"
ls -la "$WORKDIR/download/"*.json3 2>/dev/null || echo "No subtitle files found"

# 4. Parse transcript from JSON3 (using jq — no Python)
echo ""
echo "--- Parsing Transcript ---"
JSON3_FILE=$(ls "$WORKDIR/download/"*orig*.json3 2>/dev/null | head -1 || \
             ls "$WORKDIR/download/"*.json3 2>/dev/null | head -1 || echo "")

if [ -z "$JSON3_FILE" ]; then
  echo "WARNING: No JSON3 subtitle files found"
  echo "Trying English auto-captions..."
  JSON3_FILE=$(ls "$WORKDIR/download/"*.en.*.json3 2>/dev/null | head -1 || echo "")
fi

if [ -z "$JSON3_FILE" ]; then
  echo "ERROR: No usable subtitle files found"
  exit 1
fi

echo "Using: $JSON3_FILE"

# Parse JSON3 with jq — extract timestamps and text
jq -r '.events[] | 
  select(.segs != null) | 
  (.tStartMs / 1000 | floor) as $t |
  (.segs | map(.utf8 // "") | join("")) as $text |
  select($text != "") |
  "[\($t / 60 | floor | tostring | if length < 2 then "0" + . else . end):\($t % 60 | floor | tostring | if length < 2 then "0" + . else . end)] \($text)"
' "$JSON3_FILE" > "$WORKDIR/transcript.txt"

LINE_COUNT=$(wc -l < "$WORKDIR/transcript.txt")
echo "Transcript: $LINE_COUNT lines → $WORKDIR/transcript.txt"

# 5. Download video (720p max)
echo ""
echo "--- Downloading Video ---"
yt-dlp -f "bv*[height<=720]+ba/b[height<=720]/bv+ba/b" \
  --merge-output-format mp4 \
  -o "$WORKDIR/download/video.mp4" "$SOURCE"

# 6. Extract frames — scene detection first, fall back to uniform
#    CRITICAL: Minimum 15 frames required for adequate visual coverage (scale with duration)
echo ""
echo "--- Extracting Frames ---"
mkdir -p "$WORKDIR/frames"
MIN_FRAMES=15

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
| Transcript parsing | Automatic | Manual (jq) |
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

Subtitle language is auto-detected from video metadata. The binary downloads only matching subtitles to avoid rate-limiting.

1. **Manual subs** in detected language: Best quality, may not exist
2. **Auto-generated subs** in detected language: Usually available
3. **Manual English**: Always available, may lose nuance for non-English content
4. **Auto English**: Fallback when no other subs found

The `--sub-lang` flag accepts language codes (e.g., `en`, `id`, `ja`, `es`). When no language is detected, all available subs are downloaded.
