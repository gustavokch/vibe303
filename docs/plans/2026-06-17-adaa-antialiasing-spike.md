# Measurement-first spike: in-solve ADAA for BA283 WDF aliasing

## Context

The paper *"Reducing the aliasing of nonlinear waveshaping using continuous-time
convolution"* (Parker, Zavalishin, Le Bivic, DAFx-16) is the foundational
**antiderivative antialiasing (ADAA)** paper. Its rectangular kernel gives 1st-order
ADAA (`y[n] = (F0(xₙ)−F0(xₙ₋₁))/(xₙ−xₙ₋₁)`, eq 9), its triangular kernel 2nd-order
(eq 14), and **Section 6** extends the method to implicit/feedback systems by folding
the antialiased nonlinearity `Φ` into the solve and supplying its derivative
`dΦ/dxₙ` (eq 27–29) for Newton–Raphson.

We want to know whether this helps the BA283 plugin. Exploration established:

- **The obvious application is already a closed door.** A full 2nd-order ADAA2 `tanh`
  output-saturation stage (`adaa.rs`, F0/F1/F2 LUTs, nested L'Hôpital fallbacks) shipped
  and was removed wholesale on 2026-06-16 (`7e3c8da`) — "sounded bad in practice."
- **Two adjacent targets measured NO-GO** (`docs/plans/2026-06-15-adaa-phase23-findings.md`):
  steeper decimation is useless (aliasing is already in the pre-decimation os=2 stream,
  not filter-limited), and ADAA on the transformer JA core is pointless (it adds ~0 dB
  aliasing).
- **The real, untouched aliasing source** is *in-solve harmonic foldback from the BJT
  nonlinearities* (transport `exp(vbe/Vt)` + depletion-charge `pow`) inside the implicit
  Newton solve. The `alias_sweep` harness measures the floor at **−22 dB moderate /
  −10.5 dB hot-drive** at os=2.

The only unexplored path is the paper's **Section 6** treatment applied to those in-solve
BJT terms. That is a substantial, risky reformulation of the Newton solve, so per the
user's decision we **measure the potential win cheaply before committing** to full
integration. Goal accepted: *both* payoffs are in scope (quality = lower hot-drive floor;
performance = cleaner os=1 tiers) — let the measurement decide which is real.

## Approach: two gated stages, offline only

No shipped code, no params, no UI, no tier wiring in this spike. Everything lives behind
the existing `measure` feature (and a new `adaa-insolve` feature) and is evaluated through
the `alias_sweep` harness. Stage 1 gates Stage 2.

### Stage 1 — Headroom probe (cheap, decisive gate)

Establish *how much aliasing is even reducible* by adding a brute-force high-OS reference
to the alias sweep. `QualityConfig.os` is a public field (`config.rs:54`) and
`capture_os_raw` honors it (`process/mod.rs:355`), so this is a small harness change.

- Extend `plugin/tests/alias_sweep.rs`: for each grid cell, also run the model at
  **os = 8 and os = 16** with a proper high-order decimation down to host rate, and report
  its alias floor alongside the os=2 `alias4`/`alias_raw` columns.
- The **gap** `alias(os=2) − alias(os=16)` at hot drive is the *maximum recoverable*
  alias reduction. This is the go/no-go number.
- **Gate:** if the hot-drive gap is small (say < ~6 dB), the in-solve ADAA cannot beat
  it and the spike stops here with a documented NO-GO. If the gap is large, proceed.

Files: `plugin/tests/alias_sweep.rs` only (new reference columns + a printed gap line).

### Stage 2 — Faithful in-solve ADAA1 proof-of-concept (only if Stage 1 passes)

Prototype the cheapest faithful form of Section-6 ADAA and measure it — not a shippable
integration, just enough to read the real win.

- **Scope reduction to contain cost:** ADAA1 (rectangular kernel, one antiderivative,
  midpoint fallback) on the **BJT transport exponential terms only**. Defer depletion-charge
  ADAA, the triangular kernel, and Sec 6.1 delay-elimination to a follow-up.
- **Where:** the BJT residual/Jacobian stamp inside the Newton iteration
  (`plugin/src/wdf/newton.rs`, BJT batch eval; transport math in `plugin/src/wdf/math.rs`).
  Replace each instantaneous transport term `I(vₙ) = Is·exp(vₙ/Vt)` with the ADAA1 form
  `Φ(vₙ) = (F0(vₙ)−F0(vₙ₋₁))/(vₙ−vₙ₋₁)`, `F0(v)=Is·Vt·exp(v/Vt)`, where **`vₙ₋₁` is the
  previous sample's converged junction voltage** (held fixed across the sample's Newton
  iterations; the model already stores `v_prev`). The current iterate is `vₙ`.
- **Jacobian:** stamp `dΦ/dvₙ` from eq 28, with the ill-conditioned fallback
  `½·f'((vₙ+vₙ₋₁)/2)` (eq 29) when `|vₙ−vₙ₋₁|` is below tolerance. This keeps Newton
  consistent — the antialiased current and its true derivative.
- **Gating:** behind a new `adaa-insolve` cargo feature, default off, never compiled into
  the shipped plugin. The realtime os=2 path is the only tier touched.

Files: `plugin/src/wdf/newton.rs`, `plugin/src/wdf/math.rs` (add `F0`/ADAA helper),
`plugin/Cargo.toml` (feature flag), `plugin/tests/alias_sweep.rs` (A/B baseline vs ADAA).

## Decision metrics (captured in Stage 2 sweep)

1. **Alias floor** (shipped 4th-order decimation): os=2 baseline vs os=2+ADAA vs the
   os=16 reference, across the existing 7-freq × 2-op grid. Win = fraction of the
   Stage-1 headroom gap closed, especially at hot drive.
2. **Newton convergence cost:** avgIters baseline vs ADAA (the Newton iteration counter
   already exists). The divided-difference + fallback must not blow up iteration counts.
3. **Small-signal frequency response:** drive a quiet swept sine, compare magnitude
   response baseline vs ADAA. The paper (eq 17) predicts a ≈½-sample group delay → slight
   HF rolloff; we must confirm it stays within ~0.5 dB to fs/4 and does not destabilize
   the cross-coupled feedback.

**Overall Go/No-Go:** ADAA goes to a productionization plan only if it closes a
meaningful fraction of the os=2→os=16 headroom at hot drive **and** avgIters stays
bounded **and** the small-signal FR shift is acceptable. Otherwise: documented NO-GO,
matching the discipline already applied in Phase 2/3.

## Out of scope (deferred to a follow-up plan, contingent on a GO)

Depletion-charge ADAA, triangular (2nd-order) kernel, Sec 6.1 feedback-delay elimination,
tier/param/editor wiring, multi-tier tuning, convergence hardening for production. The
WaveNet LeakyReLU path (learned, would need retraining) and the EQ (linear) are not
aliasing targets.

## Verification

- Build gated: `cargo build --features measure,adaa-insolve --release` (and confirm the
  default `cargo build` is untouched — no `adaa-insolve` symbols leak into the shipped
  plugin).
- Stage 1: `cargo test --features measure --release --test alias_sweep -- --nocapture`
  prints the os=2 vs os=8/16 reference table + headroom gap line.
- Stage 2: same command with `--features measure,adaa-insolve` prints the
  baseline-vs-ADAA A/B table (alias floor, avgIters) + the small-signal FR comparison.
- Existing assertions in `alias_sweep.rs` (harness fidelity, Phase 2/3 gates) must still
  pass for the non-ADAA columns — the spike must not perturb the baseline measurement.
- Record the verdict (GO with numbers, or NO-GO with the headroom gap) in a new
  `docs/plans/2026-06-17-insolve-adaa-findings.md`, consistent with the Phase 2/3 doc.
