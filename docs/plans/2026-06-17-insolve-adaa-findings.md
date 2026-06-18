# In-solve ADAA spike — findings (Stage 1 GO, Stage 2 NO-GO)

Measurement record for `docs/plans/2026-06-17-adaa-antialiasing-spike.md` (as refined by the
second-opinion critique `~/.claude/plans/unaudhd-review-critique-and-flickering-river.md`). Consistent
with the Phase 2/3 NO-GO discipline (`docs/plans/2026-06-15-adaa-phase23-findings.md`).

**Bottom line:** there is large recoverable aliasing headroom at os=2 (Stage 1 = GO), but the plan's
**per-residual-term in-solve ADAA1** does not capture it — it makes the alias floor *worse* in most
cells while exploding Newton cost (Stage 2 = NO-GO). The failure is the *formulation*, not convergence
and not the feedback delay. The only literature-backed path that remains (Albertini per-**port**
modular ADAA) is a substantial WDF reformulation, out of scope for a measurement spike.

Harnesses (offline, `measure` / `adaa-insolve` features, never shipped):
- `plugin/tests/headroom_probe.rs` — Stage 0 (attribution) + Stage 1 (headroom gate).
- `plugin/tests/adaa_insolve.rs` — Stage 2 (alias A/B, Newton cost, small-signal FR, DC bias).

Both fix the two Stage-1 rig bugs the critique flagged: analytic internal-rate input via the new
`WdfModel::capture_internal_direct` (no `prev_in` lerp contamination), and a steep Blackman-Harris
windowed-sinc decimator for the 8:1/16:1 references (the shipped Butterworth is 2×-tuned and would
self-alias past 4:1).

---

## Stage 1 — headroom probe: GO

`cargo test --features measure --release --test headroom_probe -- --nocapture`

Alias floor (dB, lower = cleaner) at os = 2/4/8/16, analytic input + steep sinc decimation:

```
op            f0       os2       os4       os8      os16    gap2-16     gap2-4
moderate   0.240    -26.05    -42.16    -44.11    -44.15     +18.09     +16.11
moderate   0.360     -8.32    -29.89    -39.14    -39.59     +31.26     +21.57
hot        0.120    -21.13    -31.08    -38.76    -44.39     +23.27      +9.95
hot        0.300     -3.67    -18.87    -24.08    -32.38     +28.71     +15.20
hot        0.360      0.17    -12.55    -23.05    -30.60     +30.76     +12.71
```

- **Max recoverable headroom (hot, os2−os16) = 30.76 dB** — far above the ~6 dB go/no-go gate.
- **Realistic ADAA1 target (hot, os2−os4) ≈ 15 dB** (one OS octave, per Albertini/DAFx-2020).
- Reference-rig sanity assertions passed (os16 floor monotonically below os2) → the decimator is not
  self-aliasing and the gap is trustworthy.

**Stage 0 attribution** (caps Full vs Off): at os=2 hot, removing the depletion/diffusion charge model
makes the floor only a few dB *cleaner* (e.g. f0=0.36: +0.17 → −3.38; f0=0.30: −3.67 → −8.56), and the
caps-off floor is still terrible. ⇒ **transport-exp dominates the foldback**; depletion is a secondary
aliaser. The plan's transport-only ADAA1 scope was the right place to look.

## Stage 2 — per-residual-term in-solve ADAA1: NO-GO

`cargo test --features measure,adaa-insolve --release --test adaa_insolve -- --nocapture`

ADAA1 on the BJT transport exponentials inside the Newton residual: each `Is·(exp(v/Vt)−1)` term
replaced by its first-order antiderivative divided difference against the previous sample's converged
junction voltage, with the **exact** eq-28 Jacobian (`gef=(exp(v/Vt)−ef_eff)/Δ`) and the eq-29 midpoint
fallback. Implementation: `adaa1_exp` (math.rs) + `compute_bjt_preexp_batch_adaa` (bjt/mod.rs), gated
behind `adaa-insolve`, toggled by `WdfModel::set_adaa`. The bit-exact realtime path is untouched
(golden `test_wdf` 22/22 still passes; default build has no `adaa` symbols).

Alias floor + Newton cost (os=2 baseline vs ADAA@30-iter cap vs ADAA@400-iter cap vs os=16 ceiling):

```
op            f0     base   adaa30   adaaHi     os16     Δhi   clos%    itB   itHi   mxHi
moderate   0.120   -56.16   -44.68   -44.30   -57.76  -11.86  -743.2   5.99  10.98    400
moderate   0.180   -50.33   -32.22   -32.22   -52.10  -18.11 -1023.2   6.47  13.49     19
moderate   0.300   -23.44    -8.93    -8.93   -41.47  -14.50   -80.4   7.55  26.87     18
hot        0.300    -3.67    -8.80    -8.78   -32.38   +5.12    17.8   9.99  57.93    400
hot        0.360     0.17    -2.56    -2.59   -30.60   +2.76     9.0   10.70  35.80    400
```

Three independent reasons it fails:

1. **It does not reduce aliasing — it usually increases it.** Converged ADAA1 (@400 iters) raises the
   floor in every moderate cell (Δ −4 to −18 dB) and most hot cells; it helps in only 4 cells and by
   ≤ +5 dB. Hot headroom "closed" spans −42 % … +18 % of the os2→os16 gap.
2. **Not a convergence artifact.** ADAA@30 and ADAA@400 give the *same* floor (compare `adaa30` vs
   `adaaHi`), so the bad number is the formulation's true output, not under-iterated noise.
3. **Not the p/2 feedback delay either.** Small-signal FR shift is tiny (max **0.272 dB** at 20 kHz),
   the loop stays stable (all gains finite), and DC is preserved (Δ = 5.6e-9). So Holters'
   state-space delay compensation — the fix for the feedback-delay failure mode — is **not indicated**;
   that is not the failure mode here.

Cost is also prohibitive: avgIters roughly triples and `maxIters` pins at the cap (30, then 400) on the
hot cells — the modified residual genuinely will not converge on many samples even with 400 iterations.

**Root cause.** Antialiasing each transport term *in place inside the residual* replaces the
instantaneous device current with its average over the sample step. That shifts the BJT operating point;
the BA283's global DC feedback then fights the shift, and the net spectral result is *more* in-band
foldback, not less. This is exactly the unvalidated step the critique flagged: Parker Sec 6 antialiases
the nonlinearity Φ with respect to *its own argument*, and Albertini antialiases at the WDF **port** —
neither antialiases per-term inside a shared MNA residual. The per-term-in-residual form is the plan's
own simplification, and it is measured-wrong.

## Decision

**NO-GO** for in-solve ADAA at the per-residual-term granularity. The headroom is real (Stage 1), but
this formulation cannot reach it and is far too expensive. No shipped code, no params, no tier wiring —
the discipline of Phase 2/3 holds.

The spike code (`adaa-insolve` feature, off by default, never compiled into the plugin) is retained as
the substrate for the one remaining literature-backed option, should it ever be revisited:

- **Albertini, Bernardini, Sarti — "ADAA in Nonlinear Wave Digital Filters" (DAFx-2020) + JAES-2021.**
  Per-**port** modular ADAA, validated on a BJT-in-WDF with an iterative solver — antialias the device
  at its port, decoupled from the rest of the circuit, rather than per-term inside the residual. This is
  a substantial reformulation of how the BJTs couple into the solve, **not** a measurement spike, and is
  explicitly out of scope here.
- Holters' delay compensation (DAFx-19 / *Applied Sciences* 10(1):20, 2020) is **not** the lever — the
  measured FR/stability is already clean, so there is no feedback delay to compensate.

Practical aliasing mitigation today remains plain oversampling (the os=2 → os=4 step alone recovers
~10–15 dB at hot drive, per Stage 1), which the existing tier machinery already supports.
