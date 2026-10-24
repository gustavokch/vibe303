# Audit checklist

The taxonomy of defects, drift and design weaknesses, and the per-region
sweep. Read before Phase 3, together with [design-contract.md](design-contract.md):
the taxonomy names what to look for, the contract records the current rules.
Sections A-I find defects and drift; section J questions the design itself.

Each entry names the symptom, not the cause. Attribute every observation to a
device card so the report can be located in the source.

---

## A. Containment and clipping

- Content crossing a card edge onto the chassis, or a window edge.
- Text truncated, ellipsised, or cut mid-glyph.
- A value wider than its pill, well or tile (knob pills are 52 / 38 pt wide;
  a long readout such as `-100.0 ct` is the usual offender).
- A child box wider or taller than its parent.
- Content flush against a card edge that elsewhere has the 8 pt / 4 pt body
  inset.
- A scrollbar where the layout should have fit.

## B. Dead space and balance

- A run wider than 600 px (300 pt) inside one row band of a body with no
  content and no separator. The app bar's middle is exempt.
- A bank that does not spread across its body, or that clumps to one side.
- Content that stops well above its card floor while a neighbour is dense.
- A gutter other than 6 px between any two cards, or a 6 px gutter that
  turns into something else on one side.
- Content that stops short of the window bottom: the layout fills 640 pt
  exactly.

## C. Value versus position contradictions

The highest-value class. Read the *displayed value* and compare it to the
*visual position*.

- A knob pointer angle disagreeing with the value text.
- A knob arc whose tip is not at the pointer.
- An arc filling from the wrong origin: a range whose plain minimum is 0
  fills from the minimum, a range below 0 fills from 0.
- A fader fill height disagreeing with its value.
- A segment or lit action whose fill contradicts its label, or a toggle LED
  that contradicts its state (RUN unlit while running, SYNTH lit while muted).
- A playhead plate or LED on a step other than the one playing.

## D. State correctness

Requires two or more captures of the same screen.

- Something that should change between states did not.
- Something that should not change did: a card moved, a gutter changed, a
  bank re-flowed.
- A running state with no live feedback: no orange LED, no plate, no bright
  step number.
- Orange on a disabled control.
- A disabled selection that no longer shows which option is selected.
- A bypassed or inert device that does not say so: no muted title, no
  banner, no tooltip.
- A placeholder that replaces content and drops the card's title or framing.
- More than one element marked as "current" when only one can be.

## E. Colour, contrast, and palette

- Enabled text below 4.5:1; any text below 3:1; a non-text mark (arc, fill,
  pointer, arrow) below 3:1.
- A colour that matches no theme token, beyond antialiasing neighbours.
- A third hue; orange that means something other than "live"; blue
  anywhere but a lit SLD.
- A stroke, outline, shadow or gradient. The only line is the 1 pt
  `WELL_BG` separator.
- A plane out of order: a well lighter than the body, a raised control darker
  than its parent.
- A value drawn raised, or an action drawn inset.
- `TEXT_MUTED` as enabled text off its safe planes.

## F. Typography

- A numeric readout in the proportional face, or a word in monospace.
- A size that matches no row of the contract's type table for its role.
- A widget at egui's 14 pt default (about 20 px glyph height at 2x): popups,
  tooltips and new combo boxes are the usual offenders.
- Device titles at more than one size.
- A label larger than the value it labels.
- Inconsistent number formatting: zero padding, decimal places, unit spacing,
  sign on zero (`+0.0` versus `0`).

## G. Alignment and rhythm

- A card edge off the shared seam (x 1614 | 1620 at 2x).
- Rows in a column that do not share a left or right edge.
- A label not centred on the knob, fader or pill it names.
- Control heights that vary within one row: the header strip runs on 18 pt,
  TRANSPORT on 28 pt, the step column on 21 pt.
- Header-strip contents not vertically centred in the strip.
- Baselines that do not sit on a common line.

## H. Component consistency

- A boolean that is not a `ToggleButton`.
- An enum choice that is not a single segmented bar, or a bar drawn as
  detached buttons.
- A typed value outside a well.
- A knob outside the two tiers, or a pill width that does not match its tier.
- A second dimming mechanism: something faded by egui on top of its token.
- Separators used to group in one card and absent in a structurally identical
  one.
- A duplicated title or label.

## I. Affordance and feedback

- No hover, pressed or disabled treatment where one is expected. Hover and
  press need a window capture with the pointer in place; the offscreen
  render shows neither.
- An abbreviated or icon-only control without a tooltip.
- A destructive action (CLEAR) that reads exactly like a benign one.
- A status string with no visible link to the control it reports on.

## J. Design weaknesses

The design itself, beyond its rules. Each finding needs evidence (a
measurement, a side-by-side crop, the window at 1x) and a proposal that says
whether it stays in the current direction.

- **Hierarchy**: the controls used most (the VOICE knobs, the step grid,
  RUN) do not draw the eye first, or a secondary control outweighs a primary
  one in size, brightness or orange area.
- **Accent budget**: large orange areas (a full-width lit action, lit tiles at
  rest) compete with the marks that show live state, so "orange = live" stops
  reading.
- **Grouping**: related controls do not read as one group, or unrelated ones
  do; a group boundary rests on spacing the eye misses.
- **Separation**: a control, well or card that cannot be told from its
  surroundings at 1x.
- **State legibility**: two states of one control that look alike: on and
  off, current and other step, a disabled selection and its disabled
  neighbours (`SURFACE_HOVER` against `SURFACE_PRESSED` is 1.47:1; check that
  it reads).
- **Affordance**: a label that looks clickable, a tile that looks inert, a
  draggable value with no cue.
- **Size at 1x**: text or targets that work in a 3x crop but not at the real
  size, where an 8.5 pt caption is 8.5 px tall.
- **Density and balance**: one card crowded while its neighbour idles, empty
  space with no purpose.
- **Naming**: an abbreviation a user must guess, two words for one concept,
  a caption that names the implementation instead of the effect.
- **Weight**: one role drawn with different visual weight in different cards.

---

## Per-region sweep

Walk the rows top to bottom; in each row take the left card, then the right:
app bar · VOICE | ENVELOPES · SOUPED-UP ENGINE and OUTPUT CLIPPER |
ANALOG TWEAKS · ACID EUCLIDEAN GENERATOR | GENERATION · SEQUENCER |
TRANSPORT & PATTERN. For each card, ask in order:

1. **Edges**: trace all four. Anything clipped, anything flush?
2. **Fill**: is there a dead run? Does the bank spread across the body?
3. **Values**: for every control with both a value and a position, do they
   agree?
4. **Contract**: does each component here match its row in the contract?
   Any stroke, any off-role colour?
5. **Palette**: any colour here that appears nowhere else, or any dim text?
6. **Type**: does each text run match its role's face and size?
7. **Alignment**: do rows share edges, do labels centre on their controls?
8. **Across states**: what changed here between captures, and should it have?
9. **Design**: does the card read well on its own and beside its neighbours?
   Use section J.

Then make one pass over the whole window at 1x for section J: where the eye
lands first, whether orange marks only what is live and important, which
card is crowded and which idle.

## Severity

| Level | Test |
|---|---|
| HIGH | Wrong or unreadable information: a value/position contradiction, a lost state indicator, orange on a disabled control, text below 3:1, clipped content, a large dead region, two states of one control that cannot be told apart |
| MEDIUM | Visible inconsistency, drift, or a design weakness a user would feel: a stroke or outline, an off-role hue, a checkbox, a raised value field, mixed type sizes, enabled text below 4.5:1, a card off the seam, misaligned rows, weak hierarchy, a group that does not read as one, an unclear affordance |
| LOW | Spacing and margin asymmetries under 2 pt, duplicated labels, separator conventions, design polish |
| Flagged | Possibly intentional. Report the observation and what would confirm intent; do not assert a defect |
