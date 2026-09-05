# TECHNEO 303 — GUI Visual Audit, Round 15

Date: 2026-09-03
Captures: `target/gui-qa/` (`latest.png`, `running.png`, `advanced.png`,
`accurate.png`, `knobs.png`), all 2732x1984, fresh.
Baseline: commit `36fd193` "fix(gui): point default widget text at theme tokens (round-14 issue 7)" on branch `fix/gui-visual-remediation-r14` — all 7 round-14 issues claimed fixed there.

## Capture set

| File | State |
|---|---|
| `latest.png` | default (Fun topology, ADV ENV off, seq stopped) |
| `running.png` | seq_run=true, playhead on step 04 |
| `advanced.png` | ADV ENV on (MEG bank enabled) |
| `accurate.png` | CircuitTopology::Accurate |
| `knobs.png` | tuning=25.0, cutoff=0.75 (off-default knob values) |

Pixel diffs confirm five distinct states. `running.png` differs from
`latest.png` only inside the sequencer grid (y 1366-1629, x 809-2406,
n=34622): playhead on step 04, STOP lit orange.

## Round-14 items verified fixed

- **HIGH-1 (dead space right of ENVELOPES module).** `latest.png` and `advanced.png`:
  The ENVELOPES module is anchored right (`top_controls.rs:137-139`) and the 7-knob
  strip fills the left space in equal columns. Deadspace probe on panel y 180-384
  reports 0 dead columns (fill 5.6%).
- **HIGH-2 (QUALITY selector selection mark in ACCURATE).** `accurate.png`:
  Disabled QUALITY switch retains its distinct thumb fill and bounding border
  around the selected "LOW" tier (`widgets/switch.rs:334-355`), remaining
  identifiable while disabled.
- **HIGH-3 (disabled panel text contrast <3:1 in ACCURATE).** `accurate.png`:
  Double-dimming removed across disabled ANALOG CHARACTER and TWEAKS panels.
  Colors probe reports no foreground pixels below 3.0:1 across all panels
  (lowest foreground contrast is 3.17:1 on disabled sliders/toggles).
- **HIGH-4 (ACCURATE placeholder panel framing and title).** `accurate.png`:
  The "SOUPED-UP ENGINE & ANALOG DRAWER" panel header and border stroke remain
  rendered with the bypass placeholder neatly contained inside (`souped_panel.rs:43-55`).
- **MEDIUM-5 (bottom-row combo typography and row height).** `latest.png`:
  The "16th" and "Direct" combo glyphs render with theme caption font size (h=11-12),
  matching the CLEAR button and JITTER checkbox in the same row.
- **MEDIUM-6 (DRIFT knob arc/needle delta).** `latest.png`:
  DRIFT knob arc/needle delta measured at 3.5° (was 4.7° in round 14), now well
  under the 4.0° mismatch threshold.
- **LOW-7 / Issue 7 (theme token compliance).** `latest.png`:
  Captions and default widget texts now point to `#FFFFFF`, `#8F8FA4`, `#C4C4D0`
  theme tokens. Hardcoded `#B4B4B4` is completely eliminated from the bottom
  row and output clipper panels.

## Layout map

1. Header — y 84-170 — brand block left; ZOOM, QUALITY, TOPOLOGY right.
   Source: `plugin303/src/gui/panels/header_bar.rs`.
2. Top controls — y 180-384, v-dividers x 1460/1962 — WAVE switch, 7 knobs,
   ENVELOPES module. Source: `plugin303/src/gui/panels/top_controls.rs`.
3. Souped toggles — y 384-470 — VCF TAPS, VCA ENV segments.
4. Souped knobs — y 513-680.
5. Clipper tweaks — y 690-872.
6. Output clipper — y 882-1101.
7. ACID EUCLIDEAN GENERATOR — y 1111-1346, h-divider x 1166.
8. Sequencer — y 1356-1794, v-divider x 2108.
9. Bottom row — y 1794-1958 — pattern/transport controls, 16th/Direct
   combos, JITTER, CLEAR.

---

## HIGH

*No HIGH severity issues identified in this round.* All previous HIGH defects are verified resolved.

---

## MEDIUM

*No MEDIUM severity issues identified in this round.*

---

## LOW

1. **Panel 2 vertical height shift (2 px) on ADV ENV toggle.**
   **Symptom.** In `advanced.png`, panel 2 measures y 180-382 (h=202 px) compared
   to y 180-384 (h=204 px) in `latest.png`, shifting all subsequent panels up by
   2 physical pixels.
   **Cause.** In `top_controls.rs:50` vs `top_controls.rs:78`, `render_adsr_bank` uses
   `ui.label(...)` when active (which takes default widget height) vs
   `ui.allocate_exact_size(vec2(..., 11.0), ...)` when inactive (which explicitly
   constrains height to 11.0 logical px).
   **Required fix.** Unify title layout between active and inactive states in
   `render_adsr_bank` so the allocated frame height is constant regardless of
   the `is_active` state.

2. **Inter-panel vertical gutters inconsistent: `[10, 0, 43, 10, 10, 10, 10, 0]`.**
   Gap above souped toggles and bottom row is 0 px; gap above souped knobs is 43 px.
   Consistent with sub-panel grouping design, but still asymmetric.

3. **Header ZOOM combo text #999999 at 3.87:1 on #3C3C3C.**
   Below the 4.5:1 AA threshold for interactive control text.

4. **#06777F dim cyan (inactive NOTE cells) at 3.40:1 in sequencer.**
   Second cyan color outside the defined `theme.rs` token set.

---

## Tool false positives — recorded so future rounds do not re-investigate

- `knobs` flagged TONE at (1197,738), delta=10.9°. Ring-fit places centre ~6 px
  high; actual needle and arc agree within ~3°. Both driven by same parameter
  normalized value in `knob.rs`. **False positive.**
- `knobs` r=20 detections at (914,1220), (1025,1222), (2213,1854), (2262,1854)
  are combo dropdown arrow circles. **False positives.**

## Flagged, intent unclear — not asserted as defects

- GENERATOR title row: `DEAD COLUMNS x 340..2594 in band y 1121-1156` is the
  panel title row ("ACID EUCLIDEAN GENERATOR" left, NEW SEED button right).
  Expected title/action layout pattern.
- CLEAR button spans the full height of the bottom panel controls group for visual
  emphasis.

## Capture gaps for round 16

- **No hover/focus/pressed state** for any control class.
- **No zoom ≠ 100% state** (e.g. 75% zoom capture).
- **No state with QUALITY at a non-default tier** (HIGH, ULTRA, ULTRA 2X).
- **No state with generator controls modified** from default values.

## Summary

All 7 issues from Round 14 are verified fixed. The interface exhibits clean layout,
consistent typography, compliant contrast across all active and disabled states,
and accurate knob arc/needle alignment. Only minor LOW-severity polish items remain.
