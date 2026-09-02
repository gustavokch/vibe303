# Fix prompt template

Write to `docs/superpowers/plans/<YYYY-MM-DD>-gui-audit-round<N>-fix-prompt.md`.

The output is a self-contained prompt: the agent receiving it has not seen the
captures and will not run the audit. Everything it needs is in the document.

**Keep the report's issue numbers.** Renumbering breaks the mapping between the
audit and the agent's confirmation.

---

```markdown
# <App> — GUI remediation prompt, round <N>

Source audit: `docs/superpowers/plans/<date>-gui-audit-round<N>-report.md`.
Issue numbers below match that report exactly. Keep them stable in your
confirmation.

---

## Context

<What the app is, what framework the editor uses, and where the source lives.>
The window captures at <W>x<H> physical pixels (<scale>x DPI, so <w>x<h>
logical). All coordinates in this prompt are **physical** pixels from the
captures in `<dir>`.

### UI regions, top to bottom

1. **<Region>** (y <top>-<bottom>) — <contents>. Source: `<path>`.

---

## Guardrails

- **Presentation layer only.** Do not change <behaviour, ranges, defaults,
  formatting, persistence>. A control that reads `X` must still read `X`.
- Do not add, remove, or reorder controls except where an issue explicitly asks
  (issues <n>, <n>).
- Do not regress any earlier fix. In particular: <the specific invariants
  previous rounds established, each stated as a testable condition>.
- Prefer fixing the shared widget or token over patching each call site, except
  where the issue names specific call sites.
- Keep <formatter> and <linter> clean. Existing tests must pass.

---

## HIGH

### <n>. <defect, one line>

**Symptom.** The observed behaviour with its measurements, the source line if
known, and what the reader would see in which capture.

**Required fix.** The acceptance criteria. Not "fix X" — what correct looks
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

Re-render the full capture set after the changes, at <W>x<H>, and confirm the
width is <W> — a wrong window id silently produces a different width.

**States to capture:**

1. <existing state> — the baseline.
2. **New:** <state that exposes issue N directly> — without it the fix is
   unverifiable.

**Per-issue confirmation.** Report each issue number with what you changed and
the measurement that proves it, or state explicitly that you did not fix it and
why:

- **<n>** — <the specific number or region that settles it>.

**Regression checks.** Confirm in the new captures that <each invariant from
Guardrails still holds>, and that <formatter>, <linter> and the test suite pass.
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
- **Never ask for a behaviour change in a presentation-only prompt.** If a
  finding needs one, say so explicitly and let the user decide.
