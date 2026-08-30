# GUI Audit Round 8 — Fix Prompt

## Context

TECHNEO 303 is a Rust/egui desktop bass-synth plugin UI (TECHNEO 303 window, 2732x1984 px capture = 1366x992 logical @2x). UI regions, top to bottom:

1. macOS title bar / brand "TECHNEO 303"
2. Header panel — BASS SYNTH/MONO chips, TOPOLOGY, QUALITY, ZOOM
3. Oscillator strip — WAVE selector, 7 knobs, ENVELOPES (MEG + VEG ADSR boxes)
4. SOUPED-UP ENGINE & ANALOG DRAWER — VCF TAPS, VCA ENVS, 9 knobs
5. OUTPUT CLIPPER — BYPASS, OS 2X/4X, 9 knobs, LINK/DIODE cluster
6. ANALOG CHARACTER & TWEAKS — TOL/AGE/NOISE, DRIFT, SEED, RECAL, CALIBRATE
7. ACID EUCLIDEAN GENERATOR — 4 control groups, GENERATE/MUTATE buttons
8. Sequencer grid (2 rows x 8 steps) + TRANSPORT & PATTERN side panel

Reference captures: `target/gui-qa/latest.png` (stopped), `target/gui-qa/running.png` (transport running).

## Issue 1 (HIGH) — Bottom two panels have no right window margin and no right border

**Observed symptom.** In both captures, the ACID EUCLIDEAN GENERATOR panel (region 7) and the sequencer/TRANSPORT & PATTERN panel (region 8) extend flush to the last pixel column of the window. Pixel-luminance profile of `latest.png`:

- Upper panels (regions 2-5): right border line at x2710, window background x2712-2731 (20px margin). Left edge: border at x20, background x0-19. Symmetric.
- Regions 7 and 8: content luminance unbroken through x2731 (the final pixel column). No border stroke, no margin.
- Worst point: the MUTATE button's orange fill reaches x2731 — it touches the window edge with 0px margin.

**Required fix.** Allocate the two bottom panels the same width as the upper panels so that:

- Both panels end with the same right border stroke at x2710 (±2px) as regions 2-5.
- 20px (±3px) of window background remains to the right of both borders, matching the left margin exactly.
- MUTATE button right edge sits at x≤2695, with the same inner right padding the upper panels use (~20px from panel border).
- Layout of everything left of x2690 in those panels is unchanged; no wrapping, resizing, or reflow of sequencer columns, transport controls, or generator controls.
- If the cause is egui width allocation (e.g. a panel built from `allocate_new_ui` with an un-clamped `max_rect`, or a ScrollArea reserving no right space), fix the allocation at the source — do not pad with spacer widgets.

**Guardrails.**

- Presentation/layout only. Do not change behavior, ranges, defaults, persistence, MIDI, audio, pattern data, or control IDs.
- Do not touch regions 1-6 or any knob rendering (round-7 needle fixes are verified correct — do not regress them).
- Do not "fix" the flagged observations below; they are out of scope.

## Out of scope (flagged observations, do not change)

- O1: pink/red playhead highlight color on the running step.
- O2: bright-vs-dim note-name coloring (non-root pitches bright, C3 dim).
- O3: empty middle band in the ENVELOPES panel (presumed ADV ENV reserve).
- O4: cramped ADSR slider value labels ("3 ms 200 ms 0% 200 ms").
- O5: sequencer grid left padding (~12px) vs upper panels (~20px).

## Verification requirements

1. Rebuild and re-capture both states (stopped and transport running) at exactly 2732px capture width. Reject captures at any other width.
2. For each new capture, run a pixel check and confirm:
   - A border-luminance vertical line exists at x2708-2712 across regions 7 and 8, matching regions 2-5.
   - Columns x2714-2731 are window background across the full height of regions 7 and 8.
   - MUTATE button right edge ≤ x2695.
3. Confirm zero visual change in regions 1-6 between old and new stopped captures (diff must be empty above y≈990).
4. Confirm the running state still shows the step-04 playhead and RUN/STOP toggle, and the pixel diff between the two new states is confined to the playhead column and transport button, same as before.
5. Run the existing test suite; it must pass unchanged.

## Confirmation list

- [ ] Issue 1: ACID EUCLIDEAN GENERATOR panel right border at x2710 ±2, 20px margin, MUTATE right edge ≤2695
- [ ] Issue 1: sequencer/TRANSPORT panel same border + margin
- [ ] Issue 1: regions 1-6 pixel-identical to previous captures
- [ ] Issue 1: running-state feedback (playhead, RUN→STOP) unchanged
- [ ] Issue 1: test suite green
