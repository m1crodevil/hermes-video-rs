# UGC Campaign Video Workflow

> **v5.0.0 refactor (Jul 2026):** watch2 now uses single linear pipeline.
> No mode selection needed — just `watch2 <url>` and it auto-detects.

## Campaign Types
- Higher creative judgment required
- MCP coverage: ~40%

### Type B: Single-niche with provided footage (e.g., GregLav campaign)
- Footage provided via download links
- Simpler content requirements
- MCP coverage: ~55%

## Optimal Workflow

Phase 1: Download (watch2)
  watch2 URL --keep-video --out-dir /tmp/campaign

Phase 2: Analyze (watch2)
  watch2 footage.mp4 --detail transcript-moments --min-moments 50
  Output: key_moments.json, transcript.txt, frames/

Phase 3: Edit (OxiMedia CLI)
  oximedia probe footage.mp4
  oximedia transcode footage.mp4 -o av1.mp4 --codec av1
  oximedia clips create -i av1.mp4 -n hook --tc-in TIME --tc-out TIME --db campaign.json
  oximedia clips merge -c id1,id2,id3 --name final --db campaign.json
  oximedia clips export --db campaign.json --output final.mp4

Phase 4: Post (Manual)
  Post to TikTok/IG/YT, account warmup, submit analytics

## Automation Coverage

| Campaign Type | MCP Coverage | Blockers |
|---------------|--------------|----------|
| Multi-niche | ~40% | Creative decisions |
| Single niche, provided footage | ~55% | Creative decisions, posting |
| Simple trim/repost | ~70% | Posting, quality check |

## Key Insight

MCP tools handle 55-70% of technical workflow. Creative decisions and social media management remain manual.
