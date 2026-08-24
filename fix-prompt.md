# GUI Fix Prompt — TECHNEO 303 (vibe303)

## Context

App: TECHNEO 303 — synth/sequencer plugin UI. Window: dark theme, fixed size ~2560x1504 (full capture). Three states: FUN stopped (latest.png), FUN running (running.png), ACCURATE stopped with overlay (accurate.png). Layout map (top to bottom):

1. Title bar (traffic lights + TECHNEO 303)
2. Header: app name, BASS SYNTH/MONO badges, TOPOLOGY (ACCURATE / FUN), QUALITY (LOW / HIGH / ULTRA), ZOOM
3. Classic row: WAVE selector (SAW / SQR / SWP) + 7 knobs (TUNING, CUTOFF, RESONANCE, ENV MOD, DECAY, ACCENT, VOLUME) + ENVELOPES panel (MEG ADSR / VEG ADSR)
4. Mid band: FUN = soup drawer visible; ACCURATE = overlay box in empty band
5. Clipper row: OUTPUT CLIPPER (BYPASS / 2X / 4X + knobs) + ANALOG CHARACTER & TWEAKS (DRIFT 10%, RECAL 100%)
6. ACID EUCLIDEAN GENERATOR (pattern, octave behavior, probabilities, generation)
7. Sequencer 16 steps + TRANSPORT & PATTERN (RUN/STOP, SYNC:HOST, MIDI, BPM, LEN)
8. Bottom band (empty / sequencer steps)

## Issues

### 1. Bottom dead band
**Symptom:** Black void below sequencer panel above rounded window bottom. FUN ~77px real; ACCURATE ~147px. Window height fixed, content shorter.
**Required fix:** Content fills bottom consistently in all states; same right margin as left; no horizontal scroll; bottom void ≤5px padding in FUN and ACCURATE.

### 2. Dead column right of tweaks
**Symptom:** ANALOG CHARACTER & TWEAKS panel ends ~86% width; ~350px empty strip to right margin.
**Required fix:** Panel extends to right window margin with same left/right padding; no floating dead column.

### 3. Accurate overlay band empty
**Symptom:** Overlay box floats centered in full-width empty band; band height exceeds box by wide margins.
**Required fix:** Overlay band height reduced to fit box + 20px vertical padding; no floating void above/below.

### 4. Ragged panel bottoms
**Symptom:** OUTPUT CLIPPER bottom sits ~115px real above ANALOG panel bottom.
**Required fix:** Sibling panels in same row share bottom alignment; no ragged bottom edge.

### 5. Knob arc fill contradicts pointer
**Symptom:** TUNING/CUTOFF/RESONANCE/ENV MOD/DECAY/ACCENT/VOLUME all display 50% but arcs differ (past 12 o'clock ~65% vs stopshort ~45%). Same value = three different fills.
**Required fix:** All 50% knobs show identical arc fill reaching same angle relative to pointer; arc stops at pointer. Check at min/max too.

### 6. QUALITY ghost selection disabled
**Symptom:** ACCURATE dims QUALITY group; LOW keeps visible outline; HIGH/ULTRA flat.
**Required fix:** Disabled selection outline uniform: either all three show same dimmed outline or none; no single ghost highlight.

### 7. DYN/MIX dimmed with LINK checked
**Symptom:** Controls at 0% with LINK checked show uniform dim.
**Required fix:** Confirm link-driven disable; if intentional, keep uniform; if not, restore control brightness independent of LINK.

### 8. PW 50% arc contradiction (souped-up)
**Symptom:** Souped-up PW 50% arc does not match pointer position.
**Required fix:** Same fix as #5: arc stops at pointer.

### 9. STOP button pink/red
**Symptom:** STOP button uses pink-red; palette accent is orange.
**Required fix:** STOP button uses orange accent; no red/pink in neutral/play state.

### 10. OUTPUT CLIPPER title pink/red
**Symptom:** Title text uses same exception color.
**Required fix:** Title uses white/light gray; no red/pink exception.

### 11. Playhead yellow = TIE yellow
**Symptom:** Running playhead same yellow as TIE highlight; ambiguous during playback.
**Required fix:** Playhead uses distinct color (e.g., bright cyan or white) different from TIE yellow.

### 12. Mixed decimal precision
**Symptom:** +0 ct (0 dp) vs +0.10 s (2 dp) vs 2.1 kHz / 200 ms.
**Required fix:** Consistent decimal rule: values <1 use 2 dp; ≥1 integer; units always spaced (e.g., "200 ms" not "200ms").

### 13. MEG ADSR / VEG ADSR naming
**Symptom:** Cryptic pair; no legend in UI.
**Required fix:** Add tooltip or label clarifying MEG = envelope 1, VEG = envelope 2; or rename to clear terms.

### 14. WAVE selector orphan track
**Symptom:** Empty vertical slider-like bar left of SAW/SQR/SWP; looks draggable, no visible effect.
**Required fix:** Remove empty bar or make it functional (e.g., drag to scroll wave options); must have visible effect or no draggable appearance.

### 15. Micro hit targets
**Symptom:** BPM +/- steppers, JIT checkbox, envelope caps, arrow rows very small.
**Required fix:** Hit targets ≥24px height; stepper buttons clearly separated from text.

### 16. Running feedback (positive — preserve)
**Symptom:** Step 04 playhead highlight + STOP label visible; live state clear.
**Required fix:** Preserve; do not break.

## Guardrails

- Presentation layer only. Do not change behavior, ranges, persistence, or audio engine.
- No new features; only fix visual defects and formatting.
- Do not alter layout geometry beyond removing dead space and aligning siblings.
- Preserve running/accurate state differences (overlay, dimming, playhead) exactly.

## Verification

Re-render all three states (FUN stopped = latest, FUN running = running, ACCURATE stopped = accurate). Confirm per issue:

- #1: Bottom void ≤5px in all states; same height relative to content.
- #2: No empty column >20px right of tweaks.
- #3: Overlay centered vertically within reduced band; no floating void >30px.
- #4: Clipper row bottom aligns with analog panel bottom.
- #5/#8: All 50% knobs show same arc fill angle.
- #6: Disabled QUALITY selection uniform (all same outline or none).
- #7: DYN/MIX brightness consistent with link state (flag only if changed).
- #9: STOP button orange, not red.
- #10: OUTPUT CLIPPER title no red.
- #11: Playhead distinct from TIE.
- #12: Decimal spacing consistent.
- #14: Orphan bar removed or functional.
- #15: Hit targets enlarged.
- #16: Running state unchanged.

Confirmation list: 1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16.
