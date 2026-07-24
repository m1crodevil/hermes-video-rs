# Configuration

## Configuration

Config file: `~/.config/watch/.env`
```
GROQ_API_KEY=gsk_...        # Required for Whisper fallback only
OPENAI_API_KEY=sk-...        # Alternative Whisper provider
SETUP_COMPLETE=true
```

### API Key (Optional — Whisper Only)

API keys are only needed for Whisper audio transcription (when subtitles are unavailable).

- **With API key**: Whisper fallback available for videos without subtitles
- **Without API key**: Only works with videos that have auto/manual captions
- **`--no-whisper`**: Suppresses the "no API key" warning, skips Whisper entirely

When no subtitles are found and no API key is set, watch2 stops with an error:
```
No transcript available. Set GROQ_API_KEY or OPENAI_API_KEY for Whisper transcription.
```

## YouTube 2026 Support

Auto-detects and uses:
- **deno** — JS runtime for YouTube challenge solving
- **curl_cffi** — Browser impersonation (anti-bot)
- **Chrome cookies** — Authenticated sessions (opt-in via --cookies, breaks android_vr)

No manual flags needed — just ensure deps are installed.

