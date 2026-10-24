# TECHNEO 303 — GUI Visual Audit, Round 21

Date: 2026-10-23
Captures: `target/gui-qa-offscreen` (`latest`, `accurate`, `advanced`, `running`,
`running_note`, `knobs`, `quality_low`, `generator`), all 2432x1280, offscreen render.
Baseline: commit `da9e6ed` "fix(gui): ENABLE FUN MODE tooltip, lit-action rule,
changelog (#11)" on `fix/gui-audit-round20`.

## Capture set

| File | State |
|---|---|
| `latest.png` | defaults: FUN, QUALITY DRAFT, stopped |
| `accurate.png` | ACCURATE: SOUPED-UP banner, ANALOG TWEAKS readouts, QUALITY and ADV ENV disabled |
| `advanced.png` | ADV ENV on: both ADSR banks enabled |
| `running.png` | running, playhead on step 04 (a REST step) |
| `running_note.png` | running, every step NOTE |
| `knobs.png` | TUNING +25.0 ct, CUTOFF 0.75 (1.4 kHz) |
| `quality_low.png` | QUALITY LOW |
| `generator.png` | every generator field off its default, SLD CHAIN on |

Pixel diffs confirm the states are distinct and correctly scoped:

- `accurate`: the app bar (y 14-49), the ADV ENV tile (y 66-101, x 2290-2407),
  the SOUPED-UP header and body (y 272-464) and ANALOG TWEAKS (y 481-566).
- `advanced`: ENVELOPES (y 74-248), plus the GLIDE value pill (y 401-414,
  x 432-459): 5.5 ms becomes 170 ms. This is by design: the readout follows
  the voice's glide map, and that map changes with ADV ENV
  (`value_text.rs:55`, `voice.rs:1489`). The expected-scope table in
  `measurement-recipes.md` did not list it. That table is now updated.
- `knobs`: TUNING and CUTOFF only (y 118-244).
- `quality_low`: the QUALITY bar only.
- `generator`: the generator body and the GENERATION SEED value (x ≤ 2002).
- `running`: step 04 (LED, number, plate seams) and the RUN tile (x ≤ 1850).
- `running_note`: as `running`, plus every gate and pitch cell.

## Previous round (20)

- **1 Disabled ANALOG TWEAKS labels below 3:1: FIXED.** `accurate.png`, `text --box`:
  TOL 4.56:1, CMP SEED 4.56:1, FUN MODE ONLY 4.50:1, DRIFT and RECAL labels
  4.38-4.44:1, readouts `TEXT_SECONDARY` 8.77:1. The `#7B7B7B` that `colors`
  flags (177 px, 3.58:1) is glyph antialiasing around these labels. No run
  measures as a separate colour.
- **2 ZOOM 75% dead band: FIXED (control removed).** The app bar has no ZOOM.
  `gui/mod.rs:171-172` turns off keyboard zoom. The `zoom75` state is gone.
- **3 ACCURATE tweaks give no reason: FIXED.** The status reads FUN MODE ONLY,
  and CALIBRATE is hidden. `gui_tooltips.rs` (8 passed at `da9e6ed`) checks the
  disabled-hover text.
- **4 Static orange outweighs live feedback: FIXED.** The table below is the
  sum of the per-card accent counts:

  | | round 20 | round 21 |
  |---|---|---|
  | Orange at rest (`latest`) | 119,491 | 28,650 |
  | Orange running (`running`) | 141,755 | 30,274 |
  | Playhead LED share of running orange | 1,126 (0.8%) | 1,516 (5.0%) |

  GENERATE is raised, and no tile fills orange at rest. The largest static
  orange is now the four lit ACC pills: SEQUENCER 13,720 px, which is pattern data.
- **5 Off toggle looks like an action: FIXED.** Every boolean has a
  `WELL_BG`/orange LED slot. Actions have none (TRANSPORT crop: RUN, MIDI and
  JITTER have slots; CLEAR, < SHIFT and > SHIFT do not).
- **6 Playhead plate does not read: FIXED as decided.** The plate shows only
  while running. `PANEL_HEADER_BG` pixels in the step 04 column: 1 when stopped,
  7,452 when running. Step 01 has 2 in both states. `strokes`: one 8 px orange
  bar over the full 192 px column. The step number goes from 7.63:1 to 11.33:1.
- **7 Ragged tweaks field column: FIXED.** `edges --box 1760,330,2000,530`:
  all four right edges are at x 1915.
- **8 Banner title 10.5 pt: FIXED.** h = 13 px against 12 px for a 9 pt
  device title. The type table now has a 9.5 pt banner row.
- **9 CLEAR looks benign: FIXED (behaviour).** The second click is required
  (`gui_clear_confirm.rs`, 1 passed). At rest CLEAR still looks like a plain
  action, by design. The CONFIRM CLEAR state is not captured (see gaps).
- **10 ADV ENV gives no cue: FIXED.** Both banks have disabled-hover text
  (`gui_tooltips.rs`).
- **11 Abbreviated controls without tooltips: FIXED.** The knob tooltips, the
  generator fields, DIODE and ENABLE FUN MODE are covered by `gui_tooltips.rs`.
- **12 Words in a monospace well: FIXED.** MIN and MAX are proportional
  captions outside the wells.
- Flagged *knob text derived from the label*: **FIXED.** `knob.rs:209-212`
  uses `param.to_string()` or `value_text()`.
- Flagged *STOP lit while running*: **SUPERSEDED.** RUN is a `ToggleButton`
  with a fixed label.
- Flagged *TRANSPORT 8 pt labels*: **unchanged.** BPM: measures `#A1A1A1`,
  5.86:1. These are glyph cores, not drift.
- Flagged *zoom75 greys*: **SUPERSEDED.** The state is gone.

## Geometry

All 8 captures: 10 cards, every `gap_above=6`, column seam at x 1614 | 1620.
Every card matches the contract's landmark table. Every header-strip control
measures 36 px (18 pt), centred in the 44 px strip: NEW SEED, CALIBRATE,
ADV ENV, LINK, ON, and the 4-POLE, SI, FUN and DRAFT segments. Knobs: 27 in
FUN and 18 in ACCURATE. The arc tip is within 0.9 px of the pointer on every
knob. Every bipolar knob fills from the centre (TUNING +25 ct:
`arc=[-0.3, 35.2] from=centre`). No text in any state measures below 3:1.

---

## HIGH

None.

---

## MEDIUM

### 1. Two TRANSPORT combos have no caption and no tooltip

**Kind.** Design, with the symptom of a checklist I defect. The Tooltip row
covers SOUPED-UP ENGINE, OUTPUT CLIPPER, ANALOG TWEAKS and the generator, but
not TRANSPORT.

**Symptom.** Row 5 of TRANSPORT & PATTERN (`latest.png`, y 1140-1190) shows
two wells, `16th ▼` and `Direct ▼`. They have no caption. Every other TRANSPORT
field has one (CH:, BPM:, LEN:). In source they have no `on_hover_text`
either (`sequencer_panel.rs:418` and `:471`). The comment at `:405` names
them "Timebase (16th/12th) and Mode (Direct/Decoupled)". A user cannot guess
what "Direct" or "Decoupled" changes.

**Required fix.** Each combo says what it sets: a caption in the TRANSPORT
label style (8 pt), a hover tooltip, or both. Add the two combos to the
Tooltip row of the contract and to `gui_tooltips.rs`. Must NOT change: row
height (28 pt) and the two-slot grid.

### 2. MEDIUM knob pills are narrower than their longest values

**Kind.** Defect. Checklist A: a value wider than its pill.

**Symptom.** The pill is 38 pt (76 px at 2x: PITCH1, IN and GLIDE all measure
76 px). The value is 8 pt monospace (`knob.rs:224-230`) and is not clipped to
the pill. These are widths from egui's font layout, over the full range of
each parameter's `to_string`:

| Knob | Widest value | Width | Pill |
|---|---|---|---|
| PITCH1, PITCH2 | `-24.0 st` | 39.7 pt | 38 pt |
| IN, OUT | `-18.0 dB` | 39.7 pt | 38 pt |
| TILT | `-6.0 dB` | 34.7 pt | 38 pt |
| DRIVE (engine) | `10.0 dB` | 34.7 pt | 38 pt |
| TUNING (LARGE) | `-100.0 ct` | 44.7 pt | 52 pt |

The capture agrees with these widths. In `knobs.png` the 8-character
`+25.0 ct` inks 77 px. In `latest.png` the 7-character `+0.0 st` inks 67 px.
Every 8-character value is wider than its 76 px pill. On PITCH1 and PITCH2
that is any value of ±10 st or more; on IN and OUT, any value of ±10 dB or
more. The glyphs then run onto the rounded ends of the pill with no padding.
No capture holds such a value (see gaps).

**Required fix.** Every value of every knob fits inside its pill with at least
1 pt of padding on each side. Two options: widen `KNOB_PILL_WIDTH_MEDIUM` to
at least 42 pt (the 9-knob rows have room: centres are 87.5 pt apart), or
shorten the format (`-24 st`). Update the pill widths in the Knob row of the
contract. Must NOT change: the two knob tiers and the 8 pt monospace face.

### 3. The ROOT/SCALE row is off the column grid and moves with ROOT

**Kind.** Drift. Checklist G: rows in a column that do not share an edge.
Checklist D: a row that reflows when a value changes.

**Symptom.** PATTERN STRUCTURE, second label column. The STEPS (n): and
PITCH LEN: captions start at x 201-202 and the STEPS well at x 301. The SCALE:
caption starts at x 217 in `latest` (ROOT C) and at x 226 in `generator`
(ROOT D#). The SCALE well spans x 317-473 in `latest` and x 326-465 in
`generator`. The ROOT combo grows with its text: its well is 125-206 for C
and 125-215 for D#. `.width(42.0)` does not hold it
(`generator_panel.rs:90-113`). So the SCALE pair is 8 pt right of its column
at the defaults, and moves another 4.5 pt when the user picks a sharp.

**Required fix.** The ROOT and SCALE wells have a fixed width, wide enough for
`C#` and the longest scale name. SCALE: starts on the same x as STEPS (n): and
PITCH LEN:, and does not move for any ROOT or SCALE value. Must NOT change:
the three-group layout and the separators at x 545.

---

## LOW

4. **A disabled segmented bar disappears into a header strip.** Design;
   would change the Dimming table row "Segment: unselected / off". In
   `accurate.png`, LOW, HIGH and BEST in QUALITY are `SURFACE_PRESSED`
   `#303030` on the `PANEL_HEADER_BG` `#2F2F2F` app bar, 1.01:1. Only DRAFT
   (`SURFACE_HOVER`) still has a visible tile, so the bar reads as one box and
   three loose words. The labels read (`TEXT_MUTED`, 3.98:1), and the
   selection reads. The DIODE and OS bars in the OUTPUT CLIPPER header will do
   the same when the clipper is off (not captured). Proposal, in the
   direction: in a header strip, draw disabled unselected segments on a plane
   that differs from `PANEL_HEADER_BG`, for example `PANEL_BG`
   (1.13:1) or `WELL_BG`.
5. **Combo values in three treatments.** Drift (Type: "every numeric readout
   is monospace"; combo values 9.5 pt). Sizes: ROOT and SCALE use the default
   9.5 pt; the TRANSPORT combos (`sequencer_panel.rs:425`, `:480`) and TOL
   (`tweaks_panel.rs:154`) use 8.5 pt. TOL shows a number (`0%`) in the
   proportional face, left-aligned, in a column of centred monospace
   DragValues (AGE, NOISE, CMP SEED). In ACCURATE the same `0%` becomes a
   centred monospace Readout.
6. **GENERATE and MUTATE labels are left-aligned.** Drift (Components:
   Action). The GENERATE text starts at x 1653, 17 px into a tile that starts
   at x 1636. Every other action centres its label: CLEAR is at x 2191-2238 in
   a tile centred on x 2217, and < SHIFT, NEW SEED and ENABLE FUN MODE are
   also centred.
7. **The contract does not give a token for action labels.** Contract gap.
   All 11 unlit action buttons set `TEXT_PRIMARY` explicitly (for example
   `generator_panel.rs:389`, `sequencer_panel.rs:556`, CALIBRATE at rest in
   `tweaks_panel.rs:46-50`). The Colour roles list
   `TEXT_PRIMARY` for values and on toggles, and `TEXT_SECONDARY` for "the text
   on an unlit tile". The code is consistent with itself. Add action labels at
   rest to the `TEXT_PRIMARY` row.
8. **NEW SEED and the SEED it changes are in different cards.** Design
   (checklist I: a status string with no visible link to its control). NEW
   SEED is in the ACID EUCLIDEAN GENERATOR header (x 1504-1580) and changes
   `gen_params.seed` (`generator_panel.rs:63`). The value shows as the SEED
   Readout in GENERATION (x 1990-2032). Proposal: move NEW SEED next to SEED,
   or move SEED into the generator card.
9. **Two knobs are both labelled DRIVE.** Design (naming). SOUPED-UP DRIVE
   (`0.0 dB`, VCF input) and OUTPUT CLIPPER DRIVE (`50%`) sit at the same x
   (110.9) in two cards, one above the other. Their tooltips tell them apart
   (`souped_panel.rs:139`, `output_clipper.rs:96`); their labels do not.
10. **The sign on zero is not consistent.** Drift (checklist F). In
    `generator`, OCT RANGE shows MIN `0` and MAX `+1`. The knobs show zero
    as `+0.0` (`value_text::signed`).

---

## Flagged, intent unclear — not asserted as defects

- **GLIDE changes its value when ADV ENV changes.** With the knob fixed at 0.4,
  the readout is 5.5 ms with ADV ENV off and 170 ms with it on. The readout is
  correct: the voice uses the classic slide tau (applied on SLD steps only)
  or `adv_glide_ms`. The cause of the jump is a toggle in another card. The
  GLIDE tooltip could say that it depends on ADV ENV. Confirm the intent.
- **The scale names are shortened.** `Pentatonic` is `MinorPentatonic`, and
  `Harmonic` is `HarmonicMinor` (`generator_panel.rs:106-111`). From the name
  alone, a user cannot tell whether it is the minor or the major scale.
- **The TRANSPORT labels CH:, BPM: and LEN:** measure `#A1A1A1`, 5.86:1 (carried
  from round 20). These are glyph cores at 8 pt, not drift.
- **ENABLE FUN MODE is a lit fill of 7,774 px in ACCURATE.** This follows the
  round-20 decision and the Action row. Do not report it as drift.

## Capture gaps for the next round

- Long knob values: PITCH1/2 at ±24 st and IN/OUT at ±18 dB. A QA state with
  these values would prove #2 in pixels.
- OUTPUT CLIPPER off: its disabled DIODE and OS bars and its body (#4).
- The CONFIRM CLEAR state (`SURFACE_HOVER`, relabelled) during its 3 s window.
- Disabled-hover tooltips. `gui_tooltips.rs` proves they exist, but no render
  shows them.
- Open ComboBox popups: the TRANSPORT combos, ROOT, SCALE and TOL.
- Standing gaps: hover, press and focus; SQR and SWP waves; TIE under the
  playhead; the playhead on an ACC step; CALIBRATE when a rebuild is due;
  SYNTH muted, MIDI on, LINK off; QUALITY HIGH and BEST; ADV ENV in ACCURATE.
- A window capture on a 1x display: 8 pt knob values and 7.5 pt fader values
  render at 8 and 7.5 px there.

## Most urgent

1. **#2**: shipped values overflow their pills over half the PITCH and IN/OUT ranges.
2. **#1**: two transport settings that a user cannot identify.
3. **#3**: the only control row that moves when a value changes.
