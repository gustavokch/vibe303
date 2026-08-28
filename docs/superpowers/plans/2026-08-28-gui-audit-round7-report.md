# GUI Visual Audit — Round 7 Report

- **Date:** 2026-08-28
- **Captures:** `target/gui-qa/` (05:23), window 2732×1984 px (Retina 2x, 1366×992 pt)
- **States:** `latest.png` / `fun.png` (stopped — byte-identical), `running.png` (playing)
- **Method:** direct vision read of full windows + zone/crop images, pixel-level angle measurement of knob arcs/needles, code confirmation in `plugin303/src`

## Layout map (top → bottom)

1. macOS title bar — "TECHNEO 303"
2. App header — TECHNEO 303, BASS SYNTH, MONO badges; TOPOLOGY (ACCURATE/FUN), QUALITY (LOW/HIGH/ULTRA/2X), ZOOM (100%)
3. Main synth panel — WAVE (SAW/SQR/SWP); TUNING, CUTOFF, RESONANCE, ENV MOD, DECAY, ACCENT, VOLUME; ENVELOPES (MEG ADSR, VEG ADSR); ADV ENV
4. Souped-up engine & analog drawer — VCF TAPS (2/3/4-POLE), VCA ENV (GATE/ENV); DRIVE, SUB, GLIDE, PITCH1, PITCH2, MIX2, PW, SWEEP, R-COMP
5. Output clipper — BYPASS, OS (2X/4X); DRIVE, THRESH, BASS, TONE, IN, OUT, TILT, DYN, MIX; LINK, DIODE (SI/GE)
6. Analog character & tweaks — TOL, AGE, NOISE; DRIFT; SEED, RECAL; CALIBRATE/CALIBRATED
7. Acid euclidean generator — GEN SEED + NEW SEED; columns 1–4 (PATTERN STRUCTURE / OCTAVE BEHAVIOR / PROBABILITIES / GENERATION)
8. Sequencer — 16 steps × 2 rows (ACC/SLD, NOTE/TIE/REST, pitch, octave arrows)
9. Transport & pattern (right of sequencer) — RUN/STOP, SYNC, MIDI/CH/SYNTH, SHIFT, BPM, LEN, timebase/mode combos, JITTER, CLEAR

## Defects

### 1. HIGH — Bipolar knobs at zero render needle at MIN, contradicting arc and badge

Seven knobs display a zero-centered value (`+0.0`) with the value arc drawn correctly to center (~50%, 12 o'clock) while the needle sits at the 7:30 minimum:

| Knob | Region | Badge | Arc | Needle |
|---|---|---|---|---|
| TUNING | main panel | +0.0 ct | 50% | min |
| VOLUME | main panel | +0.0 dB | 50% | min |
| PITCH1 | souped-up drawer | +0.0 st | 50% | min |
| PITCH2 | souped-up drawer | +0.0 st | 50% | min |
| IN | output clipper | +0.0 dB | 50% | min |
| OUT | output clipper | +0.0 dB | 50% | min |
| TILT | output clipper | +0.0 dB | 50% | min |

Evidence: all screenshots, both states (state-independent). Pixel-measured on `latest.png`: TUNING/VOLUME arc spans rel 1–135° of the 270° sweep, needle at rel ~3°. RESONANCE 50% measured as correct reference (arc and needle both at rel 132–138°).

Root cause (code-confirmed): `plugin303/src/gui/widgets/knob.rs:146-161`. `zero_snapped` fires for zero plain values on labels matching VOLUME/PITCH/DET/IN/OUT/TILT — the `"IN"` match also catches TUN**IN**G, as the comment admits — and then `current_angle = start_angle` places the needle at the minimum. All seven params are bipolar with zero at normalized 0.5: tuning −100..+100 (`params.rs:432-442`), volume SymmetricalSkewed center 0 (`params.rs:455-464`), clip_in −18..+18 (`params.rs:691`), clipper_tilt −6..+6 (`params.rs:696`), pitch ±st. The "snap to start_angle at zero — fix #17" branch is harmless only for unipolar zero, where it is a no-op.

### 2. LOW — DRIFT and RECAL knobs vertically misaligned in tweaks panel

DRIFT knob center sits ~25 px higher than RECAL knob center in the same panel row (`crop_souped.png`). Cause: tweaks column 2 renders spacer + knob (`tweaks_panel.rs:155-157`), while column 3 renders the SEED row above its knob (`tweaks_panel.rs:171-204`). The two knobs in one panel should share a baseline.

### 3. LOW — Transport combo buttons show ▲ instead of ▼

The "16th ▲" and "Direct ▲" buttons (transport, bottom-right; `crop_bottom_margin.png`, both states) render up-pointing arrows. Both are egui `ComboBox` widgets (`sequencer_panel.rs:381-435`). Every other dropdown in the app (ROOT, SCALE, TOL, ZOOM) shows a down chevron; ▲ reads as "menu open / collapse" and breaks the app-wide convention. (Popup opening direction at the window edge may stay automatic — the icon must not imply it.)

### 4. LOW — QUALITY button "2X" breaks tier naming

The header quality row reads LOW / HIGH / ULTRA / 2X (`crop_header.png`). The fourth tier is `Tier::Ultra2x`, display name "Ultra 2x (WDF)" (`plugin303/src/dsp/filter.rs:25`). Dropping the ULTRA prefix makes "2X" read as an unrelated multiplier — and collides with the clipper OS row, which has its own "2X" with a different meaning (`output_clipper.rs:70`). Label list: `header_bar.rs:131`.

### 5. LOW — ENVELOPES region dead space, ADV ENV isolated

In the main panel (`crop_classic.png`, both states), a wide empty gap spans from the VEG ADSR fader group to the ADV ENV checkbox at the far right edge. The checkbox is the only control in ~40% of the panel width and reads as detached from the ENVELOPES group it belongs to. Rendering code: `top_controls.rs:105-180` (comment at :105 already claims "MEG/VEG/ADV ENV distributed evenly" — they are not).

### 6. LOW — Bottom dead band below sequencer; transport bottom row floats

`crop_bottom_margin.png` (both states): ~40 px of empty space between the sequencer arrow row and the window's bottom edge. In the transport column, the bottom row (16th / Direct / JITTER / CLEAR) is pushed down by a computed filler (`sequencer_panel.rs:370-372`, `transport_bottom_filler`) but still lands above the band, so the panel's last row aligns with neither the sequencer's last row nor the window bottom.

## Verified correct this round

- State diff stopped → running: RUN→STOP button swap, red playhead on step 12 (red number + border, legible on a REST step). Live feedback present; no geometry shift, no other state bleed. `crop_state_diff.png` stacks identical.
- Value/position agreement on unipolar knobs (measured): RESONANCE 50% (needle + arc exactly 0.5), DRIFT 10% (both ~0.09), RECAL 100% (arc measured full 0–270°, needle at end), DRIVE/SUB/MIX2/SWEEP/R-COMP/DYN/MIX at min, PW/ACCENT/ENV MOD at 50%.
- Badge formats consistent per type (`+0.0 ct/st/dB`, `50%`, `2.1 kHz`, `1.1 s`, `200 ms`); CUTOFF badge math verified (normalized 0.5 → 2.1 kHz); DECAY 0.5 → 1.1 s.
- Selection-accent convention (orange outline) uniform: FUN, LOW, SAW, 4-POLE, ENV, 2X, SI, GENERATE/MUTATE. NEW SEED button unclipped — round-6 fix holds.
- All four window edges: no clipped content, margins intact.
- Seed fields zero-padded 4-digit consistently (GEN SEED 0042, SEED 1337).

## Most urgent

Issue 1 — seven knobs visually report minimum while the engine sits at center value; single-class fix at `knob.rs:157`. Issues 2–6 are presentation-only cosmetics.
