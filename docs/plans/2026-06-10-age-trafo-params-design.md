# Design: `age` and `trafo` parameters for the BA283 WDF model

Date: 2026-06-10

## Problem / motivation

Add two vintage-character controls to the WDF backend:

- **`age`** (0.00–1.00, default 0.00) — random per-component drift emulating aged passives.
- **`trafo`** (0.00–1.00, default 0.50) — magnetic-core saturation + hysteresis in the LO1166
  output transformer.

Exploration found the transformer is currently a **pure linear coupled inductor** (`gl` companion
from `lp=1.9, ls=5.36, k=0.9995`). There is no saturation/hysteresis anywhere, so `trafo` is
**net-new nonlinear physics** in the Newton loop — "0.5 = baseline" means a chosen realistic LO1166
amount, with **0.0 = today's exact linear behaviour**. There is **no SPICE/ACME ground truth** for
the nonlinear core (the reference netlist transformer is also linear).

## Decisions (confirmed with the user)

- **age scope:** caps + resistors only (no transformer inductance drift).
- **age determinism:** per-instance stable seed scaled by the knob + a re-roll control; seed persisted.
- **age bands (type-aware):** resistors ±5%, film caps ±5%, electrolytics directional −20%/+2%
  (dry-out), split by a 1 µF farad threshold.
- **trafo model:** physical **Jiles–Atherton** magnetizing branch inside the Newton solve.
- **trafo range:** 0 = linear (bit-exact today), 0.5 = realistic LO1166, 1 = heavy.
- **trafo rollout:** **spike-first** behind a `trafo-core` cargo feature + a feature-gated param;
  GO/NO-GO from the validation gates before it ships as a first-class knob.

## age — architecture (shipped)

Reuses the existing cheap "mutate conductance then rebuild G0" pattern (`set_bias`/`set_rgain`):

- `SplitMix64` PRNG (no new crate) + `AgePattern::from_seed` generate fixed per-unit signed
  deviations; `age` scales them at apply time so the unit identity is invariant under the knob.
- `WdfModel` stores pristine `res_base_g`/`cap_base_far` captured at build; `set_age` re-derives
  every `res.g` and `cap.geq` from base × `(1+age·dev)` then `build_g0`. The three parameter-derived
  legs (RV1 upper/lower, R_KJ) fold the aged factor in via `set_bias`/`set_rgain`.
- nih-plug: `age` FloatParam + `age_reroll` BoolParam + persisted `age_seed: Arc<AtomicU64>`
  (`#[persist]`). Block-rate apply ordered **tier → seed/age → bias/rgain**; re-applied after any
  rebuild. Both channels share seed+age → coherent stereo. Block-rate only, alloc-free (RT-safe).
- **age=0 is bit-identical** to today (drift factor exactly 1.0).

## trafo — architecture (spike, feature `trafo-core`)

A nonlinear **magnetizing shunt** in parallel with the linear inductor companion. Leakage / winding
coupling / DCRs stay linear in `G0`; only the magnetizing path saturates (Holters & Zölzer, DAFx-16:
`M` as state variable, ODE form, folded into the implicit Newton solve).

- **Deviation form** (key choice): the branch current is `trafo · (i_JA(phi) − phi/lp_eff)`, where
  `lp_eff` is the JA small-signal magnetizing inductance derived from the constitutive constants. At
  low drive `i_JA ≈ phi/lp_eff` so the branch ≈ 0 — continuity with the linear model — and it grows
  only as the core saturates. At `trafo=0` the branch is gated off entirely (`ja_active=false`),
  giving bit-exact linear behaviour.
- **Single core state** `M`; flux `phi` integrated trapezoidally. Given `B=phi/k_phi`, a short scalar
  Newton solves `mu0(H+M(H))=B`; the companion `(i_branch, g_m)` is stamped on row `N_PA` (the
  magnetizing branch returns to the rail `N_A`). `delta` (field-rate sign) is frozen per sample so
  the outer Jacobian stays continuous across reversals.
- **One structural change:** when active, `N_PA` joins the Schur nonlinear N-set (6→7); the `n==6`
  fast path falls to the generic solver. Inactive → partition byte-identical to today.
- `set_trafo` re-partitions only on the 0⇄>0 boundary; no `G0` rebuild (the branch is purely in the
  per-iter residual/Jacobian). nih-plug: feature-gated `trafo` FloatParam, block-rate glide with an
  epsilon-snap to exactly 0 for a clean partition flip.

### Validation (no nonlinear ground truth) → GO/NO-GO

1. **Linear-at-0 bit-exactness** + N-set stays 6 (hard gate).
2. trafo>0 changes output, stays finite, deterministic.
3. **Convergence:** avg Newton iters at 0.5 within +1.5 of the linear baseline.
4. **B-H plausibility** (`eval/trafo_core/plot_bh.py`): loops closed, monotone-saturating, area grows
   with `trafo`.
5. Voicing the JA constants (`m_s,a,alpha,k,c` + geometry) so 0.5 ≈ realistic and 1 ≈ heavy is the
   GO step (needs listening + the B-H/THD plots), tracked separately.

## Files

- `plugin/src/wdf.rs` — SplitMix64/AgePattern/set_age (age); JaCore/lo1166/ja_branch/ja_commit/
  set_trafo + Newton/residual/Schur/history stamps (trafo, all `#[cfg(feature="trafo-core")]`).
- `plugin/src/dsp.rs`, `plugin/src/lib.rs` — param plumbing.
- `plugin/Cargo.toml` — `trafo-core` feature.
- `plugin/tests/` — `test_wdf.rs`/`test_params.rs`/`test_process.rs`/`test_wdf_noalloc.rs` (age),
  `test_trafo.rs` (trafo, feature-gated).
- `eval/trafo_core/plot_bh.py` — B-H plausibility harness.
