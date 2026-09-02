# Audit checklist

The defect taxonomy and the per-region sweep. Read before Phase 3.

Each entry names the symptom, not the cause. Attribute every observation to a
region so the report can be located in the source.

---

## A. Containment and clipping

- Content crossing a panel, window, or group boundary.
- Text truncated, ellipsised, or cut mid-glyph.
- A child box wider or taller than its parent.
- Zero margin: content flush against a frame that elsewhere has padding.
- Content that only fits at the captured zoom level.
- A scrollbar where the layout should have fit.

## B. Dead space and balance

- A horizontal run inside a panel wider than ~600 px with no content and no
  divider. (Title rows are exempt — they are short by design.)
- A column or sub-column whose content stops well above the panel floor.
- A panel whose fill is a few percent while a neighbour is dense.
- Content clumped to one side of the column it nominally occupies.
- Asymmetric outer margins: left vs right, top vs bottom.
- Inter-panel gutters that differ between panel pairs.

## C. Value versus position contradictions

The highest-value class. Read the *displayed value* and compare it to the
*visual position*.

- Knob needle angle disagreeing with the value text.
- Knob value arc ending somewhere other than the needle.
- An arc filling from the wrong origin: a 0-100 % control must fill from its
  minimum, a bipolar control from its centre detent.
- Slider or fader handle position disagreeing with its value.
- Toggle, checkbox, or segmented control whose rendered state contradicts its
  label.
- A meter or LED that does not move when the value behind it does.

## D. State correctness

Requires two or more captures of the same screen.

- Something that should change between states did not.
- Something that should not change between states did: geometry shifted,
  spacing collapsed, a panel resized.
- A running or active state with no live feedback — no playhead, no highlight,
  no animation.
- A disabled group that lost its **selection indicator** along with its
  interactivity, so the current value is no longer visible.
- A placeholder replacing real content that drops the region's heading, title,
  or framing.
- More than one element marked as "current" when only one can be.

## E. Colour, contrast, and palette

- Any foreground colour below 4.5:1 against its background; below 3:1 is a
  hard defect even for disabled content.
- More than one muted grey, or more than one disabled grey, in the palette.
- A colour that matches no theme token — palette drift.
- Accent used for two different meanings, or two accents used for one meaning.
- Selected-state text colour that differs between otherwise identical
  segmented controls.
- A checked-state treatment applied to one checkbox and not its siblings.

## F. Typography

- Glyph heights that differ between controls of the same class.
- One widget family rendering at the framework default while everything around
  it uses the theme font — combo boxes are the usual offender.
- Panel titles at more than one size for the same nesting level.
- A label larger than the value it labels.
- Inconsistent number formatting: zero padding, decimal places, unit spacing,
  unit style.

## G. Alignment and rhythm

- Rows in a column that do not share a right edge.
- Field left edges that do not align within a form column.
- A label not centred on the control it names.
- Control heights that vary within one row.
- Baselines that do not sit on a common line.
- Grid insets that differ left versus right.

## H. Component consistency

- One segmented control drawn as detached buttons while its peers use a single
  grouped container.
- Two visual treatments for the same interaction.
- Dividers used to group in one panel and absent in a structurally identical
  one.
- An indicator composed of several marks of differing weight that visually
  merge.
- A duplicated title or label.

## I. Affordance and feedback

- No hover, focus, pressed, or disabled treatment where one is expected.
- A destructive action styled identically to a benign one.
- A status string with no visible relationship to the control it reports on.

---

## Per-region sweep

Walk the regions top to bottom. For each one, ask in order:

1. **Edges** — trace all four. Anything clipped, anything flush?
2. **Fill** — is there a dead run? Does content reach the region's floor?
3. **Values** — for every control with both a value and a position, do they
   agree?
4. **Palette** — any colour here that appears nowhere else, or any dim text?
5. **Type** — do sibling controls share a glyph size?
6. **Alignment** — do rows share edges, do labels centre on their controls?
7. **Across states** — what changed here between captures, and should it have?

## Severity

| Level | Test |
|---|---|
| HIGH | Wrong or unreadable information: a value/position contradiction, a lost state indicator, text below 3:1, clipped content, a large dead region |
| MEDIUM | Visible inconsistency a user would notice: mixed font sizes, mixed disabled greys, misaligned rows, one component styled unlike its peers |
| LOW | Spacing and margin asymmetries, duplicated labels, divider conventions |
| Flagged | Possibly intentional. Report the observation and what would confirm intent — do not assert a defect |
