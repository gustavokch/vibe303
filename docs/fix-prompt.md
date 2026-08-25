# Fix Prompt — GUI Visual Audit Remediation (Phase 4)

Context: TECHNEO 303 standalone binary (`plugin303/src/gui/`). Window 1366x960 logical (2732x1984 retina captures). Presentation-layer only. No behavior, range, persistence, or DSP changes.

Regions (numbered for agent):
1. Header bar (TECHNEO 303 / BASS SYNTH / MONO / TOPOLOGY / QUALITY / ADV ENV)
2. WAVE panel (SAW / SQR / SWP toggle + 8 knobs)
3. Envelope panel (MEG ADSR / VEC ADSR with A D S R tick axes)
4. Output Clipper (BYPASS / 2X / 4X + 10 knob row + LINK checkbox + MODE / GE)
5. Analog Character & Tweaks (TOL / AGE / NOISE / COMP SEED / CALIBRATE)
6. Acid Euclidean Generator header + parameters + 2x8 grid + transport
7. Transport / Pattern column (RUN/STOP / SYNCHOST / MIDI / BPM / LEN / SHIFT / JIT / CLEAR)
8. Bottom step grid (rows 01-08 / 09-16; ACC / SLD / NOTE / TIE / REST / pitch / octave arrows)

---

## Issue #1 — Residual bottom gap (~10-15 px) below sequencer grid

**Symptom:** `latest_zone_bottom.png` and `running_zone_bottom.png` show small dark gap between bottom arrow row (step 16) and window bottom edge. Frame content does not reach full window height; `frame_avail_h - FRAME_VERT_CHROME_PX` leaves slack.

**Required fix:** In `plugin303/src/gui/panels/sequencer_panel.rs`, adjust `FRAME_VERT_CHROME_PX` or `set_min_height` so bottom panel border reaches within 4 px of window bottom. Acceptance: no visible dark band > 4 px below last step row in `latest.png` / `running.png`.

---

## Issue #12 — Residual decimal / spacing consistency

**Symptom:** `ENV MOD`: "50 %" has space before `%`. `GEN SEED`: colon spacing may vary. All other units (`s`, `ms`, `kHz`, `%`, `dB`, `ct`, `st`) consistent.

**Required fix:** In `plugin303/src/gui/widgets/knob.rs`, ensure `%` format has no leading space (`format!("{:.0}%", ...)`) for all percentage knobs. Ensure `GEN SEED:` label uses single space after colon (`format!("GEN SEED: {:04}", ...)`). Verify no double spaces anywhere in value badges. Acceptance: all badges match `50%`, `0%`, `+0.0 ct`, `1.1 s`, etc. without extra whitespace.

---

## Issue #17 — dB knobs ~10° CCW offset at 0.0 dB

**Symptom:** `running.png`: DRIVE, SUB, MIX2, SWEEP, R-COMP at `0 %` / `+0.0 dB` show pointer needle slightly rotated counter-clockwise (~10°) from true zero. Affects `DRIVE`, `SUB`, `GLIDE`, `PITCH1`, `PITCH2`, `MIX2`, `PW`, `SWEEP`, `R-COMP` when value is zero.

**Required fix:** In `plugin303/src/gui/widgets/knob.rs`, when `normalized == 0.0` (or within `abs(f) < 0.05` for zero-center branches), set pointer angle exactly to `start_angle` (135°) so needle points straight down-left at zero. Acceptance: `running.png` / `latest.png` — all zero-value knobs show needle aligned with start tick; no visible left rotation.

---

## Issue #18 — Envelope contrast unreadable

**Symptom:** `accurate.png` / `running.png`: envelope curves (`3 ms`, `200 ms`, `6%`, `200 ms`) and tick-axis labels very low contrast against `PANEL_BG`. Hard to read.

**Required fix:** In `plugin303/src/gui/panels/generator_panel.rs` or envelope drawing code, increase text color brightness for envelope parameters from `TEXT_SECONDARY` / `TEXT_MUTED` to `TEXT_PRIMARY`. Increase tick-axis line stroke width or change tick color from `BORDER_SUBTLE` to `BORDER_DEFAULT`. Acceptance: envelope labels clearly legible in `accurate.png`; no clipping of curves or labels.

---

## Issue #19 — CALIBRATE disabled styling weak

**Symptom:** `running.png` / `accurate.png`: CALIBRATE button in "ANALOG CHARACTER & TWEAKS" appears greyed out (`WELL_BG` + `TEXT_MUTED`) with little visual distinction from disabled state. User expects clear disabled appearance.

**Required fix:** In `plugin303/src/gui/panels/generator_panel.rs` (or analog tweaks section), apply stronger disabled styling: lower opacity, darker background, or strikethrough text for disabled CALIBRATE. Keep `RECAL` and `DRIFT` interactive with normal styling. Acceptance: `running.png` shows CALIBRATE clearly disabled (not just muted);
no regression on interactive buttons.

---

## Guardrails

- Presentation layer only. Do not change `SequencerStep`, `StepGate`, parameter ranges, DSP behavior, persistence, or pattern data model.
- Window dimensions fixed at 1366x960 logical (2732x1984 retina).
- Accent semantics preserved: `ACCENT_RUBY` = active/playhead/badge; `ACCENT_CYAN` = LED/bar/note/sld; `ACCENT_GOLD` = TIE; `ACCENT_ORANGE` = ACC/transport/run.
- Do not regress fixed findings: #2–#10, #13, #14, #15, #16.
- No new warnings from `step_button.rs`, `sequencer_panel.rs`, `knob.rs`, `generator_panel.rs`.

---

## Verification requirements

Re-render captures via `scripts/capture_standalone_gui.sh` producing `latest.png`, `running.png`, `accurate.png`. Confirm per issue:

- **#1:** `latest_zone_bottom.png` — gap ≤ 4 px.
- **#12:** `running.png` — badges show `50%`, `0%`, `+0.0 dB`, `1.1 s`, `GEN SEED: 0042` with no extra spaces.
- **#17:** `running.png` — zero knobs (DRIVE, SUB, SWEEP, MIX2, R-COMP) needle at start tick, no CCW offset.
- **#18:** `accurate.png` — envelope curves and text (`3 ms`, `6%`) clearly visible.
- **#19:** `running.png` — CALIBRATE visually distinct as disabled; `RECAL`/`DRIFT` unchanged.
