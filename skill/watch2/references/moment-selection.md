# Moment Selection

## LLM Features (Agent-Side)

**No direct LLM calls from binary.** All intelligence is handled by the agent.

### Moment Selection (MANDATORY)

Agent selects key moments using transcript + scene data from report.json:

**Data sources for moment selection:**
- **JSON3 transcript**: Word-level timing + confidence scores
  - Low confidence words (< 0.5) = potential misspellings
  - Word.start timestamps = precise moment timing
- **scene_boundaries**: Av-scenechange detection data
  - High inter_cost (> 30) = major visual shifts
  - Scene transitions = topic changes or visual context

**Moment selection criteria:**
1. Proper nouns (names, brands, titles)
2. Claims/statistics (numbers, prices, dates)
3. Deictic references ("this", "that", "look at this")
4. Topic transitions (use scene_boundaries)
5. Key arguments (conclusions, controversial statements)
6. Visual context (moments where visuals change interpretation)
7. Speaker identity (speaker changes, identity unclear from transcript)
8. Entity recognition (brand names, product names, on-screen text)

**Output**: 21-25 timestamps as comma-separated string

### Language Detection

Agent detects language via LLM from transcript (ISO 639-1 code).

### Analysis

Agent generates comprehensive analysis combining:
- Transcript insights (JSON3 word-level data)
- Scene context (av-scenechange boundaries)
- Visual evidence (vision_analyze results)

**Whisper fallback** — Binary calls Groq/OpenAI API only for audio transcription when subtitles unavailable. Requires `GROQ_API_KEY` or `OPENAI_API_KEY` in `~/.config/watch/.env`.

