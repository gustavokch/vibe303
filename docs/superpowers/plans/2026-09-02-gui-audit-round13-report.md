# TECHNEO 303 — GUI Visual Audit, Round 13

Date: 2026-09-02
Captures: `target/gui-qa/` (`latest.png`, `fun.png`, `running.png`), all 2732x1984.
Baseline: commit `d895281` "feat(qa): reusable end-to-end GUI visual audit workflow".

**Baseline warning.** Round-11 fixes (commit `3605609`) are NOT an ancestor of
HEAD, and no round-12 fix is present either. Every round-11/12 finding is
expected open in this baseline; nothing below counts as a regression. Verified
statements compare this baseline against round-12 measurements of a different
branch.

## Capture set

| File | State | Freshness |
|---|---|---|
| `latest.png` | default (CircuitTopology::Fun, seq stopped) | fresh |
| `fun.png` | "fun" (CircuitTopology::Fun, seq stopped) | fresh |
| `running.png` | seq_run=true | fresh |
| `accurate.png`, `advanced.png` | — | **stale (previous round, 17:04); excluded from every conclusion** |

`fun.png` is pixel-identical to `latest.png` inside the app (diff limited to the
macOS title-bar traffic-light dots, y 18-45 x 18-353). Cause: `params.rs:414-426`
maps both "default" and "fun" to `CircuitTopology::Fun`. The harness captures
three PNGs but only two states — a capture-harness defect (see HIGH-7).
`running.png` differs from `latest.png` inside the sequencer step grid as
expected (playhead on step 08).

## Round-12 items — status on this baseline

Round-12 fixes are absent here, so "verified fixed" is not possible. Observed
against round-12's measurements:

- **HIGH-1 (knob bipolar inferred from default==0.5).** Still open in a
  different form: on this baseline `knob.rs` has NO polarity logic at all —
  every arc fills from minimum (`knob.rs:139-151`). See HIGH-1 below.
- **HIGH-4 (playhead = 3 unaligned white elements).** Changed, not resolved:
  the playhead is now one magenta colour (#D2456A) instead of three white
  elements, but the strokes probe shows three stroke weights [1, 2, 3] px —
  still a composite. See HIGH-2.
- **HIGH-3 (disabled text 2.29-2.32:1 in ACCURATE).** Not verifiable — no
  ACCURATE capture this round. A same-class defect is present in the default
  state; see HIGH-6.
- **HIGH-5, HIGH-6, HIGH-7 (dead space), MEDIUM-8 (combo font size),
  MEDIUM-14/18 (alignment).** All re-observed on this baseline; see below.
- **HIGH-2 (QUALITY selection indicator lost in ACCURATE).** Not verifiable —
  no ACCURATE capture.

## Layout map

1. Header — y 84-170 — logo, QUALITY combo, ZOOM combo, right group.
2. ENVELOPES frame — y 179-400, x 1460-2699 — ADV ENV checkbox, MEG ADSR and
   VEG ADSR banks. Source: `plugin303/src/gui/panels/top_controls.rs:105-187`.
3. Oscillator / souped section — y 400-698, full width, divider x 541 —
   includes the toggle row y 472-521.
4. Clipper tweaks — y 707-889, full width.
5. Output clipper — y 903-1140, full width — 9 clipper knobs.
   Source: `plugin303/src/gui/panels/output_clipper.rs:85-112`.
6. GENERATOR — y 1149-1376, full width, divider x 1210 — GEN SEED, MUTATION,
   GENERATE / MUTATE.
7. Sequencer + right column — y 1385-1971, v-divider x 2136 — step grid left,
   TRANSPORT & PATTERN right.

---

## HIGH

### 1. Bipolar knobs fill their value arc from minimum, contradicting the centre-detent value

**Symptom.** All captures. `knob.rs:139-151` draws the active arc from
`start_angle` whenever `normalized > 0.001` — there is no bipolar/polarity
branch anywhere in the file. The needle uses the same normalized value
(`knob.rs:158`, `compute_needle_angle` at `knob.rs:281`), so for TUNING,
VOLUME, PITCH, IN, OUT and TILT the badge reads `+0.0 ct` / `+0.0 dB` (needle
at 12 o'clock, normalized = 0.5) while the arc sits half-filled from the
minimum. The widget claims two values at once. Unipolar 50 %-default knobs
(CUTOFF, RESONANCE, ENV MOD, DECAY, ACCENT, clipper bank) correctly fill from
min — the same code path is right for them and wrong for the bipolar six.

**Required fix.** Arc origin must follow the parameter's polarity: unipolar
fills from minimum, bipolar fills from the centre detent outward in the
direction of the value. Polarity must come from the parameter's range/type,
not from the current value or default. Unipolar arc rendering, needle angles,
and tick layout must not change.

### 2. Playhead indicator is still a multi-weight composite

**Symptom.** `running.png`. Strokes probe on box (1855,1390)-(2115,1665),
colour #D2456A, reports horizontal y1412 thickness 3 px, y1419 thickness 1 px,
verticals x1962/1968/1976/1982 thickness 1-2 px: `NON-UNIFORM WEIGHT:
thicknesses [1, 2, 3]`. The eye reads one "heavy" mark that is actually a 3 px
top bar plus a 1 px outline plus the magenta "08" cell edge. Improvement over
round 12 (one magenta colour, no white elements) but the single-indicator /
single-weight requirement is still unmet.

**Required fix.** The playhead must be one mark of one stroke weight around
the current step — one colour, one thickness, no contribution from the step
cell's own border. Non-current steps must show no segment of that colour
(control probe on a neighbouring step must come back empty).

### 3. ENVELOPES frame interior: ~265 logical px dead run between MEG and VEG banks

**Symptom.** All captures. The frame spans x 1460-2699, but
`top_controls.rs:154-156` computes `spacer_w = (avail - 2*115).max(6)` and
pushes VEG to the frame's right edge, leaving the entire middle empty
(roughly x 1900-2170 source px dead in band y 179-400). Round-12 HIGH-6
measured this as dead space right of ENVELOPES; the frame now contains the
dead run instead.

**Required fix.** MEG and VEG banks must distribute the frame width without a
dead centre run — even spacing or a shared grid. Frame width, title row
(ENVELOPES + ADV ENV), and bank contents must not change.

### 4. TRANSPORT & PATTERN column: dead bottom and dead right side

**Symptom.** All captures. Crop of x 2110-2732, y 1380-1790: content ends at
y ~1660 while the column runs to y ~1971 (panel band y 1385-1971) — a dead
block of ~150 logical px height below the BPM/LEN row (round-12 HIGH-7
measured 265 px on its baseline). Rows also stop at x ~2495-2570 while the
column extends to ~2703 — a dead run of ~70-100 logical px on the right of
every row.

**Required fix.** The column's rows must either fill the column width or the
column must shrink to its content; the empty bottom must be absorbed by the
layout (taller sequencer grid, or the column sized to content). Row contents
and order must not change.

### 5. Souped-section toggle row: 83 % dead width

**Symptom.** All captures. Band y 472-521 inside panel y 400-698 holds a
single left-aligned toggle cluster; deadspace measured a 2243 px dead run in
that band (round-12 HIGH-5, re-measured this round).

**Required fix.** The toggle row must use its width (distribute its controls
across the band) or the band must collapse to content height. Toggle labels
and state colours must not change.

### 6. Disabled ADSR content renders below 3:1 with a non-theme colour

**Symptom.** All captures (ADV ENV off by default → both banks disabled,
`top_controls.rs:158/173`). Colour report flags #54545F at 2.42:1 in panel
y 179-400 — pixel histogram locates it in the MEG and VEG slider bodies
(x 1520-1660 and x 2480-2690, y 240-370). #54545F matches no token in
`theme.rs`; it is egui's disabled fade applied to the slider widget. The
slider position marks — the state the disabled group is supposed to keep
visible — are barely readable. Text fares better but sits on the floor: MEG
title #6B6B71 3.35:1, VEG title #64646A 3.02:1, value text #6F6F75 3.55:1.
The disabling itself is deliberate (`add_enabled_ui(is_adv, …)`); losing
legibility of the current values is not.

**Required fix.** Disabled ADSR banks must keep their value/position marks at
≥3:1 against the panel background, using theme tokens only. The enabled
(ADV ENV on) appearance must not change; the disabled state must remain
visibly inactive (no accent, no hover).

### 7. Capture harness: "fun" state duplicates "default"; ACCURATE no longer captured

**Symptom.** `fun.png` ≡ `latest.png` in-app (diff only in the macOS title
bar). `params.rs:414-426` maps both names to `CircuitTopology::Fun`. The
script captures three files but yields two states, and no state selects
`CircuitTopology::Accurate` — so round-12's ACCURATE-only findings (HIGH-2,
HIGH-3) are structurally unverifiable.

**Required fix.** The state set must produce three visually distinct states:
keep "accurate" in the script, and make "default" and "fun" differ (or drop
one). Every captured file must map to a distinct code path in
`VIBE303_QA_STATE` handling.

---

## MEDIUM

### 8. Combo box glyph height 18 px vs 12 px on adjacent buttons/segments

**Symptom.** All captures. Text probe (2555,112)-(2680,158): ZOOM "100%"
glyphs h=18 (#AEAEAE on #3C3C3C, 4.97:1). Neighbouring LOW segment caption
measures h=12. Same class as round-12 MEDIUM-8 — the combo never received the
theme font size.

**Required fix.** Combo box text renders at the same size as button/segment
captions of its class. Combo popup behaviour and width must not change.

### 9. TRANSPORT & PATTERN rows do not share a right edge

**Symptom.** All captures. Crop x 2110-2732, y 1380-1790: RUN/SYNC:HOST row
ends ~x 2495, MIDI/CH/SYNTH row ends ~x 2570, SHIFT row ends ~x 2540, BPM/LEN
row ends ~x 2537. Left-flush, ragged right (round-12 MEDIUM-14 measured
2674/2676 vs 2690 on its baseline — same class, different exact numbers).

**Required fix.** Rows in the column share a common right edge, or the
raggedness is absorbed by making each row fill the column width (see HIGH-4).

### 10. GENERATOR: GEN SEED row centred while sibling rows are flush-left

**Symptom.** All captures. Edges probe (2110,1150)-(2710,1390): GEN SEED band
y 1159-1194 left=2304; MUTATION band y 1252-1287 left=2110; GENERATE/MUTATE
band y 1296-1339 left=2110. The GEN SEED label/value/NEW SEED cluster floats
centred ~97 logical px in from the left edge its siblings share (round-12
MEDIUM-18 class).

**Required fix.** All rows in the GENERATOR region share one left edge.
Button widths and the GENERATE/MUTATE split must not change.

---

## LOW

11. Inter-panel gutters inconsistent: [9, 0, 9, 14, 9, 9] — gap_above=0
    between the ENVELOPES band and the oscillator panel; 14 above the output
    clipper. One gutter value should rule.
12. VEG ADSR title at 3.02:1 — passes the 3:1 floor by 0.02. Any theme tweak
    to TEXT_MUTED or the disabled fade drops it under. Not a defect today; a
    hair away from one.

---

## Tool false positives — recorded so round 14 does not re-investigate

- `knobs` flagged TONE at (1197,755) ARC/NEEDLE MISMATCH delta=10.9°. The
  tool's ring fit put the centre 6 px high; fitting from the dim-ring
  extremes (x 1172-1223, y 730-792 → centre 1197.5,761) gives arc span
  -120.7°..-1.1° vs needle +2.0° — agreement within ~3°. Code proof:
  `knob.rs` drives arc and needle from one `normalized`, so they cannot
  disagree. **False positive.**
- `knobs` flagged a knob at (2351,1452) delta=87.4. Crop shows TRANSPORT &
  PATTERN buttons (RUN, SYNC:HOST, CH 01) — a radius-22 fit on button
  geometry. **False positive** (known r=20 dropdown-arrow class).

## Flagged, intent unclear — not asserted as defects

- ADV ENV unchecked disables BOTH ADSR banks (`top_controls.rs:158/173`), so
  the default state hides all envelope control. Product decision, not a
  rendering bug — but it means the default capture exercises none of the ADSR
  widgets' enabled rendering. An ADV-ENV-on capture would.
- ENVELOPES dead-space fix (HIGH-3) is a layout choice — banks-at-edges vs
  banks-centred. Either closes the finding; only the dead middle may not stay.

## Capture gaps for round 14

- **No ACCURATE state** — round-12 HIGH-2 (QUALITY selection indicator) and
  HIGH-3 (disabled text 2.29:1) unverifiable by construction.
- **No ADV-ENV-on state** — enabled ADSR rendering never exercised.
- **No knob off its default** — HIGH-1's arc polarity is inferred from code
  and centre-detent badges, not measured at a non-centre bipolar value.
- **No hover/focus/pressed state** for any control class.
- **No zoom >100 %** state (round-9's fixed-window clipping class).

## Most urgent

1. **HIGH-1** — bipolar arc polarity: six knobs contradict their own badges on
   every screen.
2. **HIGH-7** — capture harness: until the state set is fixed, two HIGH
   classes cannot even be seen.
3. **HIGH-6** — disabled ADSR state illegible at 2.42:1, non-theme colour.
4. **HIGH-2** — playhead still a composite mark; closest to done of the HIGHs.
5. **HIGH-3 / HIGH-4 / HIGH-5** — dead-space trio; one layout pass clears all
   three.
