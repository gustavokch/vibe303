# Measurement recipes

How to drive `scripts/gui_qa_measure.py`, how to read what it returns, and
which readings lie on this GUI. Read before Phase 2.

All coordinates are **physical** pixels. The offscreen render is 2432 × 1280,
the 1216 × 640 pt window at 2x. A window capture adds the macOS title bar on
top (`crop_gui_sections.py` assumes 32 pt); subtract it before you compare a
y with the contract's landmarks.

At startup the tool parses `plugin303/src/gui/theme.rs` (colour tokens and the
`KNOB_RADIUS_*` tiers) and `GUI_WIDTH` from `gui/mod.rs`, so token names, knob
tiers and the expected width follow the code. Pass `--theme <path>` from
outside the repo root. It needs numpy and Pillow; if `python3` lacks them,
run it as `uv run --no-project --with numpy --with pillow python scripts/gui_qa_measure.py`.

---

## Start here

```bash
python3 scripts/gui_qa_measure.py report target/gui-qa-offscreen
```

About 15 s for the eight states on an idle machine; `colors` takes most of
it. It prints, in audit order: the capture set, diffs against `latest`, per
capture the geometry, dead space and colours, knobs in `latest` and `knobs`,
then the accent table.

Each section below is the focused version of one part of the report.

---

## Capture set

**A width other than 2432 invalidates every later number**: the capture
script attached to the wrong window. Re-capture first.

## `diff`: do these captures show different states?

```bash
python3 scripts/gui_qa_measure.py diff target/gui-qa-offscreen/latest.png target/gui-qa-offscreen/advanced.png
```

Prints changed-region clusters as `y … x … n=…`. Expected scopes against
`latest`:

| State | Changes only in |
|---|---|
| `accurate` | app bar (TOPOLOGY, QUALITY), SOUPED-UP ENGINE header switches and body (banner), ANALOG TWEAKS (CALIBRATE hidden, readouts, status), the ADV ENV tile |
| `advanced` | ENVELOPES (both ADSR banks, ADV ENV tile), and the GLIDE value pill: its readout follows the voice's glide map, which ADV ENV switches (`value_text::glide`) |
| `knobs` | TUNING and CUTOFF |
| `quality_low` | the QUALITY bar |
| `knob_extremes` | TUNING, PITCH1, PITCH2, IN, OUT, and TILT knobs only: TUNING (`x 241-328, y 118-244`), PITCH1 and PITCH2 (`x 597-849, y 324-414`), IN, OUT and TILT (`x 772-1161, y 490-580`) |
| `clipper_off` | OUTPUT CLIPPER card only (`x 8-1614, y 434-594`; diff clusters in `x 25-1580, y 438-580` covering dimmed header switches, unlit ON tile, dimmed knobs) |
| `generator` | ACID EUCLIDEAN GENERATOR body, the GENERATION SEED value |
| `running` | SEQUENCER step 04 (plate, LED, number), the RUN tile |
| `running_note` | as `running`, plus every gate and pitch cell |

- **IDENTICAL** means the harness produced two captures of one state. That
  hides every state-comparison finding behind it. Report it.
- A cluster outside the expected scope is a state leaking into a neighbour.
- A cluster spanning the whole window is usually a 1-2 px global shift, not a
  real change. Check one landmark in both before you call it either.
- `diff` has no `--box`. To compare one region, crop it in both states.

## `panels`: geometry and gutters

```bash
python3 scripts/gui_qa_measure.py panels target/gui-qa-offscreen/latest.png
```

Cards are found by an XY cut along pure-chassis rows and columns. Expect
exactly 10 (the app bar and nine devices), every `gap_above=6`, and the
column seam at x 1614 | 1620; the contract's landmark table names each box.
Any other count or gap is a layout change. `header_bottom` marks the end of
the 44 px header strip; `interior` starts 8 px below it.

## `deadspace`: where the layout gave up

```bash
python3 scripts/gui_qa_measure.py deadspace target/gui-qa-offscreen/latest.png --min 600 --min-rows 120
```

- `DEAD COLUMNS … in band y …`: a run with no content inside one row band of
  a body. Bands 24 px or shorter, and every band of the app bar, are tagged
  `(title row, expected)`; ignore those.
- `DEAD ROWS`: a full-width empty strip in a body.

`fill=` only compares structurally similar cards: a knob bank is sparse by
nature.

## `colors`: palette drift and contrast

```bash
python3 scripts/gui_qa_measure.py colors target/gui-qa-offscreen/accurate.png
```

Per card: the background (the commonest interior colour), then every
foreground colour above the pixel floor, with its contrast against that
background and its nearest token.

The background is `PANEL_BG` in most cards but `SURFACE_ELEVATED` (#3A3A3A)
in SEQUENCER and TRANSPORT, where tiles outnumber body. There, a ratio is
against the tile fill, not the body.

Triage every flag before it becomes a finding:

- **`ACCENT_ORANGE <-- below 4.5:1`** with thousands of pixels is a lit fill
  (selected segment, lit action): a non-text mark with a 3:1 floor. Orange text
  exists only in the CALIBRATE status line. Check text on an orange fill with
  `text --box`.
- **An antialiasing grey snapped to the wrong token.** At 8.5 pt, the edge
  pixels of `TEXT_SECONDARY` labels on `PANEL_BG` settle on #878787, which
  snaps to `TEXT_MUTED` and reports 4.21:1. Probe the glyphs with `text --box`
  before you file: the TUNING label reads `TEXT_SECONDARY`, 7.3:1.
- **`NOT A THEME TOKEN`** on a few hundred pixels is a hard-coded colour or
  an egui default leaking through. A handful of antialiasing neighbours is
  normal.
- **`<-- BELOW 3:1`** on real text is a hard defect in any state.
- **Faded text hidden as a surface.** egui's disabled fade turns a
  `TEXT_SECONDARY` label on `PANEL_BG` into #6C6C6C (2.9:1). That grey snaps
  to `GATE_TIE_FILL`, a surface token, so `colors` never lists it. Matching
  surfaces exactly does not help: glyph antialiasing then floods every card
  with sub-3:1 greys. Probe every label in a disabled group with `text --box`.

Run it on **every** state. The disabled palettes (`accurate`, and the ADSR banks
in every state without ADV ENV) are where the 3:1 floor gets breached.
Surfaces, fills and gate colours are hidden unless you pass `--show-surfaces`.

## `accent`: live state where it belongs

```bash
python3 scripts/gui_qa_measure.py accent target/gui-qa-offscreen/latest.png target/gui-qa-offscreen/accurate.png
```

Accent pixels per card per state; rows are cards sorted top-left and labelled
from the first image. `<-- ACCENT IN SOME STATES ONLY` marks a card that has
accent in one capture and none in another.

The contract changes how to read it. A disabled control drops the accent by
design, so ANALOG TWEAKS at 0 in `accurate` is correct, and SLD CHAIN lighting
the generator card in `generator` is correct. The findings are:

- accent on a control that is disabled in that state;
- no accent where an enabled selection exists (every segmented bar has one
  selected segment);
- a count that drops inside a card with no state change to explain it.

Narrow a card to one control group with `--box x0,y0,x1,y1`. Then confirm that
the disabled selection still reads: crop it and look for the `SURFACE_HOVER`
segment among `SURFACE_PRESSED` neighbours.

## `knobs`: arc versus pointer

```bash
python3 scripts/gui_qa_measure.py knobs target/gui-qa-offscreen/knobs.png
```

Expect 27 knobs: 9 LARGE (seven in VOICE, DRIFT and RECAL) and 18
MEDIUM (nine in SOUPED-UP ENGINE, nine in OUTPUT CLIPPER). Expect 18 in
`accurate`, where the SOUPED-UP body is a banner. Any other count is a layout
change or a detector failure; look at the image before you read further.

Per knob: centre, tier, pointer angle in degrees clockwise from 12 o'clock
and its fraction of the -135°..+135° sweep, the arc's extent and colour
(`accent`, or `muted` when disabled), the fill origin, and the distance from
arc tip to pointer.

- `<-- ARC/NEEDLE MISMATCH` (tip more than 2 px from the pointer along the
  ring) is a hard defect.
- `from=min` versus `from=centre` is the polarity the widget chose. Check it
  against the parameter range: a range whose plain minimum is below 0 fills
  from centre, every other range from min. `from=other` is a defect.
- `arc=NONE` is correct only at a zero-length fill: a unipolar knob at its
  minimum, a bipolar knob on its detent. Read the value text to tell. At
  defaults 14 of 27 knobs draw no arc and every bipolar knob sits on its
  detent, so only `knobs` (TUNING +25 ct) tests the from-centre polarity.
- A `muted` arc on an enabled knob, or an `accent` arc on a disabled one, is
  a state defect.

Detection notes:

- The detector looks for the `KNOB_BODY` disc ringed by panel background.
  Buttons share the body colour but score 0, because both test circles land on
  the same fill. Real knobs score 0.94-1.0 (`--threshold`, default 0.7).
- The centre is a weighted centroid of the body disc alone, inside the gap
  before the ring. **Never refine a centre over a window that reaches the
  label**: the label drags it down and produces 10° errors that read as
  false mismatches.
- The pointer angle is the direction of the pointer's long axis, not the
  angle of its pixels seen from the centre. egui snaps lines to the pixel
  grid, and a 1 px sideways shift tilts the from-centre angle by up to 6° on
  a MEDIUM knob.

## `text --box`: glyph sizes and real text colour

```bash
python3 scripts/gui_qa_measure.py text target/gui-qa-offscreen/latest.png --box 235,203,335,227
```

That box is the TUNING label: `h= 12`, `TEXT_SECONDARY`, 7.3:1. Aim one probe
at a single label, caption or value. It reports the glyph bounding box (not
the ink band, so a control's own fill does not inflate it), the dominant
foreground, background, token and contrast. An 8.5 pt caption measures 12 px;
a widget at egui's 14 pt default measures about 20 px.

This is the arbiter for every contrast flag from `colors`.

## `edges --box`: row alignment

```bash
python3 scripts/gui_qa_measure.py edges target/gui-qa-offscreen/latest.png --box 1628,320,1960,540
```

That box is the ANALOG TWEAKS field column. It splits the box into row bands,
prints each band's left and right content extent, and flags `MISALIGNED`
when an edge falls short of the extreme by more than 2 px but less than
`--tolerance` (default 60; beyond that it reads as a deliberately short row).

The content mask treats the box's median colour as background, so keep the
box on one plane. A box over the TRANSPORT tiles reads as a single band.

## `strokes --box`: is this indicator one mark?

```bash
python3 scripts/gui_qa_measure.py strokes target/gui-qa-offscreen/running.png --box 616,856,808,880 --color ACCENT_ORANGE
```

The design has no outlines, so use this on single-colour bars: the playhead
LED above reads as one 8 px horizontal segment (`LED_H` 4 pt), and a
separator is one 2 px `WELL_BG` line. Several thicknesses in one indicator
mean separate marks are touching. A segment of the same colour in the same
box on a step that is not playing means the indicator leaks.

`--color` takes a token name or `#RRGGBB`. Lower `--coverage` if the mark does
not span the whole box.

## `crop`: hand a region to vision

```bash
python3 scripts/gui_qa_measure.py crop target/gui-qa-offscreen/running.png \
    --box 600,850,830,1070 --scale 3 -o /tmp/step04.png
```

Then Read the output. Scale 3 or more uses nearest-neighbour, so stroke
widths stay countable; below that it uses Lanczos, so text stays readable.
Compose two states side by side: crop each and read both in one turn.

Both capture scripts already run `scripts/crop_gui_sections.py`, which writes
`<state>_crop_{header,voice,engine,generator,sequencer}.png` per state (the
`latest` set also without the prefix), `crop_right_margin.png`,
`crop_bottom_margin.png`, and `crop_state_diff.png` (the engine rows of
`latest`, `accurate` and `running` stacked).

---

## Machine-readable output

Every command except `crop` accepts `--json`, for diffing one round's numbers
against the next.

## When a number and the image disagree

The image wins, but find out why the number lied before you write the finding;
the reason is usually a second defect. A mismatch traced to a stale token, a
misplaced probe box or a wrong card split is a tool problem: fix the tool and
note it in the round's memory. A mismatch that survives that is real.
