# Moment Selection

## LLM Features (Agent-Side)

**No direct LLM calls from binary.** All intelligence is handled by the agent.

### Moment Selection (MANDATORY)

Agent selects key moments using transcript + scene data from report.json.

**Data sources for moment selection:**
- **JSON3 transcript**: Word-level timing + confidence scores
  - Low confidence words (< 0.5) = potential misspellings
  - Word.start timestamps = precise moment timing
- **scene_boundaries**: Av-scenechange detection data
  - High inter_cost (> 30) = major visual shifts
  - Scene transitions = topic changes or visual context

---

### Moment Selection Criteria (Priority-Ordered)

Selection follows a tiered priority system. **Always fill Tier 1 first**, then Tier 2, then Tier 3. Never pad with low-priority moments to hit a minimum count.

#### Tier 1 — HIGH IMPACT (always select these):

1. **HOOK MOMENTS** — Surprising claims, bold statements, emotional peaks, "did you know" moments. The first 2-3 seconds must grab attention immediately. Look for: surprising statements, bold claims, rhetorical questions, emotional peaks.
2. **KEY ARGUMENTS** — Conclusions, controversial statements, actionable advice. Moments where the speaker makes their core point.
3. **CLAIMS / STATISTICS** — Numbers, prices, dates, percentages, comparisons. These need visual verification and are often mis-transcribed.

#### Tier 2 — MEDIUM IMPACT (select if slots remain):

4. **ENTITY RECOGNITION** — Brand names, product names, on-screen text, proper nouns that ASR might misspell.
5. **TOPIC TRANSITIONS** — Conversation shifts (correlate with scene_boundaries where inter_cost > 30).
6. **VISUAL CONTEXT** — Moments where understanding the visuals changes the interpretation of what's being said.

#### Tier 3 — LOW IMPACT (fill remaining slots):

7. **DEICTIC REFERENCES** — "this", "that", "here", "look at this" where speaker points at something visual.
8. **SPEAKER IDENTITY** — Speaker changes, identity unclear from transcript alone.

---

### Standalone Value Check (MANDATORY for all selections)

Every selected moment MUST make complete sense without surrounding context. **REJECT** moments that:

- Reference earlier content: "as I mentioned earlier", "like we discussed", "going back to what I said"
- Start mid-argument: "and therefore", "so the point is", "that's why"
- Require visual context that cannot be captured in a single frame
- Are ambiguous without the sentence before/after

When a moment fails the standalone check, expand the window ±5s to find the nearest moment that DOES work standalone. If no standalone version exists within ±5s, discard the moment.

---

### Anti-Pattern Filter (REJECT even if they match other criteria):

- **Intros**: "hey guys", "welcome to my channel", "what's up everyone", "hello everyone"
- **Outros**: "don't forget to subscribe", "like and share", "see you next time", "thanks for watching"
- **Filler**: "so anyway", "and then we...", "basically", "you know", "um", "uh"
- **Sponsor reads**: "this video is sponsored by", "use my code", "thanks to our sponsor"
- **Low-density**: repetitive statements, long pauses, tangents with no new information
- **Self-references**: "in my last video", "as I said before", "on my channel"

When a rejected moment is Tier 1 or Tier 2, replace it with the nearest non-rejected moment of the same tier.

---

### Hook Assessment (for each candidate moment)

Evaluate the **FIRST 2-3 SECONDS** of each candidate moment:

| Hook Signal | Impact |
|-------------|--------|
| Starts with a surprising statement | +2 |
| Starts with a question | +1 |
| Starts with a number or statistic | +1 |
| Starts with direct address ("you", "everyone") | +1 |
| Starts with filler ("so", "um", "well") | −2 |

Hook score adjusts the moment's effective priority within its tier. Two Tier 2 moments? Pick the one with the better hook.

---

### Scoring Rubric

Each selected moment receives an **impact_score** (1–10) based on:

| Dimension | Weight | Description |
|-----------|--------|-------------|
| **hook** | 30% | Does the opening grab attention? |
| **value** | 30% | Is the content insightful, surprising, or actionable? |
| **verification** | 25% | Does this moment benefit from visual cross-referencing? |
| **context** | 15% | Does this moment clarify or contradict the transcript? |

**Scoring guide:**
- 9–10: Must-select. High impact on understanding the video.
- 7–8: Strong candidate. Clearly valuable for analysis.
- 5–6: Decent. Fills coverage gaps.
- 3–4: Weak. Only select if you need to hit minimum count.
- 1–2: Skip. Not worth a frame slot.

---

### Output Format

**Quantity scaling** (replaces fixed "minimum 21"):

| Video Duration | Minimum Moments |
|----------------|-----------------|
| < 5 min | 15 |
| 5–20 min | 21 |
| 20+ min | 1 per 30–60s of transcript |

**NEVER pad with low-impact moments to hit minimum.** Better to have 15 high-impact moments than 30 with filler.

**Output**: JSON array of moments (see key_moments.json schema in transcript-moments-pipeline.md)

### Language Detection

Agent detects language via LLM from transcript (ISO 639-1 code).

### Analysis

Agent generates comprehensive analysis combining:
- Transcript insights (JSON3 word-level data)
- Scene context (av-scenechange boundaries)
- Visual evidence (vision_analyze results)

**Whisper fallback** — Binary calls Groq/OpenAI API only for audio transcription when subtitles unavailable. Requires `GROQ_API_KEY` or `OPENAI_API_KEY` in `~/.config/watch/.env`.

