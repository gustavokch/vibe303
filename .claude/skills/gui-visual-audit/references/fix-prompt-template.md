# Fix prompt template

Write to `docs/superpowers/plans/<YYYY-MM-DD>-gui-audit-round<N>-fix-prompt.md`.

The output is a self-contained prompt: the agent receiving it has not seen the
captures and will not run the audit. Everything it needs is in the document.

**Keep the report's issue numbers.** Renumbering breaks the mapping between the
audit and the agent's confirmation.

---

```markdown
# TECHNEO 303 — GUI remediation prompt, round <N>

Source audit: `docs/superpowers/plans/<date>-gui-audit-round<N>-report.md`.
Issue numbers below match that report exactly. Keep them stable in your
confirmation.

---

## Context

The editor is an egui UI (nih_plug_egui) in `plugin303/src/gui/`: tokens in
`theme.rs`, layout in `mod.rs`, components in `widgets/`, devices in
`panels/`. The window is 1216 x 640 pt; the captures are 2432 x 1280
physical px (2x), from `<dir>`. All coordinates in this prompt are physical
pixels in those captures.

### Devices, top to bottom (left | right)

1. **<Device>** (x …, y …): <contents>. Source: `<path>`.

---

## Design contract

The flat device-chain design is the current direction, not a finished design.
Restore its rules where the render drifted. Change a rule only where a design
issue below says so (the user selected those), and update
`.claude/skills/gui-visual-audit/references/design-contract.md` in the same
change. The rules that bear on these fixes:

- <each contract rule an issue touches, stated as a testable condition>
- No strokes, outlines, shadows or gradients, and no new hue or token, unless
  an issue asks for one. Reuse the existing widgets and tokens.

## Guardrails

- **Presentation layer only.** Do not change <behaviour, ranges, defaults,
  formatting, persistence>. A control that reads `X` must still read `X`.
- Do not add, remove, or reorder controls except where an issue explicitly asks
  (issues <n>, <n>).
- Do not regress any earlier fix. In particular: <the invariants previous
  rounds established, each stated as a testable condition>.
- Prefer fixing the shared widget or token over patching each call site, except
  where the issue names specific call sites.
- Keep `cargo fmt` and `cargo clippy` clean. `cargo test -p plugin303 --release`
  must pass, including `tests/gui_layout_test.rs` (window filled, no overflow,
  transport seam) and the theme and widget unit tests.

---

## HIGH

### <n>. <defect, one line>

**Symptom.** The observed behaviour with its measurements, the source line if
known, and what the reader would see in which capture.

**Required fix.** The acceptance criteria. Not "fix X": what correct looks
like, and what must stay unchanged beside it. Where a decision has cases,
enumerate them:

- **<case A>**: <the members>.
- **<case B>**: <the members>.

**Acceptance criteria.** The condition that decides whether this is done,
stated so it can be measured in a re-render.

---

## MEDIUM / LOW

<Same shape. LOW items may be one line each with a required outcome.>

---

## Verification

Re-render the full set with `scripts/render_gui_offscreen.sh` and confirm
every capture is 2432 px wide. Use `scripts/capture_standalone_gui.sh` as well
when an issue concerns the window itself (title bar, zoom, GL text).

**States to capture:**

1. <existing state>: the baseline.
2. **New:** <state that exposes issue N directly>: without it the fix is
   unverifiable. Add it to `QaState` (`plugin303/src/params.rs`), the
   `STATES` table in `tools/gui_render/src/main.rs`,
   `scripts/capture_standalone_gui.sh` and `scripts/crop_gui_sections.py`;
   they must stay in sync.

**Per-issue confirmation.** Report each issue number with what you changed and
the measurement that proves it, or state explicitly that you did not fix it and
why:

- **<n>**: <the specific number or region that settles it>.

**Regression checks.** Confirm in the new captures that the design contract and
<each guardrail invariant> still hold, and that `python3
scripts/gui_qa_measure.py report <dir>` shows 10 cards with 6 px gutters and
no new knob mismatch.
```

---

## Rules

- **Acceptance criteria, not instructions.** "All rows share one right edge,
  differing by at most 2 px" beats "align the rows".
- **Say what must not change.** Every fix has a neighbouring property that a
  careless change would break; name it.
- **Ask for the missing capture.** If a finding could not be verified because
  no state showed it, requiring that state is part of the fix.
- **Give the source line where the audit found one.** It converts a search into
  an edit.
- **Group the requests, not the symptoms.** Several instances of one root cause
  are one issue with a table, not five issues.
- **Never ask for a behaviour change in a presentation-only prompt, and
  include a design change only if the user selected it.** If a finding needs
  either, say so explicitly and let the user decide.
