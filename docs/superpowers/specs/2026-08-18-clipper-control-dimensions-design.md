# Clipper Control Dimensions Design

**Date:** 2026-08-18
**Scope:** `plugin303` WarmClipper output stage (v4, Ge/JFET)
**Status:** Approved (design sections 1–4 confirmed in chat)

## Goal

Add four control dimensions to the output clipper:

1. Dry/Wet Mix (parallel clipping)
2. Input/Output Gain with link option (manual gain staging)
3. Pre-emphasis tilt filter (pre/post shelf pair around saturation)
4. Dynamic transient recovery (fast peak detector ducking drive)

## Architecture

Approach A: integrate all new DSP as sub-blocks inside the existing
`WarmClipper` / `StagedEngine` pipeline. No wrapper structs, no new files for
DSP. Tilt and dynamics run at the **oversampled rate** inside the OS block;
input/output gain and mix run at base rate.

## Signal Flow

```
base rate:  dry_delay | in_gain | [OS up → tilt_pre → dynamics → Ge/JFET core → tilt_post → OS down] | out_gain | mix
```

1. **Dry path**: existing `Delay48` (reused — matches the 48-sample OS latency).
2. **Input gain**: `10^(in_db/20)` scalar, base rate.
3. **Pre-tilt**: first-order high shelf, +tilt_db at 2 kHz, oversampled rate.
4. **Dynamics**: fast peak detector (attack ~1 ms, release ~50 ms) on tilted
   input at OS rate; modulates effective drive:
   `drive_eff = drive_ohms · (1 − dynamics · peak_norm)`, clamped to the
   existing drive floor. `dynamics = 0` → detector skipped.
5. **Core**: existing stages A–D (Ge feedback clipper, JFET shunt), unchanged.
6. **Post-tilt**: inverse shelf, −tilt_db, oversampled rate.
   `tilt_db = 0` → both shelves passthrough.
7. **Output gain**: `10^(out_db/20)`, base rate.
8. **Mix**: equal-power law — `angle = mix·π/2`, `dry_g = cos(angle)`,
   `wet_g = sin(angle)`; output = `dry_g·dry + wet_g·wet`.

## Parameters

| ID | Name | Type | Range | Default |
|---|---|---|---|---|
| `clipper_mix` | Clipper Mix | FloatParam | Linear 0.0–1.0 (%) | 0.0 (dry) |
| `clipper_in_gain` | Clipper In | FloatParam | Skewed −18.0..+18.0 dB | 0.0 dB |
| `clipper_out_gain` | Clipper Out | FloatParam | Skewed −18.0..+18.0 dB | 0.0 dB |
| `clipper_gain_link` | Gain Link | BoolParam | — | true |
| `clipper_tilt` | Clipper Tilt | FloatParam | Skewed −6.0..+6.0 dB | 0.0 dB |
| `clipper_dynamics` | Clipper Dynamics | FloatParam | Linear 0.0–1.0 (%) | 0.0 |

`ClipperParams` (DSP struct) gains: `mix`, `in_db`, `out_db`, `tilt_db`,
`dynamics`. Link is GUI/param-layer only — not passed to DSP.

### Gain link behavior (GUI-side)

- Link on, user moves IN: setter writes `in = x`, `out = −x`.
- Link on, user moves OUT: setter writes `out = x`, `in = −x`.
- Link off: knobs independent.
- DAW automation with link on: no enforcement; DSP uses raw values.

## Coefficient Strategy

- All coefficients precalculated in `set_params()` / `reset()` — zero
  per-sample `exp()`/`powf()` (v4 perf convention).
- Tilt cutoff 2 kHz absolute: `tan(π·2000/sr_os)` where `sr_os = sr·os_factor`.
  Recomputed on OS switch (already triggers full coefficient refresh).
- Dynamics attack/release: `exp(−1/(t·sr_os))`, recomputed on sr/OS change.
- Per-OS engines own their own tilt/dynamics state — no state bleed during
  OS crossfade.

## State Additions

Per `StagedEngine`: `tilt_pre` (x1, y1), `tilt_post` (x1, y1), `peak_env`,
precalculated `tilt_coeffs`, `dyn_attack`, `dyn_release`.
Base `WarmClipper`: `in_lin`, `out_lin`, `dry_g`, `wet_g` (block-rate smoothed).

## GUI

`output_clipper.rs` grows to two rows:

```
Row 1: OUTPUT CLIPPER  [BYPASS]  OS[2X|4X]  DRIVE THRESH BASS TONE [ASYM]  GE/JFET
Row 2: IN [LINK] OUT  TILT  DYNAMICS  MIX
```

- Existing `Knob303` (size 13.0) and `SlideSwitch`/`checkbox` widgets reused.
- Row 2 greyed when bypass on.
- All new params smoothed at block rate — no zipper noise on automation.

## Error Handling

- NaN/inf guard on `peak_env`, reset at block boundary.
- All params clamped by nih_plug `FloatRange`; `drive_eff` clamped to valid
  ohm range in DSP.

## Testing

### Unit (`plugin303/tests/warm_clipper.rs`)

1. Mix law: mix=0 → delayed dry; mix=1 → wet only; mix=0.5 → equal energy
   (−3 dB each, ±10%).
2. Mix=0 null test: matches bypass output within 1e-5.
3. Gain round trip: in=+6 dB, out=−6 dB, mix=0 → unity ±0.1 dB.
4. Tilt: +6 dB → 4 kHz sine gains ≈+6 dB pre-core (low drive, mix=1);
   tilt=0 → bit-transparent passthrough.
5. Tilt inversion: below clip threshold, ±6 dB tilt → flat output ±0.5 dB.
6. Dynamics: impulse train — dynamics=1 preserves higher post-transient peak
   vs dynamics=0; dynamics=0 → identical to current v4 behavior.
7. Regression: existing suites pass with mix=1 set explicitly (default is
   mix=0, dry).
8. OS consistency: tilt/dynamics engaged, 2x vs 4x spectral match within
   existing OS tolerance.

### Integration

- Extend `eval_clipper.rs` / `eval_clipper_aliasing.py` with mix/tilt/dynamics
  sweeps; aliasing metrics must not regress at defaults.
- Latency stays 48 samples — topology tick-delay test unchanged.

### GUI (manual)

- Link syncs knobs both directions; bypass greys row 2; MIX automation smooth.

## Files Touched

- `plugin303/src/params.rs` — 6 new params
- `plugin303/src/dsp/warm_clipper.rs` — DSP sub-blocks, state, coefficients
- `plugin303/src/gui/panels/output_clipper.rs` — row 2 layout, link logic
- `plugin303/tests/warm_clipper.rs` — new + updated tests
- `plugin303/src/bin/eval_clipper.rs`, `eval/303/eval_clipper_aliasing.py` —
  eval sweeps
