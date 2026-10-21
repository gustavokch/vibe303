# Report template

Write to `docs/superpowers/plans/<YYYY-MM-DD>-gui-audit-round<N>-report.md`.
Round numbers continue from the highest report in that folder.

Issue numbers assigned here are permanent: the fix prompt and the fixing
agent's confirmation both reference them.

---

```markdown
# TECHNEO 303 — GUI Visual Audit, Round <N>

Date: <YYYY-MM-DD>
Captures: `<dir>` (<files>), all <W>x<H>, <offscreen render | window capture>.
Baseline: commit `<sha>` "<subject>" on `<branch>`.

## Capture set

| File | State |
|---|---|
| `latest.png` | <the state this capture holds> |

Pixel diffs confirm the states are distinct and correctly scoped: <state>
differs from `latest` only in <card> (y …, x …).

## Previous round

- **<n> <item>: FIXED.** <the measurement that proves it, not an assertion>
- **<n> <item>: OPEN.** <what still measures wrong>
- **<n> <item>: SUPERSEDED by `<sha>`.** <the widget or layout it was about no
  longer exists>

## Geometry

Cards match the contract's landmark table: <yes | the cards that differ, with
their measured boxes>.

---

## HIGH

### 1. <One line naming the problem, not the fix>

**Kind.** Defect, drift (cite the contract rule it breaks), or design (cite
the rule it would change, or say that no rule covers it).

**Symptom.** What is wrong, where, in which captures, with the measurement
that proves it. Quote displayed text and values. Name the source line when
you found it.

**Required fix.** For a defect or drift: what correct looks like, as
acceptance criteria, not a patch. For a design finding: the proposal, and
whether it stays in the current direction. Name what must NOT change
alongside it.

---

## MEDIUM

### <n>. <problem>

Same shape, shorter. A measurement table where several instances share a
class:

| Control | Measurement |
|---|---|
| … | … |

---

## LOW

<n>. <problem, one line each, with its kind>

---

## Flagged, intent unclear — not asserted as defects

- <observation>, and what would confirm intent (a tooltip, a source comment,
  a deliberate call). Say which part IS still a defect if any part is.

## Capture gaps for the next round

- <state combination the set does not cover>
- <the state that would have proven finding N, and did not exist>

## Most urgent

<issue numbers>, in order, with a half-line each on why.
```

---

## Rules

- **Order by severity, not by region.** The reader wants the worst first.
- **Every finding carries its evidence.** A contrast claim carries the ratio
  from `text --box`, a dead-space claim the pixel run, a knob claim the
  pointer angle and the arc tip. A finding without a number is an opinion.
- **Name the rule.** Drift cites the contract row it departs from, so the fix
  restores the rule instead of inventing a new one. A design finding names
  the rule it would change; the user decides whether it changes.
- **Design findings need the same evidence.** A crop, a 1x view or a number
  that shows what a user cannot see, tell apart or find. Taste without
  evidence is not a finding.
- **Separate defect from observation.** If the source shows a choice was
  deliberate, say so and narrow the finding to the part that is still wrong.
- **Account for every open item of the previous round.** A fix that regressed
  is worse than one never made. After a redesign, close an item as
  superseded only when its widget is gone; an item whose widget survived is
  re-measured.
- **Name the capture gaps.** A finding you cannot verify is not finished, and
  the next round needs to know what to capture.
- Be exhaustive. Minor inconsistencies belong in the report alongside critical
  ones.
