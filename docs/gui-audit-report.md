# GUI Visual Audit Report — TECHNEO 303 (vibe303)

Screenshots audited: `latest.png` (FUN stopped), `running.png` (FUN running), `accurate.png` (ACCURATE stopped + overlay), plus zone/crop images (`zone_bottom`, `zone_mid`, `zone_top`, `crop_classic`, `crop_sequencer`, `crop_clipper_tweaks`, `crop_souped`). The screenshot capture set lives outside the repository (local QA captures) and is not tracked here.

Reference taxonomy: `fix-prompt.md` (16 issues) plus direct vision inspection.

## Defects by Region / Category

### 1. Bottom Dead Band + Sequencer Clipping (Critical)
- What: Large black void below content in all three states. FUN ~77 px, ACCURATE ~147 px. Sequencer only shows step "01"; steps 02–16 completely clipped/missing. No scroll indicator.
- Where: Region 7 (Transport & Pattern + Sequencer) and Region 8 (Bottom band).
- Evidence: `latest.png`, `running.png`, `accurate.png`, `latest_zone_bottom.png`, `latest_crop_sequencer.png`.
- Defect: Confirmed.
- Status: Resolved 2026-08-24 — root cause removed (`ui.available_height()` background allocation in `render_step_strip`); 2×8 grid, all 16 steps visible, pills 26 px drawn / 30 px hit, panel border reaches window bottom (verified in fresh captures on `fix/gui-audit-phase4`).

### 2. Dead Column Right of Tweaks
- What: ANALOG CHARACTER & TWEAKS panel ends ~86% width. Empty strip (~350 px) to right window margin.
- Where: Region 5 (Clipper row / tweaks panel).
- Evidence: `accurate.png`, `latest_crop_clipper_tweaks.png`.
- Defect: Confirmed.

### 3. Accurate Overlay Band Empty / Over-sized
- What: Overlay box ("ACCURATE MODE ACTIVE") floats centered in full-width empty band. Band height exceeds box by wide top/bottom margins.
- Where: Region 4 (Mid band) in ACCURATE state.
- Evidence: `accurate.png`, `accurate_zone_mid.png`.
- Defect: Confirmed.

### 4. Ragged Panel Bottoms (Clipper vs Analog)
- What: OUTPUT CLIPPER bottom sits above ANALOG CHARACTER & TWEAKS bottom; sibling panels in same row do not share bottom alignment.
- Where: Region 5.
- Evidence: `latest_crop_clipper_tweaks.png`, `running_crop_clipper_tweaks.png`.
- Defect: Confirmed.

### 5. Knob Arc Fill vs Pointer Position (Contradiction)
- What: TUNING, CUTOFF, RESONANCE, ENV MOD, DECAY, ACCENT, VOLUME all display 50% but arc fills vary (some past 12 o'clock ~65% fill, others stop short ~45%). Same value = different visual states.
- Where: Region 3 (Classic row knobs).
- Evidence: `latest_crop_classic.png`, `running_crop_classic.png`.
- Defect: Confirmed.

### 6. QUALITY Ghost Selection Disabled
- What: In ACCURATE state (`accurate.png`), QUALITY group dimmed. LOW retains visible outline; HIGH/ULTRA flat with no outline. Non-uniform disabled appearance.
- Where: Region 2 (Header / QUALITY).
- Evidence: `accurate.png`, `latest_crop_header.png`.
- Defect: Confirmed.

### 7. DYN/MIX Dimmed with LINK Checked
- What: Controls at 0% with LINK checked show uniform dim. Brightness does not reflect independent link state clearly.
- Where: Region 5 (ANALOG CHARACTER & TWEAKS panel).
- Evidence: `latest_crop_clipper_tweaks.png`.
- Defect: Confirmed / flagged (verify if intentional).
- Status: Closed 2026-08-24 — intentional 0%-value dim styling, consistent across all knobs (soup DRIVE/SUB/SWEEP/R-COMP at 0% dim identically). No change.

### 8. PW 50% Arc Contradiction (Souped-Up)
- What: PW knob at 50% in soup drawer (`SOUPED-UP ENGINE & ANALOG DRAWER`) shows arc not matching pointer position.
- Where: Region 4 / soup drawer (middle row, below classic).
- Evidence: `latest_zone_mid.png`, `running_zone_mid.png`.
- Defect: Confirmed (same mechanism as #5).

### 9. STOP Button Color Exception
- What: `fix-prompt.md` reports pink/red. In current screenshots (`latest.png`, `running.png`), transport STOP button appears orange/black (not pink/red). Issue may already be corrected or not reproduced in current build. Flagged.
- Where: Region 7 (Transport).
- Evidence: `latest_zone_bottom.png`.
- Status: Flagged — verify current build state.

### 10. OUTPUT CLIPPER Title Color Exception
- What: `fix-prompt.md` reports pink/red title. In current images, "OUTPUT CLIPPER" appears white/light gray, not red.
- Where: Region 5.
- Evidence: `latest_crop_clipper_tweaks.png`.
- Status: Flagged — appears resolved; confirm.

### 11. Playhead Yellow = TIE Yellow Ambiguity
- What: In running state (`running.png`), step 01 shows NOTE highlight (cyan). Full 16-step playhead tracking impossible because only step 01 visible (see #1). Cannot confirm TIE highlight distinction without all steps visible.
- Where: Region 7 (Sequencer steps).
- Evidence: `running.png`, `running_zone_bottom.png`.
- Status: Resolved 2026-08-24 — initially blocked by #1; unblocked once the sequencer grid was restored. Playhead = ruby number badge + cyan LED bar + elevated cell background; TIE = gold badge. Distinct at a glance and at 100% zoom; verified across the full grid in `running.png` (step 04 active, TIE on 02/06/10/14).

### 12. Mixed Decimal Precision / Formatting
- What: `+0.0 ct` (1 decimal, spaced), `2.1 kHz` (1 decimal, spaced), `200 ms` (integer, spaced), `50 %` (integer, spaced), `3 ms` / `200 ms` / `0%` / `200 ms`. No consistent rule applied; some values use 2 dp (`+0.10 s`), others 1 dp (`+0.0 ct`), others integer.
- Where: Region 3 (Classic knobs), Region 5 (Clipper), Region 6 (Generator), Region 7 (Transport/BPM).
- Evidence: `latest_crop_classic.png`, `latest_crop_clipper_tweaks.png`, `accurate_crop_sequencer.png`.
- Defect: Confirmed.
- Status: Resolved 2026-08-24 — DECAY badge now 1 dp seconds (`1.1 s`); rule unified: 1 dp for s/kHz/dB/ct/st/BPM, integer for ms/Hz/%/counts.

### 13. MEG ADSR / VEG ADSR Naming (Cryptic)
- What: Pair labels provide no legend in UI. User cannot distinguish envelope 1 vs envelope 2 without external reference.
- Where: Region 3 (ENVELOPES panel).
- Evidence: `latest_crop_classic.png`, `running_crop_classic.png`.
- Defect: Confirmed / flagged (naming/intention unclear).

### 14. WAVE Selector Orphan Track
- What: Empty vertical slider-like bar left of SAW/SQR/SWP options. Appears draggable but has no visible effect.
- Where: Region 3 (WAVE selector, left side).
- Evidence: `latest_crop_classic.png`.
- Defect: Confirmed.

### 15. Micro Hit Targets
- What: BPM +/- steppers, JIT checkbox (`latest.png` shows small checkbox), envelope caps (A/D/S/R), arrow rows (`▲`/`▼`), step buttons very small.
- Where: Regions 3, 6, 7.
- Evidence: `latest_zone_bottom.png`, `latest_crop_sequencer.png`.
- Defect: Confirmed.
- Status: Resolved 2026-08-24 — BPM +/- (24 px) and JIT (28×24) fixed earlier; step-strip sub-controls now 30 px hit targets with 26 px drawn pills (ACC/SLD/gate/pitch/octave).

### 16. Running Feedback (Preserve — Positive)
- What: Step 04 playhead highlight + STOP label visible in running state. Live state clearly communicated.
- Where: Region 7 (Transport + Sequencer).
- Evidence: `running.png`, `running_zone_bottom.png`.
- Defect: None — positive observation; must not break during fix.

## Most Urgent Summary (Top Fixes)
1. Unclip sequencer / restore 16 steps (#1) — blocks verification of other sequencer-related issues.
2. Remove bottom dead band / align content to window bottom (#1).
3. Fix knob arc/pointer contradiction (#5, #8) — clear value/position mismatch.
4. Reduce accurate overlay band height / center vertically (#3).
5. Remove dead column / extend tweaks to right margin (#2).
6. Align sibling panel bottoms (#4).

## Confirmation List
Issues verified / observed: 1, 2, 3, 4, 5, 6, 7, 8, 9 (flagged), 10 (flagged), 11 (blocked by 1), 12, 13, 14, 15, 16 (positive, preserve).
