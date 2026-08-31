#!/usr/bin/env bash
set -euo pipefail

: "${RUN_YOUTUBE_SMOKE:=}"
: "${YOUTUBE_SMOKE_URL:=}"

if [[ "$RUN_YOUTUBE_SMOKE" != 1 || -z "$YOUTUBE_SMOKE_URL" ]]; then
  printf '%s\n' 'Set RUN_YOUTUBE_SMOKE=1 and YOUTUBE_SMOKE_URL=https://youtu.be/VIDEO_ID.' >&2
  exit 2
fi

workdir=$(mktemp -d)
trap 'rm -rf "$workdir"' EXIT

provider=$(yt-dlp -v --simulate "$YOUTUBE_SMOKE_URL" 2>&1 | grep 'PO Token Providers:' || true)
if [[ -z "$provider" || "$provider" == *'Providers: none'* ]]; then
  printf '%s\n' 'No yt-dlp PO-token provider detected; refusing live YouTube smoke test.' >&2
  exit 3
fi

yt-dlp -f 'bv*[height<=144]+ba/b[height<=144]' -o "$workdir/probe.%(ext)s" "$YOUTUBE_SMOKE_URL"
ffprobe -v error -show_entries format=duration -of default=nw=1:nk=1 "$workdir"/probe.* >/dev/null
printf '%s\n' 'YouTube stream smoke test passed.'
