# Stats Collection

## Stats Collection (Optional)

Stats are useful for debugging or when the user asks about processing time. By default, do NOT include stats in the output.

**When stats are needed:** User asks "how long did it take?", "how many frames?", or similar.

**How to get stats from report.json:**

```bash
# Quick metadata
rtk jq '{title, uploader, language, engine}' /tmp/watch-XXX/report.json

# Frame count
rtk jq '.frames | length' /tmp/watch-XXX/report.json

# Transcript segments count
rtk jq '.transcript | length' /tmp/watch-XXX/report.json

# Key moments count
rtk jq '.key_moments | length' /tmp/watch-XXX/report.json

# Duration (if available)
rtk jq '.duration_seconds' /tmp/watch-XXX/report.json
```

**Fallback when report.json missing:**
```bash
# Get duration
ffprobe -v quiet -show_entries format=duration \
  -of default=noprint_wrappers=1:nokey=1 /tmp/watch-XXX/download/video.mp4

# Count frames
ls /tmp/watch-XXX/frames/*.jpg 2>/dev/null | wc -l

# Check transcript
ls /tmp/watch-XXX/download/*.json3 2>/dev/null
```

