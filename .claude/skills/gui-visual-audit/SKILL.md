---
name: gui-visual-audit
description: Use when auditing the TECHNEO 303 editor for visual defects, design weaknesses or drift from its design rules, running a GUI QA round, reviewing GUI captures for clipping, dead space, contrast, value/position mismatches or inconsistent states, or writing a fix prompt for a round's findings. Also use for a one-off visual review of UI screenshots the user supplies.
---

# GUI Visual Audit

The flat device-chain restyle (`56b383e`) set the editor's current design
direction. It is better than what came before, and it is not finished. An
audit finds three kinds of problem:

- **Defects**: the render shows wrong or unreadable information.
- **Drift**: the render breaks one of the design's own rules, recorded in the
  design contract.
- **Design weaknesses**: the design itself falls short, through one of its
  rules or in a place no rule covers.

Then it hands back a report and, on request, a fix prompt a coding agent can
execute. Measure first, look second: the numbers find what the eye slides
past, and the eye finds what no check was written for.

**Read the screenshots with vision. Never OCR them.**

**Judge against the current direction, not the old one.** Rounds 1-19
audited a bordered, multi-hue design. Their conventions (a 3:1 border on every
field, a checkbox family, a white playhead frame) no longer apply. A design
finding needs evidence of what a user cannot see, tell apart or find, plus a
concrete proposal. "Add a border" with no legibility failure behind it is a
preference, not a finding. A proposal may leave the direction (an outline, a
new hue, a new grid); say so, because the user decides those.

## Quick reference

| Phase | Command | Output |
|---|---|---|
| 1 Capture | `bash scripts/render_gui_offscreen.sh` | `target/gui-qa-offscreen/*.png` |
| 2 Measure | `python3 scripts/gui_qa_measure.py report target/gui-qa-offscreen` | numeric findings |
| 3 Look | `… crop <img> --box … --scale 3 -o …`, then Read | confirmed and new defects |
| 4 Report | `docs/superpowers/plans/<date>-gui-audit-round<N>-report.md` | numbered findings |
| 5 Fix prompt | `docs/superpowers/plans/<date>-gui-audit-round<N>-fix-prompt.md` | agent-ready prompt |
| 6 Record | round-status memory | carried state |

References:

- [references/design-contract.md](references/design-contract.md): read first;
  every later phase checks against it. Planes, colour roles, contrast,
  dimming, components, type, and the layout landmarks.
- [references/measurement-recipes.md](references/measurement-recipes.md):
  read before Phase 2. What each number means and which readings lie.
- [references/audit-checklist.md](references/audit-checklist.md): read
  before Phase 3. The defect taxonomy, the per-card sweep, severity.
- [references/report-template.md](references/report-template.md) and
  [references/fix-prompt-template.md](references/fix-prompt-template.md):
  Phases 4 and 5.

## Phase 1 — Capture

Run `bash scripts/render_gui_offscreen.sh`. It draws the real `render_editor`
through wgpu, one PNG per `VIBE303_QA_STATE`, into `target/gui-qa-offscreen`,
and cuts the section crops: a few seconds once built (the first build
compiles wgpu, about 45 s). There is no window,
so it needs no Screen Recording permission and has no pointer, focus or
tooltip artifacts.

Use the window capture, `scripts/capture_standalone_gui.sh` →
`target/gui-qa/`, when the round must judge what a host shows: the title bar,
GL text rasterisation, hover under a real pointer. It
needs Screen Recording permission. Its frames carry the title bar on top, so
subtract it before you compare coordinates with the offscreen set. Say in the
report which source each finding came from.

**Verify every capture is 2432 px wide** (2 × `GUI_WIDTH`). A wrong window
id silently yields another width, and every coordinate becomes meaningless.
`report` checks this first.

If the user supplied screenshots instead, skip to Phase 2 and take their
dimensions as given.

| File | Exercises |
|---|---|
| `latest.png` | defaults: FUN, QUALITY DRAFT, stopped |
| `accurate.png` | ACCURATE: SOUPED-UP banner, ANALOG TWEAKS, QUALITY and the ADV ENV tile disabled |
| `advanced.png` | ADV ENV on: both ADSR banks enabled |
| `running.png` | running, playhead pinned on step 04, a REST step |
| `running_note.png` | running, every step NOTE: the playhead over a lit gate |
| `knobs.png` | TUNING +25 ct (bipolar, off its detent), CUTOFF 0.75 |
| `quality_low.png` | QUALITY LOW |
| `generator.png` | every generator field off its default, SLD CHAIN on |

`quality_high` and `quality_best` exist in `QaState`, but neither script
renders them; capture them by hand when a round needs them.

**A state you cannot capture is a defect you cannot find.** Before auditing,
list what the set does not cover and put it in the report. Standing gaps:
hover, press and keyboard focus; open ComboBox and piano-roll popups; the SQR
and SWP waves; TIE under the playhead; CALIBRATE when a rebuild is due; SYNTH
MUTE, MIDI ON, LINK off, OUTPUT CLIPPER off; QUALITY HIGH and BEST; ADV ENV in
ACCURATE.

## Phase 2 — Measure

```bash
python3 scripts/gui_qa_measure.py report target/gui-qa-offscreen
```

That runs, over every capture: state diffs, card geometry and gutters, banded
dead space, the colour inventory with contrast ratios, knob arc versus
pointer, and accent presence per card per state. Then use the focused
commands (`panels`, `deadspace`, `colors`, `knobs`, `text --box`,
`edges --box`, `strokes --box`, `diff`, `crop`) on whatever it flags.

Three readings carry findings that vision misses, and each needs the contract
to read:

- **`panels`**: 10 cards, every gutter 6 px, the seam at x 1614 | 1620.
  Anything else is layout drift.
- **`knobs`**: 27 knobs at 100%. A mismatch, or a fill origin that
  contradicts the parameter's range, is a value/position defect.
- **`accent`**: a disabled control loses the accent by design. The finding
  is orange on a disabled control, or no orange where an enabled selection
  exists.

Triage `colors` flags before they become findings. Most `ACCENT_ORANGE` flags
are lit fills, and a grey snapped to `TEXT_MUTED` is often the antialiased
edge of a brighter label. `text --box` on the glyphs decides.

## Phase 3 — Look

Read the full frame of every state first. Then crop what Phase 2 flagged at 3x
or more and read those. Sweep each card with the checklist against its rows
in the contract.

Compare states pairwise and deliberately:

- What *should* change (selection, mode overlays, transport): is it correct?
- What *should not* change (geometry, gutters, banks): did it move?
- Does the running state show live feedback (orange LED, plate, bright step
  number)? Absence is a defect.

Judge intent. Clipping, a value/position contradiction, unreadable text and
drift are defects. An unusual unit or a quirky label is a *flagged
observation*. When the source explains a choice (a tooltip, a doc comment, a
deliberate `add_enabled_ui`), say so and narrow the finding to the part that
is still wrong.

Then step back for the design pass in the checklist's section J. Read the
whole window at its real size (1216 px wide at 1x) as well as in crops. Ask
where the eye goes first, whether each group reads as a group, which card is
crowded and which is idle, and whether every control looks as clickable,
draggable or inert as it is. A weakness found there is a finding like any
other, with evidence and a proposal.

## Phase 4 — Report

Write to `docs/superpowers/plans/<date>-gui-audit-round<N>-report.md`
following [references/report-template.md](references/report-template.md).
Order by severity. Every finding names its kind (defect, drift or design),
what is wrong, where, which captures show it, the measurement that proves it,
and the contract rule it breaks or would change.
Account for every open item of the previous round. Round 19 predates the
restyle: close its items whose widget is gone as superseded by `56b383e` and
re-measure the rest. Close with the capture gaps and a "most urgent" list.

## Phase 5 — Fix prompt

Only when the user asks. Follow
[references/fix-prompt-template.md](references/fix-prompt-template.md).
**Keep the issue numbering identical to the report** so the agent's
confirmation maps back one-to-one, and carry the contract rules each issue
touches into the prompt. A design finding goes into the prompt only when the
user selects it, and its fix updates the contract in the same change.

## Phase 6 — Record

Update the round-status memory (`gui-audit-round<N>-status.md` and its line
in `MEMORY.md`): what was verified fixed, what remains open with file:line
where known, and any measurement gotcha worth not rediscovering. A gotcha that
is a tool problem gets fixed in `scripts/gui_qa_measure.py`, with a case in
`tests/test_gui_qa_measure.py`, not only noted. When the user changes the
design, update the contract in the same change.

## Common mistakes

| Mistake | Consequence |
|---|---|
| Auditing before checking capture width | Every coordinate in the report is wrong |
| Mixing window-capture and offscreen coordinates | Every y is off by the title bar |
| Proposing a border or a new hue with no legibility failure behind it | A preference, not a finding: show what the user cannot tell apart, then propose |
| Treating the contract as the finished design | It records the current rules, not their ceiling: design weaknesses go unreported |
| Reporting accent lost on a disabled control | The contract drops it there on purpose; check that the disabled selection still reads |
| Filing `ACCENT_ORANGE` "below 4.5:1" from `colors` | It is a lit fill: a non-text mark with a 3:1 floor |
| Filing a grey that `colors` snapped to `TEXT_MUTED` | It is often a label's antialiased edge; `text --box` decides |
| Treating two captures of one screen as redundant | State-comparison defects go unfound |
| Reporting `arc=NONE` without reading the value | The arc is correctly absent at a zero-length fill |
| Asserting intent from a screenshot alone | Grep the source: a disabled group is often deliberate, its unreadable text is not |
| Renumbering issues in the fix prompt | The agent's confirmation no longer maps to the audit |
