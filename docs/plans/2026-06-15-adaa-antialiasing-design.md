# ADAA Antialiasing — Design

Date: 2026-06-15
Status: approved, implementing on `feat/adaa-antialiasing`
Source report: `docs/adaa/adaa-rust-dsp.md`

## Problem / motivation

The plugin's only anti-aliasing today is oversampling: the NeuralNet path uses a 63-tap Kaiser
half-band FIR at 2x (sub-96 kHz only); the WDF path solves the implicit circuit at os=1/2 with a
4th-order RBJ IIR Butterworth decimator at 0.45·host. The ADAA report proposes antiderivative
antialiasing as a cheaper, mathematically-exact alternative for nonlinear stages.

**Critical constraint discovered during design:** ADAA (as in the report) applies to *memoryless
waveshapers*. BA283 has none — the NeuralNet backend is a recurrent LSTM, and the WDF backend is a
high-order coupled implicit solve plus a Jiles-Atherton transformer (hysteresis ODE). The report's
stateful-ADAA techniques (pre-solved 1-D LUT; coefficient warping) do not map onto a many-state
dynamic circuit. So ADAA cannot be retrofitted onto the existing nonlinearities — it must be applied
to *new or memoryless* stages.

## Approach

Three workstreams, each tone change behind an ON/OFF toggle (A/B switchable):

1. **New ADAA2 output saturation stage** (primary) — a memoryless asymmetric soft-clip, post-backend
   pre-Mix, antialiased by an ADAA2-LUT core. The clean textbook ADAA target.
2. **Transparent OS/decimation filter tuning** (measure-first) — improve the existing FIR/IIR only
   where measurements show a win without latency regression.
3. **ADAA the trafo saturation** (spike) — apply the ADAA2 core to the JA anhysteretic Langevin curve;
   go/no-go based on the trafo B-H/THD harness.

ADAA order: **ADAA2** (triangular kernel, 2nd-antiderivative LUT) from the start.

## Architecture

- `plugin/src/adaa.rs` (new): `Adaa2Sat` — fixed-shape asymmetric soft-clip; `F1`/`F2` antiderivative
  LUTs (8192 pts, trapezoidal, built once off-thread, `Arc<[f32]>`, no runtime rebuild → no arc-swap);
  ADAA2 difference equation with nested L'Hôpital fallbacks; clamp + linear-interp + `get_unchecked`;
  `#[inline] process_sample`, `reset`. No heap/lock in the hot path.
- `plugin/src/dsp.rs`: per-channel `Adaa2Sat` between model output and dry/wet mix; reset on the same
  events as other DSP state.
- `plugin/src/lib.rs`: `sat_on: BoolParam`, `sat_amount: FloatParam` (and the trafo spike's
  `trafo_adaa_on` under `trafo-core`); block/per-sample plumbing mirrors existing params.
- `plugin/src/editor.rs`: one new control row (knob + toggle pill); editor height bump.

## Data flow

input → drive/bias → backend (LSTM | WDF, internally oversampled) → **[ADAA2 sat stage, if sat_on]**
→ dry/wet mix → output gain. The sat stage runs at host (base) rate; ADAA2 suppresses its own
aliasing without additional oversampling.

## Error handling / edge cases

- ADAA2 ill-conditioning: three nested L'Hôpital branches (`|x0−x1|`, `|x1−x2|`, `|x0−x2|` < tol),
  degrading ADAA2 → ADAA1 → midpoint `f`. Unit-tested for continuity.
- Index safety: input hard-clamped to `[U_MIN, U_MAX]` before `get_unchecked`; checked path under
  `debug_assertions`.
- Toggle clicks: OFF path bypasses entirely and is bit-exact vs today; re-engage ramps amount / short
  xfade like existing toggles.
- Param-IDs: `sat_on`/`sat_amount` use explicit string `#[id=...]`, so adding them is additive —
  existing preset/automation bindings are unaffected (no break, unlike the trafo-dials *rename*).

## Testing

- Unit: DC→exact `f`; each L'Hôpital branch; no-alloc in `process_sample`.
- Spectral: swept-sine alias rejection vs a 16x-oversampled reference of the same shape.
- Regression: every new toggle OFF ⇒ bit-exact (≤1 ULP) vs pre-change build.
- Builds: `cargo test` default and `--no-default-features` both green and warning-clean.
- GUI: `gui_standalone` — new row visible, not HiDPI-clipped.
- Eval harnesses: `eval/wdf_vs_spice`, `eval/wdf_profile` (Phase 2), `eval/trafo_core` (Phase 3).

See `/home/gustavo/.claude/plans/1-read-the-report-glimmering-dawn.md` for the phased execution plan.
