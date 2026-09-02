# Measurement recipes

How to drive `scripts/gui_qa_measure.py` and how to read what it returns.
Read before Phase 2.

All coordinates are **physical** pixels. The editor captures at 2732x1984,
which is 1366x992 logical at 2x DPI, so a 1 px logical stroke measures 2 px
here.

The tool parses `plugin303/src/gui/theme.rs` at startup, so token names in its
output stay true as the palette changes. Pass `--theme <path>` if you run it
from outside the repo root.

---

## Start here

```bash
scripts/gui_qa_measure.py report target/gui-qa
```

Roughly 12 s for a four-state set. It runs every check and prints them in the
order the audit needs: capture set, state diffs, then per-capture geometry,
dead space and colours, then knobs, then accent presence.

Everything below is the focused version of one of those sections, for when the
report flags something and you need to look closer.

---

## Capture set

The first block lists each capture and its dimensions. **A width other than
2732 invalidates every later number** — the capture script attached to the
wrong window. Re-capture before doing anything else.

## `diff` — do these captures actually show different states?

```bash
scripts/gui_qa_measure.py diff target/gui-qa/latest.png target/gui-qa/running.png
```

Prints changed-region clusters as `y … x … n=…`.

- **IDENTICAL** means the harness produced two captures of one state. That is a
  capture-harness defect, and it hides every state-comparison finding behind
  it. Report it.
- A tightly bounded cluster is good news: it proves the state change is scoped.
  `advanced` differing only inside the envelope box means the ADV ENV toggle
  touches nothing it should not.
- A cluster spanning the whole window usually means a 1-2 px global shift, not
  a real change. Check a landmark's coordinate in both before calling it one.

## `panels` — geometry and gutters

```bash
scripts/gui_qa_measure.py panels target/gui-qa/latest.png
```

Reports each panel's box, the gap above it, and its dividers.

- `INCONSISTENT inter-panel gaps` lists the gutters. One value that differs
  from the rest is a spacing defect.
- `gap_above=0` means two panels share a single border line with no gutter at
  all, while their neighbours have one.
- Panels sharing a border are separated by their background colour, so a
  redesign that gives every panel the same background will merge them in this
  output. Fall back to `--json` and read the raw edges if that happens.

## `deadspace` — where the layout gave up

```bash
scripts/gui_qa_measure.py deadspace target/gui-qa/latest.png --min 600 --min-rows 120
```

Two kinds of finding:

- `DEAD COLUMNS … in band y …` — a horizontal run with no content, measured
  *within one row band* rather than across the whole panel, so a gap under a
  short row does not masquerade as one beside a full row. Bands 24 px or
  shorter are tagged `(title row, expected)`; ignore those.
- `DEAD ROWS` — reported for the panel and again for each column between
  vertical dividers, which is how a half-empty side column surfaces while the
  panel as a whole looks full.

`fill=` is a weak signal on its own: a panel of knobs is legitimately sparse.
Use it to compare structurally similar panels, not as a threshold.

## `colors` — palette drift and contrast

```bash
scripts/gui_qa_measure.py colors target/gui-qa/accurate.png
```

Per panel: the background, then every foreground colour above the pixel floor,
with its contrast ratio and its theme token.

- `NOT A THEME TOKEN` is the palette-drift signal. A handful of antialiasing
  neighbours is normal; a colour with a few hundred pixels and no token is a
  hard-coded value or a framework default leaking through.
- `<-- BELOW 3:1` on a foreground colour is a hard defect, disabled or not.
- Run it on **every** state. A disabled state has its own palette, and it is
  where the contrast floor gets breached.
- Surfaces, fills and frames are excluded from the contrast flags and hidden
  unless you pass `--show-surfaces`.
- Comparing the same panel across states finds duplicate tokens: three
  different greys for "dimmed" show up as three entries that no single token
  explains.

## `accent` — a selection that lost its indicator

```bash
scripts/gui_qa_measure.py accent target/gui-qa/*.png
```

Accent pixel count per panel per state. `<-- ACCENT LOST IN ONE STATE` means a
panel has accent in one capture and none in another.

The usual cause is a group wrapped in a disabled scope: the framework strips
the selected item's accent stroke along with its interactivity, so the current
value becomes invisible. Disabling is often intentional; losing the indicator
is not. **Check the source before writing the finding.**

Panel granularity is a first pass. When a panel's count merely *drops* rather
than reaching zero, one group inside it lost its indicator — narrow it with
`--box`:

```bash
scripts/gui_qa_measure.py accent target/gui-qa/latest.png target/gui-qa/accurate.png \
    --box 2140,100,2520,160
```

## `knobs` — arc versus needle

```bash
scripts/gui_qa_measure.py knobs target/gui-qa/latest.png --radii 20-44:2
```

Per knob: centre, radius, needle angle and its fraction of the sweep, and the
arc's angular extent, origin and disagreement with the needle.

The sweep is -135° to +135° clockwise from 12 o'clock. `needle_fraction` is the
needle's position in that sweep, so a knob reading 50 % should show 0.5.

- `<-- ARC/NEEDLE MISMATCH` (delta above 4°) is a hard defect.
- `from=min` versus `from=centre` is the polarity the widget *chose*. A
  0-100 % control filling `from=centre` is wrong even when its needle is right.
- `arc=NONE` on a knob sitting exactly at its fill origin is correct and
  expected. `arc=NONE` on a knob **away** from its origin is a defect. If most
  knobs report `arc=NONE`, check whether they are all parked at their defaults
  — and then capture a state where they are not, because that is the only way
  to test this.

Detection notes:

- Centres come from a ring-template fit, not a centroid. **Never refine a knob
  centre by taking the centroid of nearby pixels** — it drags down into the
  value label and produces angle errors of 10° or more, which read as false
  mismatches.
- The inactive arc track is itself a ring at 0.75 × radius, so the fit prefers
  the largest radius in a neighbourhood. A knob reported at three quarters of
  its real size would make the tool look for the arc in the wrong band.
- Expect a few false positives at `r=20` from dropdown arrow glyphs. They have
  no value pill beneath them; ignore them.
- Raise `--threshold` if the count is implausible, narrow `--radii` if it is
  slow.

## `text --box` — glyph sizes

```bash
scripts/gui_qa_measure.py text target/gui-qa/latest.png --box 2555,112,2680,158
```

Aim one probe at a single label, caption, or field and compare heights across
control classes. Reports the glyph bounding box — not the ink band, so a
control's own border does not inflate the number — plus foreground, background,
token and contrast.

A combo box whose text measures 18 px next to an 11 px button caption is a
widget that never received the theme font.

## `edges --box` — row alignment

```bash
scripts/gui_qa_measure.py edges target/gui-qa/latest.png --box 2118,1410,2705,1715
```

Splits the box into row bands and prints each band's left and right content
extent, then flags bands whose edge falls short of the extreme by more than
2 px but less than `--tolerance` (default 60 — beyond that it is a
deliberately short row, not a misalignment).

Point it at a column of stacked controls that should share an edge.

## `strokes --box` — is this indicator one mark or three?

```bash
scripts/gui_qa_measure.py strokes target/gui-qa/running.png --box 800,1370,1070,1660
```

Reports every horizontal and vertical segment of one colour inside the box,
with its thickness, and flags a set of differing weights.

An outline, underline, LED bar or playhead frame should be one weight.
Thicknesses of `[2, 10]` mean several separate marks are touching and reading
as a single uneven edge — the eye registers "heavy" without resolving why.

Run it against a box that should be empty (the same region on a non-current
item) as a control. Segments there mean the indicator is leaking.

`--color` takes a theme token name or `#RRGGBB`. Lower `--coverage` if the
indicator does not span the whole box.

## `crop` — hand a region to vision
scripts/gui_qa_measure.py crop target/gui-qa/running.png \
    --box 780,1375,1090,1650 --scale 3 -o /tmp/step04.png
```

Then Read the output. Scale 3 or more uses nearest-neighbour so stroke widths
stay countable; below that it uses Lanczos so text stays readable. Compose two
states side by side by cropping each and reading both in one turn.

---

## Machine-readable output

Every command except `crop` accepts `--json`, for diffing one round's numbers
against the next.

## When a number and the image disagree

The image wins, but find out why the number lied before writing the finding —
the reason is usually a second defect. A mismatch traced to a stale theme
token, a misplaced probe box, or a wrong panel split is a tool problem; fix the
tool. A mismatch that survives that is real.
