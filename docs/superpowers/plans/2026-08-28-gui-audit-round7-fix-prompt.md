# Fix Prompt — GUI Visual Audit Round 7 Remediation

**Audience:** planning/coding agent. Produce a plan, then implement.
**Source audit:** `docs/superpowers/plans/2026-08-28-gui-audit-round7-report.md` (full evidence, measurements, screenshots map).

## Context

TECHNEO 303 — TB-303-style bass synth plugin (nih-plug + egui), GUI code in `plugin303/src/gui/`. Window 1366×992 pt (captures at 2732×1984 px, Retina 2x). Reference captures: `target/gui-qa/latest.png` (stopped) and `target/gui-qa/running.png` (playing).

UI regions, top to bottom:

1. App header — title, badges, TOPOLOGY, QUALITY, ZOOM (`panels/header_bar.rs`)
2. Main synth panel — WAVE, 7 main knobs, ENVELOPES (MEG/VEG ADSR), ADV ENV (`panels/top_controls.rs`)
3. Souped-up engine & analog drawer — VCF TAPS, VCA ENV, 9 knobs
4. Output clipper — BYPASS, OS, 9 knobs, LINK, DIODE (`panels/output_clipper.rs`)
5. Analog character & tweaks — TOL/AGE/NOISE, DRIFT, SEED/RECAL, CALIBRATE (`panels/tweaks_panel.rs`)
6. Acid euclidean generator — GEN SEED, 4 columns, GENERATE/MUTATE
7. Sequencer — 16 steps × 2 rows (`panels/sequencer_panel.rs`)
8. Transport & pattern — right of sequencer (`panels/sequencer_panel.rs`)

Knob widget: `plugin303/src/gui/widgets/knob.rs`. Params: `plugin303/src/params.rs`.

## Issues to fix (numbering is stable — map confirmations back to these numbers)

### Issue 1 (HIGH) — Bipolar knobs at zero render needle at MIN

**Symptom:** TUNING (+0.0 ct), VOLUME (+0.0 dB), PITCH1/PITCH2 (+0.0 st), IN/OUT/TILT (+0.0 dB) draw the value arc to center (50%) but the needle at the 7:30 minimum. Pixel-measured: arc rel ~135° of 270°, needle rel ~3°.

**Cause:** `knob.rs:146-161` — `zero_snapped` triggers on zero plain value for zero-center labels, then `current_angle = start_angle` (minimum). Zero on these params is normalized 0.5 (12 o'clock).

**Required fix:** at zero on a zero-center branch, the needle must point at the param's zero position — `start_angle + normalized * (end_angle - start_angle)` — not `start_angle`. The unipolar-zero snap (normalized < 0.001) may stay; it is a no-op there. Acceptance: on all 7 named knobs, needle, arc, and badge agree (needle at 12 o'clock, arc endpoint at the needle). No other knob's rendering changes — RESONANCE/DRIFT/RECAL/DRIVE/PW etc. must render byte-identically at their current values.

### Issue 2 (LOW) — DRIFT and RECAL knobs vertically misaligned

**Symptom:** DRIFT knob center sits ~25 px higher than RECAL in the tweaks panel row.

**Cause:** `tweaks_panel.rs` — column 2 is spacer + knob (`:155-157`); column 3 has the SEED row above the knob (`:171-204`).

**Required fix:** both knobs share one vertical baseline within the panel. Acceptance: DRIFT and RECAL knob centers at the same y (±2 px); SEED row stays above RECAL; no column width changes.

### Issue 3 (LOW) — Transport combo buttons show ▲ instead of ▼

**Symptom:** "16th ▲" and "Direct ▲" (transport, bottom-right) render up-pointing arrows; every other dropdown in the app (ROOT, SCALE, TOL, ZOOM) shows ▼. Widgets: egui `ComboBox` at `sequencer_panel.rs:381-435`.

**Required fix:** both combos render the down-chevron closed state, matching the app-wide convention. Acceptance: "16th" and "Direct" buttons show ▼; popup open/close still works; opening direction at the window edge may remain automatic.

### Issue 4 (LOW) — QUALITY button "2X" breaks tier naming

**Symptom:** header quality row reads LOW / HIGH / ULTRA / 2X. The fourth tier is `Tier::Ultra2x` ("Ultra 2x (WDF)", `dsp/filter.rs:25`). "2X" also collides with the clipper OS "2X" (different meaning). Label list: `header_bar.rs:131`.

**Required fix:** rename the header button to "ULTRA 2X". Acceptance: quality row reads LOW / HIGH / ULTRA / ULTRA 2X; selection highlight and tier behavior unchanged; the clipper OS "2X" label stays as-is. If the wider label breaks the header row's fit, adjust spacing within the header only — no other region may move.

### Issue 5 (LOW) — ENVELOPES dead space, ADV ENV isolated

**Symptom:** in the main panel, a wide empty gap separates the VEG ADSR fader group from the ADV ENV checkbox at the far right; the checkbox owns ~40% of the panel width and reads as detached from the ENVELOPES group. Code: `top_controls.rs:105-180`.

**Required fix:** distribute the envelope row so ADV ENV sits adjacent to the VEG ADSR block at the group's normal control spacing (or the row is otherwise balanced — no lone control on the right margin). Acceptance: no empty gap wider than the row's standard inter-control spacing anywhere between MEG ADSR and ADV ENV; MEG/VEG fader geometry unchanged.

### Issue 6 (LOW) — Bottom dead band; transport bottom row floats

**Symptom:** ~40 px of empty space between the sequencer arrow row and the window bottom edge. The transport bottom row (16th / Direct / JITTER / CLEAR) is pushed down by `transport_bottom_filler` (`sequencer_panel.rs:370-372`) but still lands above the band, aligning with neither the sequencer's last row nor the window bottom.

**Required fix:** reclaim the band — the sequencer's final row and the transport's final row share one bottom baseline with the standard panel margin (matching the left/right margins), leaving no residual empty strip. Acceptance: bottom margin equals the side margins (±2 px); sequencer and transport bottom rows are baseline-aligned; no vertical scrollbar appears; window fill is complete in both states.

## Guardrails

- Presentation layer only. Do not change parameter ranges, defaults, DSP, behavior, persistence, or state recall.
- Do not refactor beyond what each issue requires. No drive-by cleanups, no renames outside the named labels.
- Do not regress round-6 fixes (NEW SEED button fit, fader/track contrast) or the RUN/STOP + playhead behavior.
- Keep existing knob badge strings and formats unchanged except Issue 4's one label.
- `cargo test` must stay green; add or update widget tests only where they pin a fixed behavior (e.g., zero-center needle angle).

## Verification

1. Rebuild the standalone app; re-capture stopped and running states at the same window size into `target/gui-qa/`.
2. Per issue, confirm in the fresh captures:
   - **Issue 1:** TUNING, VOLUME, PITCH1, PITCH2, IN, OUT, TILT — needle at 12 o'clock, arc endpoint at the needle, badge still `+0.0 …`. Spot-check RESONANCE 50% and RECAL 100% still render as before.
   - **Issue 2:** DRIFT/RECAL knob centers level.
   - **Issue 3:** both transport combos show ▼.
   - **Issue 4:** quality row shows ULTRA 2X; header layout intact, no wrap or overlap.
   - **Issue 5:** ADV ENV adjacent to the envelope group; no dead gap.
   - **Issue 6:** bottom margin matches side margins; sequencer and transport bottom rows aligned.
3. Compare stopped vs running captures: only RUN/STOP and the playhead may differ.
4. Report a per-issue confirmation list referencing issue numbers 1–6, with before/after crop filenames.
