# Fix Prompt — TECHNEO 303 GUI Visual Audit Round 3+4 (2026-08-25)

## Status

Round-4 audit (same-day re-capture, 20:53) confirmed: **none of the round-3 fixes below have landed yet**. Issues 1–5 and 8–10 are still open and verbatim reproducible. Round 4 added four new observations (issues 11–14). Prior bottom-clip regression is confirmed fixed — octave arrows fully visible in all three states; do not regress this.

## Context

TECHNEO 303 is a bass-synth audio plugin (Rust, `plugin303/src/gui/`). Captures are 2732×1984 px @2x (≈1366×992 logical). Reference screenshots: `target/gui-qa/latest.png` (FUN topology, stopped), `target/gui-qa/running.png` (FUN, playing), `target/gui-qa/accurate.png` (ACCURATE topology).

UI regions, top to bottom:

1. **Header bar** — title/badges left; TOPOLOGY (ACCURATE/FUN), orphan "2x" box, QUALITY (LOW/HIGH/ULTRA), ZOOM right.
2. **Classic synth panel** — WAVE selector, 7 knobs (TUNING…VOLUME), ENVELOPES sub-panel (MEG ADSR, VEG ADSR, ADV ENV checkbox).
3. **SOUPED-UP ENGINE & ANALOG DRAWER** — VCF TAPS, VCA ENV, 9 knobs (DRIVE…R-COMP). In ACCURATE mode replaced by an "ACCURATE MODE ACTIVE" overlay.
4. **OUTPUT CLIPPER** — BYPASS, OS (2X/4X), 9 knobs, LINK, DIODE (SI/GE).
5. **ANALOG CHARACTER & TWEAKS** — TOL/AGE/NOISE/COMP SEED, DRIFT, RECAL, CALIBRATE.
6. **ACID EUCLIDEAN GENERATOR** — 4 parameter columns + GENERATE/MUTATE.
7. **Sequencer grid** — 16 steps × 2 rows (ACC/SLD, NOTE/TIE/REST, pitch, octave arrows).
8. **TRANSPORT & PATTERN** — RUN/STOP, SYNC, MIDI/SYNTH, SHIFT, BPM, LEN, rate dropdowns, JIT, CLEAR.

Issue numbers below match the audit report. Issues 6 and 7 from the audit are confirmed INTENDED — do not touch them (MIX2 naming, seed/zero-padding formats).

---

## Issue 1 — Orphaned unlabeled "2x" widget in header

Symptom: a gray "2x" box sits between the TOPOLOGY and QUALITY groups in region 1, with no group label. Lit in FUN mode, dimmed in ACCURATE mode, so it belongs to the souped-up engine, but it floats detached in the header.

Required fix:
- The "2x" control must be either (a) moved into the SOUPED-UP ENGINE & ANALOG DRAWER section (region 3) next to VCF TAPS / VCA ENV with a clear label, or (b) given an explicit group label in the header so its purpose and grouping are unambiguous.
- Its enabled/dimmed state must still track the topology exactly as today (lit in FUN, dimmed in ACCURATE).
- Header control order and spacing of the remaining groups (TOPOLOGY, QUALITY, ZOOM) must stay visually stable after removal/relabeling — no gaps left behind.

## Issue 2 — Dead space in OUTPUT CLIPPER, ANALOG CHARACTER & TWEAKS, ENVELOPES

Symptom: OUTPUT CLIPPER panel border ends at ~47% window width, right half of the row empty. ANALOG CHARACTER & TWEAKS content occupies left ~30% of a full-width panel. ENVELOPES sub-panel has MEG/VEG clusters at left, ADV ENV checkbox at far right, large empty band between.

Required fix:
- OUTPUT CLIPPER: rebalance so the panel does not leave a dead right half — either widen the panel content (larger knobs/spacing) to fill the row, or size the panel to its content and pair it with another element in the freed space. No empty area to the right of the panel border.
- ANALOG CHARACTER & TWEAKS: distribute content across the panel width or shrink the panel to fit content; no >40% empty horizontal area.
- ENVELOPES: close the gap — either place ADV ENV adjacent to the envelope clusters or distribute MEG/VEG/ADV ENV evenly across the sub-panel width.
- All controls must remain inside panel borders with margins matching the current left/right margin of other panels.

## Issue 3 — QUALITY LOW uses red accent instead of orange

Symptom: the selected QUALITY option (LOW) renders with a red/pink border, while every other selection accent in the UI is orange (FUN, 4-POLE, ENV, OS 2X, SI, ACC cells). Red elsewhere signals playhead/warning.

Required fix:
- The selected QUALITY option must use the same orange selection accent as every other selected toggle in the UI.
- Red must remain reserved for the running-state playhead highlight only.
- Verified in both FUN and ACCURATE states (in ACCURATE the group is dimmed — dimmed rendering must also follow the corrected accent, not red).

## Issue 4 — BPM displays decimal "120.0"

Symptom: TRANSPORT & PATTERN BPM field renders `120.0`. Commit 8cff6b8 reverted TEMPO display to integer BPM per the #12 consistency rule; this control still shows one decimal.

Required fix:
- BPM displays as an integer (`120`) in the TRANSPORT & PATTERN panel, matching the reverted TEMPO convention.
- Internal tempo precision must not change — presentation layer only.
- Stepper behavior (−/+) unchanged; displayed value rounded to integer at all times, including after +/− presses and host-sync tempo changes.

## Issue 5 — Section geometry shifts between topologies

Symptom: switching FUN ↔ ACCURATE replaces region 3 (souped-up engine) with a smaller "ACCURATE MODE ACTIVE" overlay, so OUTPUT CLIPPER and all sections below reflow upward ~100 px (confirmed in `crop_state_diff.png`).

Required fix:
- The ACCURATE-mode overlay must occupy the same vertical height as the FUN-mode souped-up section it replaces, so regions 4–8 keep identical Y positions across topologies.
- Overlay content stays centered within that fixed-height area.
- No layout shift anywhere below region 3 when toggling ACCURATE ↔ FUN.

## Issue 8 — ENVELOPES mini-ADSR value row cramped

Symptom: the value text under the MEG/VEG mini sliders (`3 ms`, `200 ms`, `0%`, `200 ms`) is tiny and crowds the slider thumbs (region 2, ENVELOPES sub-panel).

Required fix:
- Increase value-text size and/or vertical spacing so each A/D/S/R value is clearly legible and does not overlap or touch the slider thumbs.
- Values must remain aligned under their respective sliders after the change.

## Issue 9 — "JIT" checkbox label cryptic

Symptom: TRANSPORT & PATTERN has an unchecked box labeled only `JIT`, next to the rate dropdowns. Meaning (jitter) is not discoverable.

Required fix:
- Label the control unambiguously (e.g. `JITTER`) within available space, or add a tooltip on hover stating what it does.
- Control position and behavior unchanged.

## Issue 10 — Bottom-right dead space

Symptom: the area below TRANSPORT & PATTERN and right of sequencer rows 09–16 is empty to the window edge.

Required fix:
- Eliminate the void: either extend the sequencer/transport region to fill the window width and height, or rebalance the bottom row so no large empty rectangle remains at the bottom-right.
- Bottom and right margins must match the margins used elsewhere in the window.

## Issue 11 — CALIBRATE idle state uses strikethrough (round 4)

Symptom: the CALIBRATE button in region 5 renders its label struck-through when idle (`tweaks_panel.rs:159,176` — the strikethrough is deliberate in code, "marks the button as inert"). Strikethrough conventionally signals "removed/deprecated", not "temporarily inert", so the intent does not read.

Required fix:
- Replace the strikethrough with a conventional disabled/inert rendering: dimmed text and border (same dim treatment ACCURATE mode applies to disabled groups), no line through the text.
- The lit/active state (when aging/recal changes pending) must stay visually distinct and unchanged from today's lit rendering.
- Behavior unchanged: button still only triggers when armed.

## Issue 12 — Souped-up knob row uneven spacing (round 4)

Symptom: region 3's 9-knob row mixes two spacing regimes — DRIVE/SUB tight at far left, a center cluster (GLIDE, PITCH1, PITCH2), and MIX2/PW/SWEEP/R-COMP spread across the right — with no separator or group label marking the implied groups.

Required fix:
- Make the grouping legible: either equalize knob spacing across the row, or keep the two clusters and add a thin vertical separator or subgroup labels so the split reads as deliberate.
- Knob size, values, and order unchanged.

## Issue 13 — TOL vs AGE/NOISE affordance mismatch (round 4)

Symptom: in region 5, TOL renders as a dropdown (▼) while AGE and NOISE render as plain value boxes with no arrow, in the same row with the same box style.

Required fix:
- Give the row one consistent affordance: if AGE/NOISE are dropdowns, add the ▼ indicator; if they are text/numeric entries, make TOL visually match or otherwise distinguish entry vs. preset controls.
- Values, ranges, and behavior unchanged.

## Issue 14 — MUTATE button color breaks action-button convention (round 4, low priority)

Symptom: GENERATE renders orange (the UI's action/selection accent) while MUTATE renders cyan — the only cyan button in the UI; cyan elsewhere means NOTE/SLD sequencer cells.

Required fix:
- Decide one convention and apply it: either both generation buttons use the orange action accent (with MUTATE distinguished by label only), or document cyan as the deliberate "pattern-content" accent and keep it. If kept, no code change — record the decision in a comment.
- Button size, position, and behavior unchanged.

---

## Guardrails

- Presentation layer only. Do not change DSP behavior, parameter ranges, defaults, automation, or persistence.
- Do not touch audit issues 6 (MIX2/PITCH naming) or 7 (seed/zero-padding formats) — confirmed intended.
- Do not redesign colors, fonts, or panel styling beyond what each issue's required fix states.
- No new dependencies. Follow existing code patterns in `plugin303/src/gui/`.
- Do not regress prior round fixes: no edge clipping on any side (bottom-row octave arrows are currently fully visible — keep them that way); ACCURATE mode must still dim QUALITY, the "2x" control, and ANALOG CHARACTER & TWEAKS; running state must still show RUN→STOP and the playhead highlight.

## Verification

Re-render and capture all three states (FUN stopped, FUN running, ACCURATE) at the same window size. Confirm per issue:

1. "2x" control labeled/relocated; header shows no orphan box; dim state still tracks topology.
2. OUTPUT CLIPPER, ANALOG CHARACTER & TWEAKS, ENVELOPES panels show no dead space; margins consistent.
3. QUALITY selection accent is orange in FUN and dimmed-orange in ACCURATE; red appears only on the playhead.
4. BPM reads `120` (integer); press + and − once and confirm integer display persists.
5. Toggle ACCURATE ↔ FUN: regions 4–8 do not move; overlay fills the exact height of the souped-up section.
8. All A/D/S/R values under both mini ADSRs are legible and non-overlapping.
9. JIT control label/tooltip reads clearly as jitter.
10. No empty rectangle at bottom-right; margins match the rest of the window.
11. CALIBRATE idle shows dimmed (not struck-through) text; lit state unchanged.
12. Souped-up knob row spacing reads as one even row or as two clearly separated groups.
13. TOL/AGE/NOISE controls share one consistent affordance style.
14. GENERATE/MUTATE follow one documented color convention.

Also confirm no regressions: bottom-row octave arrows fully visible, right-edge controls (ADV ENV, R-COMP, MUTATE, NEW SEED) inside margins, running state still shows step highlight and STOP button.
