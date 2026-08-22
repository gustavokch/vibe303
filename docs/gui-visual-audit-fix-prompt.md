# GUI Visual Audit — Fix Prompt (TECHNEO 303)

You are fixing visual defects in the TECHNEO 303 bass-synth plugin GUI. Work from the audit screenshots in `target/gui-qa/`:

- `accurate.png` — TOPOLOGY=ACCURATE, transport stopped (2560×1504 px capture, @2x; logical window 1280×752)
- `running.png` — TOPOLOGY=FUN, transport running (playhead visible on step 04)
- `latest.png` — TOPOLOGY=FUN, transport stopped

Region map (top to bottom):

1. Title bar (traffic lights + "TECHNEO 303")
2. Header: product name, BASS SYNTH / MONO badges, TOPOLOGY (ACCURATE/FUN), QUALITY (LOW/HIGH/ULTRA), ZOOM
3. Classic row: WAVE selector (SAW/SQR/SWP), knobs TUNING…VOLUME, ENVELOPES panel (MEG ADSR, VEG ADSR, ADV ENV)
4. Mid band: FUN = "SOUPED-UP ENGINE & ANALOG DRAWER" (VCF TAPS, VCA ENV, DRIVE…R-COMP knobs); ACCURATE = centered overlay box ("ACCURATE MODE ACTIVE / Souped-up engine is bypassed / ENABLE FUN MODE")
5. Clipper row: OUTPUT CLIPPER panel (left) + ANALOG CHARACTER & TWEAKS panel (right)
6. ACID EUCLIDEAN GENERATOR panel (sections 1–4, GENERATE / MUTATE)
7. Sequencer row: 16 step columns + TRANSPORT & PATTERN panel
8. Bottom band between sequencer panel and window edge

Issue numbers below are stable. Your confirmation report must use the same numbers.

## Issues

### 1. Bottom dead band, state-dependent height
Symptom: empty black band between the sequencer panel bottom and the window bottom edge; ~77 px (@2x) in FUN states, ~147 px in ACCURATE. The window height is fixed while content height changes with topology.
Required fix: the content must fill the window in every state with a uniform outer margin. Either size the window to content on topology switch, or grow the layout so the bottom gap equals the standard outer margin in all three states.
Acceptance: in re-captured `accurate`, `running`, `latest`, the gap between the sequencer panel and the window content edge equals the left/right outer margin (±2 px) and is identical across states. No visible void in any state.

### 2. Dead column right of ANALOG CHARACTER & TWEAKS
Symptom: the ANALOG panel's right edge stops at ~86% of window width, leaving an empty vertical strip to the right margin in all states.
Required fix: extend the ANALOG panel to the same right edge as the other full-width panels (ACID EUCLIDEAN GENERATOR, header). Redistribute its internal controls (e.g. DRIFT / RECAL knobs) into the reclaimed width; do not leave stretched emptiness inside the panel.
Acceptance: panel right edges are collinear (±2 px) with the GENERATOR panel in all states; no empty column outside or inside the panel.

### 3. ACCURATE overlay band is a full-width void
Symptom: in ACCURATE mode the mid band is a full-width panel containing only a small centered box; the surrounding void is several times the box size.
Required fix: the band height must hug the overlay box with the standard panel margin on all four sides (same margin value used by other panels). The box stays horizontally centered.
Acceptance: `accurate` re-capture shows band height = box height + 2× standard margin (±2 px); no full-width void; issue 1's bottom gap stays uniform after this change.

### 4. Ragged panel bottoms in the clipper row
Symptom: OUTPUT CLIPPER panel bottom edge sits ~115 px (@2x) above the ANALOG CHARACTER & TWEAKS panel bottom edge.
Required fix: panels sharing a row must have equal height with collinear bottom edges.
Acceptance: both panel bottoms collinear (±2 px) in all states.

### 5. Knob arc fill contradicts pointer position at equal values
Symptom: in the classic row all knob pointers sit at 12 o'clock, but arc fills differ: TUNING/CUTOFF arcs end at 12 o'clock (~50%), RESONANCE/ENV MOD/DECAY arcs sweep past it (~65%), ACCENT/VOLUME arcs stop short (~45%) — while displayed values are all 50% (or the centered equivalent).
Required fix: use one arc-mapping function for every rotary control: arc sweep = normalized value × total sweep, arc end exactly at the pointer angle, for all values, in all knob widgets (classic row, souped-up drawer, clipper, tweaks).
Acceptance: at 50% every knob shows an identical sweep ending exactly at the pointer. Verify RESONANCE, ENV MOD, ACCENT, VOLUME, DECAY, PW, GLIDE at 0/25/50/75/100: arc end and pointer coincide at each step.

### 6. QUALITY shows a ghost selection while disabled
Symptom: in ACCURATE mode the QUALITY group is disabled, yet LOW keeps a visible selection outline while HIGH/ULTRA dim flat.
Required fix: a disabled group renders every segment identically dimmed with no selection outline; the outline returns when the group re-enables.
Acceptance: `accurate` re-capture shows LOW/HIGH/ULTRA uniformly dim; `running` still shows LOW with a bright outline.

### 7. DYN/MIX wrongly disabled while LINK is checked
Symptom: DYN and MIX knobs render dim at 0% while LINK is checked; all other clipper knobs are bright. Confirmed not intentional by the product owner.
Required fix: remove the erroneous disable. DYN and MIX must render enabled (bright, draggable) like their sibling clipper knobs, regardless of the LINK checkbox state. Do not change any other LINK effect. This is the one allowed behavior change (see Guardrails).
Acceptance: with LINK checked, re-captures show DYN/MIX at the same brightness as DRIVE…TILT; dragging DYN or MIX changes its readout; toggling LINK never dims them.

### 8. Souped-up drawer arcs vs pointers (same root cause as 5)
Symptom: PW at 50% shows an arc ending near 11 o'clock while its pointer sits at 12 o'clock; GLIDE/PITCH1/PITCH2 arcs need the same check.
Required fix: covered by the single arc-mapping function from issue 5; verify these knobs explicitly.
Acceptance: PW at 50% has arc end at the pointer; 0/25/50/75/100 sweep check passes for PW, GLIDE, PITCH1, PITCH2, SWEEP, R-COMP, DRIVE, SUB, MIX2.

### 9. STOP button uses pink/red outside the palette
Symptom: while running, the transport button reads STOP on a pink/red fill. The UI accent is orange; cyan and amber are reserved for step semantics; red nowhere else means "active".
Required fix: the running-state transport control must use the established accent (orange fill or an accent-derived "active" treatment, e.g. brighter border + fill). Introduce no new hue for it.
Acceptance: `running` re-capture shows the STOP control in the orange accent family; no pink/red on transport.

### 10. OUTPUT CLIPPER title color breaks the panel-title convention
Symptom: the OUTPUT CLIPPER title renders pink/red while every other panel title (SOUPED-UP ENGINE…, ACID EUCLIDEAN GENERATOR) is orange.
Required fix: panel titles use one color: the standard orange.
Acceptance: all panel titles render the same orange in all states.

### 11. Playhead reuses the TIE color
Symptom: the running playhead highlights step 04 with the same yellow used by TIE buttons, so "currently playing" and "tied" are indistinguishable by hue.
Required fix: give the playhead a distinct treatment that collides with none of NOTE (cyan), TIE (amber), ACC (orange) — e.g. a white/bright outline or a dedicated hue used only for playback position.
Acceptance: `running` re-capture: playhead identifiable at a glance on a TIE step and on a NOTE step; no semantic color reuse.

### 12. Inconsistent decimal precision in readouts
Symptom: `+0 ct` (0 dp) next to `+0.0 dB` and `+0.0 st` (1 dp); `1.10 s` (2 dp); `2.1 kHz`; `200 ms`.
Required fix: adopt one formatting rule and apply it everywhere: signed quantities always show sign + one decimal (`+0.0 ct`); each unit has a fixed decimal count (ms integer, s two decimals, kHz one decimal, % integer).
Acceptance: TUNING reads `+0.0 ct`; no two readouts with the same unit differ in precision.

### 13. Cryptic MEG ADSR / VEG ADSR names (confirm-only)
Symptom: the envelope pair is labeled MEG ADSR and VEG ADSR with no legend.
Required fix: if the names have no documented meaning, rename to self-explanatory labels (e.g. MOD ADSR / AMP ADSR) — text change only. If intentional, add a code comment documenting the meaning and report it.
Acceptance: labels are self-explanatory or documented; confirmation states which.

### 14. Orphan vertical track in the WAVE selector
Symptom: an empty vertical slider-like bar sits left of SAW/SQR/SWP, spanning all three rows; it looks draggable but has no visible function.
Required fix: remove the non-functional track, or make it functional (wave scroll/browse). No decorative element may look interactive.
Acceptance: classic row re-capture shows no control-looking element without a function.

### 15. Micro hit targets
Symptom: BPM `-`/`+` steppers, the JIT checkbox, envelope slider caps, and step up/down arrows have very small interactive areas.
Required fix: enlarge hit areas to ≥ 24×24 logical px using invisible hit padding; visible size may stay compact.
Acceptance: hit-test geometry ≥ 24 px logical for each listed control; visuals unchanged.

## Guardrails

- Presentation layer only. Do not change parameter ranges, defaults, tapers, DSP behavior, MIDI handling, persistence, or host sync. Single exception: issue 7 removes the erroneous DYN/MIX disable — the only allowed behavior change.
- No new dependencies. No refactors outside the touched widgets. No public API renames (display text excepted).
- Prefer growing content into the window over shrinking the window for issue 1; if the host constrains the window size, keep 1280×752 logical in FUN and fill it.
- Do not "improve" adjacent styling that is not listed here.

## Verification

1. Re-run the gui-qa capture harness for the three states (`accurate`, `running`, `latest`) at the same resolution, plus the region crops (`crop_header`, `crop_classic`, `crop_souped`, `crop_clipper_tweaks`, `crop_sequencer`, `crop_bottom_margin`, `crop_right_margin`).
2. Knob sweep check: set each knob named in issues 5/8 to 0/25/50/75/100 and capture; arc end must coincide with the pointer at every step.
3. Per-state confirmation: `accurate` for issues 1, 3, 4, 6, 10, 12, 14; `running` for 1, 2, 4, 9, 10, 11, 12; `latest` for 2, 4, 10, 12, 15; both topologies for 5, 7, 8, 13.
4. Return a confirmation list numbered 1–15; each entry states `fixed` (with the crop path as evidence), `confirmed-intentional` (with code location), or `blocked` (with reason).
