---
name: gui-visual-audit
description: Use when auditing the TECHNEO 303 editor for visual defects, running a GUI QA round, reviewing GUI screenshots for clipping, dead space, contrast, control/value mismatches or inconsistent states, or writing a fix prompt for the defects a round found. Also use for a one-off visual review of any UI screenshots the user supplies.
---

# GUI Visual Audit

Find every visual defect in a UI, then hand back a report and a fix prompt a
coding agent can execute. Measurement first, vision second: the numbers find
what the eye slides past, and the eye finds what no check was written for.

**Never OCR the screenshots.** Read them with vision.

## Quick reference

| Phase | Command | Output |
|---|---|---|
| 1 Capture | `scripts/capture_standalone_gui.sh` | `target/gui-qa/*.png` |
| 2 Measure | `scripts/gui_qa_measure.py report target/gui-qa` | numeric findings |
| 3 Look | `... crop <img> --box … --scale 3 -o …`, then Read | confirmed/new defects |
| 4 Report | `docs/superpowers/plans/<date>-gui-audit-round<N>-report.md` | numbered findings |
| 5 Fix prompt | `docs/superpowers/plans/<date>-gui-audit-round<N>-fix-prompt.md` | agent-ready prompt |

Read [references/audit-checklist.md](references/audit-checklist.md) before
Phase 3 — it is the defect taxonomy and the per-region sweep.
Read [references/measurement-recipes.md](references/measurement-recipes.md)
before Phase 2 — it says what each number means and which readings lie.

## Phase 1 — Capture

Run `scripts/capture_standalone_gui.sh`. It builds the standalone binary and
captures one PNG per `VIBE303_QA_STATE` (see `plugin303/src/params.rs`).

**Verify every capture is 2732 px wide.** A wrong window id silently yields a
different width and every coordinate below becomes meaningless. `report` checks
this first and says so.

If the user supplied screenshots instead, skip to Phase 2 and take their
dimensions as given.

**A state you cannot capture is a defect you cannot find.** Before auditing,
list the state combinations the set does *not* cover and say so in the report.
Round 12 could not verify its top finding because no capture had a knob away
from its default.

## Phase 2 — Measure

```bash
scripts/gui_qa_measure.py report target/gui-qa
```

That runs, over every capture: state diffs, panel geometry and inter-panel
gaps, banded dead space per panel and per column, the colour inventory with
contrast ratios, knob arc-versus-needle agreement, and accent presence per
panel per state. Then use the focused commands on whatever it flags —
`deadspace`, `colors`, `knobs`, `text --box`, `edges --box`, `diff`, `crop`.

Two readings deserve special attention because they encode findings vision
misses entirely:

- **`accent`** — a panel with accent pixels in one state and zero in another
  has usually lost its *selection indicator*, not its selection.
- **`knobs`** — `arc_needle_delta_deg` above 4, or a knob whose arc is absent
  while a sibling at the same value has one, means the widget disagrees with
  its own displayed value.

## Phase 3 — Look

Read the full frame of every state first, then crop what Phase 2 flagged at
3x or more and read those. Sweep each region against
[references/audit-checklist.md](references/audit-checklist.md).

Compare states pairwise and deliberately:

- What *should* change (selection, mode overlays, transport) — is it correct?
- What *should not* change (geometry, spacing, padding) — did it shift?
- Does a claimed dynamic state show live feedback? Absence is a defect.

Judge intent. Clipping, a value/position contradiction, and dead space are
defects. An unusual unit or a quirky label is a *flagged observation*. When the
source explains a choice (a tooltip, a deliberate `add_enabled_ui`), say so and
narrow the finding to the part that is still wrong.

## Phase 4 — Report

Write to `docs/superpowers/plans/<date>-gui-audit-round<N>-report.md` following
[references/report-template.md](references/report-template.md). Order by
severity. Every finding names what is wrong, where, which captures show it, and
the measurement that proves it. Verify the previous round's fixes and say which
held. Close with the capture gaps and a "most urgent" list.

## Phase 5 — Fix prompt

Only when the user asks. Follow
[references/fix-prompt-template.md](references/fix-prompt-template.md).
**Keep the issue numbering identical to the report** so the agent's
confirmation maps back one-to-one.

## Phase 6 — Record

Update the round-status memory file and its `MEMORY.md` line: what was verified
fixed, what remains open with file:line where known, and any measurement gotcha
worth not rediscovering.

## Common mistakes

| Mistake | Consequence |
|---|---|
| Auditing before checking capture width | Every coordinate in the report is wrong |
| Treating two captures of one screen as redundant | State-comparison defects go unfound |
| Reporting "no arc" as a knob defect without checking polarity | The arc may be correctly zero-length at the detent |
| Refining a knob centre by centroid | The centroid drags into the label below; angles come out wrong. Use the tool's ring fit |
| Claiming a dead-space defect from a title row | Title rows are short by design; the tool tags them |
| Asserting intent from a screenshot alone | Grep the source; a disabled group is often deliberate, its lost indicator is not |
| Renumbering issues in the fix prompt | The agent's confirmation no longer maps to the audit |
