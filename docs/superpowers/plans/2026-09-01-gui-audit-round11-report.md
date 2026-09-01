# GUI Visual Audit — Round 11

Date: 2026-09-01
Captures: `target/gui-qa/` (window 2732 x 1984, retina; `standalone.log` window id 45121)
Method: direct visual reading of the PNGs plus pixel measurement (border scans, WCAG contrast, stopped-vs-running pixel diff).

## Capture set

| File | Content |
|---|---|
| `latest.png` | Stopped state, TOPOLOGY = FUN, QUALITY = LOW |
| `fun.png` | **Byte-identical to `latest.png`** (same MD5 `edd0eab4…`) |
| `running.png` | Same view, transport running |
| `zone_bottom.png` | Identical content to `crop_sequencer.png` |
| `running_zone_mid.png` | Identical content to `zone_mid.png` |
| others | Zone / margin crops of `latest.png` |

**Harness gap (A-1).** No capture exists for `TOPOLOGY = ACCURATE`, and `fun.png` duplicates
`latest.png` byte for byte. The ACCURATE-vs-FUN state comparison could not be audited this round.
`running_zone_mid.png` is also a duplicate. The capture script should either produce a real
second topology state or stop emitting the duplicate files.

## Layout map

1. Title bar — traffic lights, "TECHNEO 303"
2. Header panel — title, BASS SYNTH / MONO badges, TOPOLOGY, QUALITY, ZOOM
3. Oscillator row panel — WAVE selector, 7 large knobs, nested ENVELOPES sub-panel (MEG ADSR / VEG ADSR)
4. SOUPED-UP ENGINE & ANALOG DRAWER — VCF TAPS, VCA ENV, 9 knobs
5. OUTPUT CLIPPER — BYPASS, OS, 9 knobs, LINK / DIODE group
6. ANALOG CHARACTER & TWEAKS — TOL/AGE/NOISE, DRIFT, SEED/RECAL, CALIBRATE
7. ACID EUCLIDEAN GENERATOR — 4 numbered sub-sections, GEN SEED, NEW SEED
8. Sequencer grid — 2 rows x 8 steps
9. TRANSPORT & PATTERN — RUN/SYNC, MIDI/CH/SYNTH, SHIFT, BPM/LEN, rate/mode/JITTER/CLEAR

Outer panel margins measured symmetric at 20 px left and right on every panel — the round-10
margin fix holds. `TEXT_MUTED` measured `#8f8fa4` = 5.59:1 against panel background — the
round-10 contrast fix also holds.

---

## HIGH

### H-1. Knob needle contradicts its own arc fill on non-linear parameters

The white needle and the orange arc are driven by different values. They agree at 0 %, 50 % and
100 %, and diverge everywhere else.

| Knob | Displayed value | Arc terminus | Needle |
|---|---|---|---|
| GLIDE (souped row) | `200 ms` | ≈ 11:30 (≈ 44 % of sweep) | ≈ 1:00 (≈ 61 % of sweep) |
| DRIFT (analog character) | `10%` | ≈ 8:00 (≈ 10 % of sweep) | ≈ 10:30 (≈ 33 % of sweep) |
| RECAL (analog character) | `100%` | 4:30 (100 %) | ≈ 4:15 (≈ 97 %) |

Screenshots: `latest.png`, `running.png` (identical in this region). Most visible on DRIFT, where
the arc is a short stub while the needle sits roughly a third of the way round.

One of the two is reading the raw parameter and the other the normalised/skewed position. Every
knob whose range is non-linear or offset will show the contradiction.

### H-2. ENVELOPES sub-panel overflows the bottom of its parent panel

Measured border rows at x = 2705: the oscillator-row panel's bottom border is at y = 367–368, but
the ENVELOPES sub-panel's border runs to y = 400–401 and the VEG ADSR frame to y ≈ 380. The
ENVELOPES box hangs ~33 px below the panel that is supposed to contain it, and its right border
(x = 2699) sits only 12 px inside the parent's (x = 2711). Three rounded frames nest inside a
30 px band in the top-right corner, producing a stacked-outline artifact.

Screenshots: `latest.png`, `crop_right_margin.png`, `zone_top.png`.

### H-3. ENVELOPES panel is ~60 % dead space

MEG ADSR is pinned to the panel's left edge, VEG ADSR to its right edge, with roughly 1150 px of
empty panel between two ~340 px groups. Classic `left_to_right` + `right_to_left` with no filler.

Screenshots: `latest.png`, `zone_top.png`.

### H-4. TRANSPORT & PATTERN panel has a large vertical void and an unused right third

- ~225 px of empty panel between the BPM/LEN row and the bottom rate/mode/JITTER/CLEAR row.
  Content is top-anchored and bottom-anchored with nothing between.
- Right dead space: the widest control in the panel (`SYNTH: ON`) ends ~120 px short of the panel
  border; `SYNC: HOST` ends ~215 px short.

The sequencer grid to the left is comparatively cramped. Screenshots: `latest.png`,
`crop_sequencer.png`, `crop_right_margin.png`.

### H-5. Playhead frame is discontinuous and changes the highlighted column's geometry

In `running.png`, step 16 is marked with a red frame. Defects:

- The frame's left and right edges are **broken** at each internal row boundary — the child
  widgets' own backgrounds paint over the border, leaving visible gaps.
- The frame is drawn ~2 px outside each step widget's own border, so the step number cell shows a
  double outline.
- The frame extends above the step-number header and below the arrow row, making the highlighted
  column taller than its neighbours.
- The step number itself turns red; it is the only red text in the UI.

Screenshot: `running.png`.

---

## MEDIUM

### M-1. VEG ADSR is rendered in the disabled colour but is not gated by ADV ENV

MEG ADSR (mod envelope, gated by the unchecked `ADV ENV`) and VEG ADSR (voice envelope) render in
exactly the same disabled tier: `#6f6f75`, measured **3.55:1** against the panel background — this
**fails WCAG AA** for normal text. The voice envelope is presumably always active, so a live
control group is drawn as disabled and below the contrast floor. Affects the group titles, the
A/D/S/R letters, and the `3 ms` / `200 ms` / `0%` value labels of both groups.

### M-2. Panel titles use two different colours with no rule

Orange `#ff7a01`: SOUPED-UP ENGINE & ANALOG DRAWER, ANALOG CHARACTER & TWEAKS, ACID EUCLIDEAN
GENERATOR. White `#ffffff`: ENVELOPES, OUTPUT CLIPPER, TRANSPORT & PATTERN. Both sets are
top-level panel titles.

### M-3. Header right inset is more than double the left inset

Header content spans x = 42 … 2664 inside a panel spanning 20 … 2711 — 22 px of left inset against
47 px of right inset. The ZOOM control does not sit on the same margin as the title.
Screenshot: `crop_header.png`.

### M-4. ANALOG CHARACTER & TWEAKS — misaligned and sparse four-column layout

- The DRIFT and RECAL knob centres differ by ~20 px vertically; nothing in the panel shares a
  baseline (TOL row, SEED field, CALIBRATE button and the two knobs are all at different y).
- The two knobs are drawn at slightly different diameters (DRIFT ≈ 33 px, RECAL ≈ 36 px).
- Inside the third column, `SEED: 1337` is left-aligned while the RECAL knob is centred.
- Column 1 content ends 273 px before its divider; the CALIBRATE button occupies ~36 % of column 4.
- `CALIBRATE` button text uses `#8f8fa4` while every other button label is `#ffffff`, so the button
  reads as disabled.

Screenshot: `latest.png`, `crop_right_margin.png`.

### M-5. ACID EUCLIDEAN GENERATOR — ragged label/field columns

Within each numbered sub-section the value fields do not share a left edge:

- Section 1: ROOT field at x ≈ 84, PULSES field at x ≈ 110, SHIFT field at x ≈ 84; the second label
  of each row starts at 151 / 181 / 151.
- Section 2: the OCT MIN/MAX field starts at x ≈ 619 while OCT UP and OCT DN start at x ≈ 583.
- Section 3: the ACCENT field starts at x ≈ 1196 while ACC BIAS starts at x ≈ 1215.

Field widths also drift: the two `16` fields (STEPS n, PITCH LEN) are 66 px and 56 px wide;
`05` is 60 px and `00` is 54 px.

### M-6. Two type families mixed inside the same control rows

Monospace: `05`, `16`, `00`, `01`, `120`, `0042`, `1337`, `+0.0 ct`, `2.1 kHz`, `C 3`.
Proportional: `20%`, `35%`, `25%`, `50%`, `15%`, `Pentatonic`, `16th`, `Direct`.
`PULSES (k): 05` and `REST: 20%` sit in the same visual row of the same panel in different fonts.

### M-7. `MIDI: OFF` and `SYNTH: ON` render identically

Both are dark outlined buttons with white text; the on/off state is carried only by the word inside
the label. Elsewhere in the same panel, state is encoded by fill (`RUN` orange fill) or border
(`SYNC: HOST` orange border). Three encodings for the same concept in one panel.

### M-8. Playhead colour introduces a fourth accent

The palette is orange (ACC / active), cyan (NOTE / SLD), amber (TIE). The playhead is red/pink,
which in this palette reads as an error state rather than "current step".

### M-9. Starting the transport also flips SYNC from HOST to INT

Pixel diff of `latest.png` against `running.png` returns exactly four changed regions:
`y 1428–1475, x 2225–2494` (the RUN→STOP **and** SYNC:HOST→SYNC:INT buttons) and three bands in the
step-16 column. The sync source changed together with the transport state. Either the capture
script toggled both, or starting the transport mutates the sync setting; the latter would be a
behavioural defect.

### M-10. No live feedback anywhere except the playhead

The same pixel diff confirms that between stopped and running **nothing else on screen changes** —
no level meter, no envelope activity, no MIDI/clock indicator. For a running synth this is a large
absence of state feedback.

---

## LOW / flagged observations

- **L-1.** Bipolar knobs (`TUNING +0.0 ct`, `PITCH1/PITCH2 +0.0 st`, `IN/OUT/TILT +0.0 dB`) fill the
  arc from the minimum up to 12 o'clock rather than from a centre detent. It is applied
  consistently, so it looks deliberate, but a bipolar control at zero conventionally shows no fill.
- **L-2.** The `ZOOM` label is left-aligned over its control while `TOPOLOGY` and `QUALITY` are
  centred over theirs.
- **L-3.** Two seeds in two formats: `SEED: 1337` (editable field, unpadded) in ANALOG CHARACTER and
  `GEN SEED: 0042` (static text, zero-padded to 4) in the generator.
- **L-4.** The `SLD CHAIN` checkbox label is rendered at a noticeably larger font size than every
  sibling label in the same sub-section (`REST:`, `SLIDE:`, `ACCENT:`, `ACC BIAS:`).
- **L-5.** `OCT MIN/MAX:` is followed by two unlabelled boxes (`-1`, `+1`); which is min and which is
  max is not indicated.
- **L-6.** Transport rows have five different right edges (RUN/SYNC row 712, MIDI row 860, SHIFT row
  709, BPM row 791, bottom row 903 in panel-local px) — no shared grid.
- **L-7.** ADSR slider handles render differently at the track extremes: at minimum (`A = 3 ms`,
  `S = 0%`) the handle is a thin bar overhanging the track cap, while mid-track handles
  (`D`/`R = 200 ms`) are thick rounded blocks with a filled track below them.
- **L-8.** The sequencer panel's bottom inner padding is ~23 px, visibly tighter than the padding
  above the first step row.
- **L-9.** The `BPM` stepper mixes styles: `-` and `+` are outlined buttons while `120` between them
  is a filled field; the `-` glyph is a plain hyphen and reads much lighter than the `+`.

---

## Most urgent

1. **H-1** — needle/arc contradiction. A control that displays one value and points at another is
   the worst class of defect here.
2. **H-2** — ENVELOPES sub-panel escaping its parent's border.
3. **H-5** — broken, geometry-shifting playhead frame.
4. **H-3 / H-4** — the two large dead-space regions (ENVELOPES middle, TRANSPORT bottom-right).
5. **M-1** — VEG ADSR drawn disabled and below the WCAG AA contrast floor.
