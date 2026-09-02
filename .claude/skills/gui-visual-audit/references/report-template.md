# Report template

Write to `docs/superpowers/plans/<YYYY-MM-DD>-gui-audit-round<N>-report.md`.

Issue numbers assigned here are permanent: the fix prompt and the fixing
agent's confirmation both reference them.

---

```markdown
# <App> — GUI Visual Audit, Round <N>

Date: <YYYY-MM-DD>
Captures: `target/gui-qa/` (<files>), all <W>x<H>.
Baseline: commit `<sha>` "<subject>".

## Capture set

| File | State |
|---|---|
| `latest.png` | <the state this capture holds> |

Pixel diffs confirm the states are distinct and correctly scoped: <state>
differs from <base> only inside <region> (y …, x …).

## Round <N-1> items verified fixed

- **<item>.** <the measurement that proves it, not an assertion>

## Layout map

1. <region> — y <top>-<bottom> — <contents>. Source: `<path>`.

---

## HIGH

### 1. <One line naming the defect, not the fix>

**Symptom.** What is wrong, where, in which captures, with the measurement
that proves it. Quote displayed text and values. Name the source line when
you found it.

**Required fix.** What correct looks like — the acceptance criteria, not a
patch. Name what must NOT change alongside it.

---

## MEDIUM

### <n>. <defect>

Same shape, shorter. A measurement table where several instances share a
class:

| Control | Measurement |
|---|---|
| … | … |

---

## LOW

<n>. <defect, one line each>

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
- **Every finding carries its evidence.** A contrast claim carries the ratio, a
  dead-space claim carries the pixel run, a knob claim carries both angles. A
  finding without a number is an opinion.
- **Separate defect from observation.** If the source shows a choice was
  deliberate, say so and narrow the finding to the part that is still wrong.
- **Verify the previous round.** A fix that regressed is worse than one never
  made, and the reader needs to know which held.
- **Name the capture gaps.** A finding you cannot verify is not finished, and
  the next round needs to know what to capture.
- Be exhaustive. Minor inconsistencies belong in the report alongside critical
  ones — that is what was asked for.
