# GUI Visual Remediation — Round 11

You are fixing visual defects in the TECHNEO 303 desktop GUI (Rust + egui). A visual audit of
fresh screenshots found the issues below. Fix all of them.

## Context

- The app is a TB-303-style bass synth with a Euclidean pattern generator and a 16-step sequencer.
- Audited window: 2732 x 1984 physical pixels (retina, 2x), TOPOLOGY = FUN, QUALITY = LOW,
  ZOOM = 100%.
- Captures live in `target/gui-qa/`. The full-window stopped state is `latest.png`; the running
  state is `running.png`.
- All coordinates in this document are physical pixels in those 2732 x 1984 captures.

### UI regions (top to bottom)

1. **Title bar** — traffic lights, "TECHNEO 303".
2. **Header panel** — app title, `BASS SYNTH` / `MONO` badges, `TOPOLOGY` segmented control,
   `QUALITY` segmented control, `ZOOM` dropdown.
3. **Oscillator row panel** — `WAVE` selector (SAW/SQR/SWP) and seven large knobs (TUNING, CUTOFF,
   RESONANCE, ENV MOD, DECAY, ACCENT, VOLUME); on its right, a nested **ENVELOPES** sub-panel
   containing the `ADV ENV` checkbox, the **MEG ADSR (Mod Envelope)** group and the
   **VEG ADSR (Voice Envelope)** group.
4. **SOUPED-UP ENGINE & ANALOG DRAWER** — `VCF TAPS`, `VCA ENV`, and nine knobs (DRIVE, SUB, GLIDE,
   PITCH1, PITCH2, MIX2, PW, SWEEP, R-COMP).
5. **OUTPUT CLIPPER** — `BYPASS`, `OS`, nine knobs (DRIVE, THRESH, BASS, TONE, IN, OUT, TILT, DYN,
   MIX), and a `LINK` / `DIODE` group on the right.
6. **ANALOG CHARACTER & TWEAKS** — `TOL` / `AGE` / `NOISE`, the `DRIFT` knob, `SEED` +
   the `RECAL` knob, and the `CALIBRATE` button with its `CALIBRATED` status label; four columns
   separated by vertical dividers.
7. **ACID EUCLIDEAN GENERATOR** — `GEN SEED` + `NEW SEED` in the title row, then four numbered
   sub-sections: 1. PATTERN STRUCTURE, 2. OCTAVE BEHAVIOR, 3. PROBABILITIES, 4. GENERATION.
8. **Sequencer grid** — two rows of eight steps; each step is a number header, an `ACC`/`SLD` pair,
   a `NOTE`/`TIE`/`REST` cell, a note-name cell, and an up/down arrow pair.
9. **TRANSPORT & PATTERN** — `RUN`/`STOP`, `SYNC`, `MIDI` / `CH` / `SYNTH`, `< SHIFT` / `> SHIFT`,
   `BPM` stepper + `LEN`, and a bottom row of rate / mode / `JITTER` / `CLEAR`.

---

## 1. Knob needle contradicts its own arc fill

**Symptom.** The white needle and the orange arc fill are driven by different values. They coincide
at 0 %, 50 % and 100 % and diverge in between:

| Knob | Displayed value | Arc terminus | Needle |
|---|---|---|---|
| `GLIDE` (region 4) | `200 ms` | ≈ 44 % of sweep | ≈ 61 % of sweep |
| `DRIFT` (region 6) | `10%` | ≈ 10 % of sweep | ≈ 33 % of sweep |
| `RECAL` (region 6) | `100%` | 100 % | ≈ 97 % |

One path is using the raw parameter value and the other the normalised (skewed) position.

**Required fix.** The needle angle and the arc terminus must be computed from a single normalised
position value, in one place, for every knob. After the fix, for every knob in regions 3, 4, 5 and
6, at any value, the needle must point exactly at the end of the orange arc — no visible angular
gap at any value, on linear, logarithmic, offset and bipolar ranges alike. Do not change any
parameter range, skew, or displayed value text; only make the two indicators agree.

## 2. ENVELOPES sub-panel overflows its parent panel

**Symptom.** At x = 2705 the oscillator-row panel's bottom border is at y = 367–368, while the
ENVELOPES sub-panel's border runs to y = 400–401 and the VEG ADSR frame to y ≈ 380. The ENVELOPES
box hangs about 33 px below the panel that contains it. Its right border (x = 2699) is only 12 px
inside the parent's (x = 2711), so three rounded frames stack inside a 30 px band in the top-right
corner.

**Required fix.** The ENVELOPES sub-panel must sit fully inside the oscillator-row panel on all four
sides, with the same inner padding on its bottom edge as on its top edge, and an inner padding on
its right edge equal to the panel's left inner padding. No border of a child frame may cross or
touch a border of its parent.

## 3. ENVELOPES panel is mostly dead space

**Symptom.** MEG ADSR is pinned to the sub-panel's left edge and VEG ADSR to its right edge, with
roughly 1150 px of empty panel between two groups that are each about 340 px wide.

**Required fix.** Remove the void. Either place the two ADSR groups adjacent with a normal control
gap and let the sub-panel shrink to fit its content, or distribute the two groups so no single gap
inside the panel exceeds the largest gap between sibling controls elsewhere in the same window. The
panel must not contain a horizontal empty run wider than the widest control it holds.

## 4. TRANSPORT & PATTERN dead space

**Symptom.** About 225 px of empty panel sits between the `BPM`/`LEN` row and the bottom
rate/mode/`JITTER`/`CLEAR` row — the content is top-anchored and bottom-anchored with a void
between. Horizontally, the widest control (`SYNTH: ON`) stops about 120 px short of the panel's
right border, and `SYNC: HOST` stops about 215 px short.

**Required fix.** The panel's rows must flow from the top with consistent row spacing and no
bottom-anchored group leaving a gap; any leftover vertical space belongs below the last row, not
between rows. Horizontally, either widen the controls to a shared grid that reaches the panel's
right inner margin, or narrow the panel and give the reclaimed width to the sequencer grid on its
left. No empty vertical run inside the panel may exceed one row height.

## 5. Sequencer playhead frame is broken and shifts column geometry

**Symptom.** In `running.png`, step 16 carries a red frame with four problems: its left and right
edges are interrupted at every internal row boundary because the child widgets' backgrounds paint
over the border; it is drawn ~2 px outside each step widget's own border, so the step-number cell
shows a double outline; it extends above the step-number header and below the arrow row, making the
highlighted column taller than its neighbours; and the step number turns red, the only red text in
the UI.

**Required fix.** The playhead indicator must be a single continuous, unbroken outline (or a
background fill) around the current step column, painted after — or behind — the step's child
widgets so nothing overdraws it. It must occupy exactly the same bounding box as a non-highlighted
step column: switching the playhead on or off must not change any step's position or size by a
single pixel. Compare the highlighted column's bounding box against its neighbours to confirm.

## 6. Playhead uses a colour outside the palette

**Symptom.** The established palette is orange (`ACC`, active toggles), cyan (`NOTE`, `SLD`) and
amber (`TIE`). The playhead is red/pink, which in this palette reads as an error.

**Required fix.** Draw the playhead in a colour already in the palette, or in a neutral high-value
highlight, so it reads as "current step" and not as an error. It must remain distinguishable from
the `ACC` orange and the `NOTE` cyan on the same column.

## 7. VEG ADSR is drawn disabled and fails WCAG AA

**Symptom.** The MEG ADSR group (gated by the unchecked `ADV ENV`) and the VEG ADSR group render in
the identical disabled tier `#6f6f75`, measured **3.55:1** against the `#18181c` panel background —
below the WCAG AA 4.5:1 floor for normal text. The voice envelope is not gated by `ADV ENV`, so a
live control group is drawn as disabled and below the contrast floor. Affects both group titles, the
`A` / `D` / `S` / `R` letters and the `3 ms` / `200 ms` / `0%` value labels.

**Required fix.** Render the VEG ADSR group in the normal (enabled) text tier, visually distinct
from the disabled MEG group. Every text colour used for an enabled control must measure at least
4.5:1 against the surface it is drawn on. If a disabled tier below 4.5:1 is kept for genuinely
disabled controls, it must be used only for controls that are actually disabled.

## 8. Panel titles use two colours with no rule

**Symptom.** Orange `#ff7a01`: `SOUPED-UP ENGINE & ANALOG DRAWER`, `ANALOG CHARACTER & TWEAKS`,
`ACID EUCLIDEAN GENERATOR`. White `#ffffff`: `ENVELOPES`, `OUTPUT CLIPPER`, `TRANSPORT & PATTERN`.
All six are top-level panel titles.

**Required fix.** Pick one colour for top-level panel titles and apply it to all six. If a second
tier is genuinely needed, it must correspond to a real structural difference and be documented in a
comment next to the constant.

## 9. Header right inset is more than double the left inset

**Symptom.** Header content spans x = 42 … 2664 inside a panel spanning x = 20 … 2711 — 22 px of
left inset against 47 px of right inset.

**Required fix.** The right-hand group (`ZOOM`) must end at the same inner margin from the panel's
right border as the app title starts from its left border, within 2 px.

## 10. ANALOG CHARACTER & TWEAKS misalignment

**Symptom.** In region 6:
- The `DRIFT` and `RECAL` knob centres differ by about 20 px vertically.
- The two knobs are drawn at different diameters (about 33 px against 36 px).
- Inside the third column, `SEED: 1337` is left-aligned while the `RECAL` knob is centred.
- Column 1's content ends 273 px before its divider; the `CALIBRATE` button fills about 36 % of
  column 4.
- The `CALIBRATE` button label uses the muted `#8f8fa4` while every other button label is
  `#ffffff`, so the button reads as disabled.

**Required fix.** All knobs in this panel share one diameter and one vertical centre line. Within
each column, all content uses the same horizontal alignment. The four columns are sized to their
content rather than to equal fixed widths, so no column has a large trailing void. The `CALIBRATE`
label uses the same text tier as other enabled button labels.

## 11. ACID EUCLIDEAN GENERATOR ragged columns

**Symptom.** Value fields inside each numbered sub-section do not share a left edge:
- Section 1: the `ROOT` field starts at x ≈ 84, `PULSES` at x ≈ 110, `SHIFT` at x ≈ 84; the second
  label of each row starts at x ≈ 151 / 181 / 151.
- Section 2: the `OCT MIN/MAX` field starts at x ≈ 619 while `OCT UP` and `OCT DN` start at x ≈ 583.
- Section 3: the `ACCENT` field starts at x ≈ 1196 while `ACC BIAS` starts at x ≈ 1215.

Field widths also drift: the two `16` fields (`STEPS (n)`, `PITCH LEN`) are 66 px and 56 px wide;
`05` is 60 px and `00` is 54 px.

**Required fix.** Inside each numbered sub-section, lay the rows out on a label/field grid: every
label in a given column shares a left edge, and every field in a given column shares a left edge and
a width. Numeric entry fields holding values of the same kind and digit count must be the same
width.

## 12. Two type families mixed inside the same rows

**Symptom.** Monospace is used for `05`, `16`, `00`, `01`, `120`, `0042`, `1337`, `+0.0 ct`,
`2.1 kHz`, `C 3`; the proportional UI font for `20%`, `35%`, `25%`, `50%`, `15%`, `Pentatonic`,
`16th`, `Direct`. `PULSES (k): 05` and `REST: 20%` sit in the same visual row of the same panel in
different fonts.

**Required fix.** Establish one rule — for example, monospace for all numeric readouts and
proportional for all word-valued controls — and apply it consistently. Two numeric fields side by
side in the same sub-section must never use different families.

## 13. `MIDI: OFF` and `SYNTH: ON` render identically

**Symptom.** Both are dark outlined buttons with white text; the on/off difference is carried only
by the word in the label. In the same panel, `RUN` encodes its state with an orange fill and
`SYNC: HOST` with an orange border — three encodings for one concept.

**Required fix.** Use one visual encoding for boolean state across the transport panel, so an
enabled toggle is visually distinct from a disabled one without reading the label text. Keep the
label text as it is.

## 14. Starting the transport also flips SYNC from HOST to INT

**Symptom.** A pixel diff of `latest.png` against `running.png` shows exactly four changed regions:
`y 1428–1475, x 2225–2494`, which covers both the `RUN`→`STOP` button and the
`SYNC: HOST`→`SYNC: INT` button, plus three bands in the step-16 column. The sync source changed
together with the transport state.

**Required fix.** Determine whether the capture script toggled both controls or whether starting the
transport mutates the sync source. If it is the capture script, fix the script so the running
capture differs from the stopped capture only in transport state. If the app is mutating the sync
source when the transport starts, that is a behavioural bug — report it rather than papering over it
in the presentation layer.

## 15. No live feedback while running

**Symptom.** The same pixel diff confirms that between stopped and running, nothing on screen
changes except the transport buttons and the step-16 playhead. There is no level meter, envelope
activity, or clock indicator.

**Required fix.** Presentation only: if any live signal is already available to the GUI (output
level, envelope value, clock phase), surface it. If none is available, do not invent one — instead
note in the confirmation what would be required. Do not add DSP or plumbing to satisfy this item.

---

## Lower-severity items

16. **Bipolar knob fill.** `TUNING`, `PITCH1`, `PITCH2`, `IN`, `OUT`, `TILT` fill the arc from the
    minimum up to 12 o'clock at their zero value. Conventionally a bipolar control at zero shows no
    fill, and fills outward from the centre detent. Change these to fill from the centre — or, if
    the current look is deliberate, say so in the confirmation and leave it.
17. **`ZOOM` label alignment.** It is left-aligned over its control while `TOPOLOGY` and `QUALITY`
    are centred over theirs. Make all three consistent.
18. **Two seed formats.** `SEED: 1337` (editable field, unpadded) in region 6 and
    `GEN SEED: 0042` (static text, zero-padded to four digits) in region 7. Use one format, and make
    it obvious that these are two different seeds if they are.
19. **`SLD CHAIN` label size.** It renders at a noticeably larger font than its sibling labels
    (`REST:`, `SLIDE:`, `ACCENT:`, `ACC BIAS:`). Match the sub-section's label size.
20. **`OCT MIN/MAX` boxes.** Two unlabelled fields (`-1`, `+1`) with no indication of which is min
    and which is max. Label them.
21. **Transport row right edges.** Five rows end at five different x positions (panel-local 712,
    860, 709, 791, 903). Put them on a shared grid.
22. **ADSR handle rendering at track extremes.** At minimum (`A = 3 ms`, `S = 0%`) the handle is a
    thin bar overhanging the track cap; mid-track handles (`D`/`R = 200 ms`) are thick rounded
    blocks. The handle must render identically at every position, fully inside the track.
23. **Sequencer bottom padding.** About 23 px, visibly tighter than the padding above the first step
    row. Match them.
24. **`BPM` stepper styling.** `-` and `+` are outlined buttons while `120` between them is a filled
    field, and the `-` glyph reads much lighter than the `+`. Give the three elements one coherent
    treatment.
25. **Capture harness.** `fun.png` is byte-identical to `latest.png` (MD5 `edd0eab4…`), and
    `running_zone_mid.png` is identical to `zone_mid.png`. No `TOPOLOGY = ACCURATE` capture exists,
    so that state could not be audited. Fix the capture script to produce a genuine second topology
    state and stop emitting duplicate files.

---

## Guardrails

- **Presentation layer only** for items 1–13 and 16–24. Do not change parameter ranges, skews,
  default values, units, DSP behaviour, or persistence. Item 1 changes which value drives the
  needle, not what the parameter is.
- Item 14 is an investigation: report the cause, do not mask the symptom.
- Item 15 must not introduce new DSP or new plumbing.
- No regressions against previously fixed rounds. In particular, preserve: symmetric 20 px outer
  panel margins on every panel; `TEXT_MUTED = #8f8fa4` at 5.59:1; the header's right group flush to
  the right; zoom presets capped at 100 %; and no per-frame allocations in the paint path.
- Do not restructure panels beyond what each required fix demands, and do not rename controls.
- Run `cargo fmt` on every file you touch.

## Verification

Re-render and capture, at minimum:

- The stopped state at TOPOLOGY = FUN, QUALITY = LOW, ZOOM = 100 % (the `latest.png` equivalent).
- The running state, with the playhead on a **middle** step, not step 16, so the frame is surrounded
  by neighbours on both sides.
- The stopped state at TOPOLOGY = ACCURATE (this capture does not exist yet — item 25).
- A capture with `ADV ENV` checked, to confirm the MEG/VEG enabled-state distinction from item 7.
- A capture with `GLIDE`, `DRIFT` and `RECAL` each set to roughly 25 %, 50 % and 75 % of their
  ranges, to confirm item 1 across the sweep.

Then confirm, item by item, referencing these numbers:

1. Needle points exactly at the arc terminus on `GLIDE`, `DRIFT` and `RECAL` at 25 %, 50 % and 75 %.
2. ENVELOPES sub-panel is fully inside the oscillator-row panel on all four sides.
3. No horizontal empty run in the ENVELOPES panel wider than its widest control.
4. No vertical empty run in TRANSPORT & PATTERN taller than one row.
5. Playhead outline is continuous, and the highlighted column's bounding box matches its neighbours.
6. Playhead colour is drawn from the existing palette.
7. VEG ADSR renders enabled; every enabled text colour measures ≥ 4.5:1 (state the measured ratio).
8. All six panel titles share one colour.
9. Header left and right insets match within 2 px (state both).
10. All region-6 knobs share one diameter and one vertical centre; `CALIBRATE` uses the enabled
    label tier.
11. Label and field left edges align within each generator sub-section; equal-kind fields share a
    width.
12. One font-family rule applied; state the rule.
13. One boolean-state encoding across the transport panel.
14. State the cause of the SYNC change and what you did about it.
15. State what live feedback you surfaced, or what would be required.
16–24. State the change made, or the reason for leaving it.
25. State that the capture script now produces distinct topology captures and no duplicates.
