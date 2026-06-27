# F4 handoff prompt — High (WDF-lite) tier

Paste the block below into a fresh session to start F4.

---

Pick up the 303 fidelity-tier work at **Phase F4 (High tier)**. Branch
`feat/phase2-303-fidelity-tiers` (pushed, base `main`, clean). Read `HANDOFF.md` first, then this.
Use `superpowers:executing-plans`; brainstorm the High definition (below) before coding.

## Where things stand
- **Low** = TPT (polyBLEP saw + `TptLadder`), ships. Spectrally near-uncorrelated with SPICE
  (corr ≈ 0.27) — it's a regression guard, not a fidelity target.
- **Ultra** = nodal circuit chain: `Vco303` (21-node relaxation oscillator) → `Os4x` →
  `LadderVcf` (nodal diode-ladder VCF). Ships under a **spectral character gate**
  (`tests/ultra_spectral.rs`): Welch gold-power-weighted log-spectral distance + dB-spectrum
  correlation. Achieved **LSD 4.42 dB, corr 0.977**.
- **High** currently routes to `TptLadder` (identical to Low) — `voice.rs::tick` matches
  `Tier::Low | Tier::High => self.filter.tick(...)`. **F4 makes High a distinct, cheaper-than-Ultra
  tier.**
- Acceptance is **spectral, not sample-exact ESR** — sample-exact −30 dB vs ngspice is structurally
  unreachable for a real-time chain (design doc F3-VCO V4/V5). `tests/ultra_esr.rs` is `#[ignore]`
  informational only.

## F4 goal
A High tier between Low and Ultra: cheaper than Ultra, more faithful than Low, spectral budget
looser than Ultra. The dominant Ultra cost is the **21-node VCO Newton** (~3 iters/sample) plus the
4× OS (4 LadderVcf solves/sample).

## Recommended High definition (confirm in brainstorm)
**High = polyBLEP saw → `LadderVcf` @ 2× OS** (skip the nodal VCO — the biggest cost — keep the
nonlinear diode-ladder character). Alternatives to weigh: Ultra chain at 2× OS with reduced Newton
iters/looser tol; or a reduced-node ladder. Recommendation skips VCO Newton entirely while keeping
the audible nonlinear filter, so it should land between Low and Ultra on corr at a fraction of
Ultra's cost.

## Tasks (TDD, batch F4.1–F4.3, commit each)
1. **F4.1 — wire High.** In `voice.rs` route `Tier::High` to its own path: `osc.tick()` (polyBLEP)
   → `Os2x` → `LadderVcf`. Add an `Os2x` + a second `LadderVcf` field (or reuse with care — the
   Ultra ladder runs at 4× sr, High at 2× sr, so they need separate instances at the right
   `LadderVcf::new(sr * factor)`). Keep zero-alloc.
2. **F4.2 — spectral gate.** `tests/high_spectral.rs` reusing the Welch LSD+corr helpers from
   `tests/ultra_spectral.rs` (extract them to a shared test module or a `pub` lib helper to avoid
   copy-paste). Render High, measure LSD/corr vs the same `eval/303/gold/voice_A2_mid.f32`. Set the
   budget to achieved + margin (expect looser than Ultra; corr likely 0.6–0.9 since the polyBLEP saw
   ≠ circuit saw). Cross-check with `eval/303/coherence.py` via `render303 --tier 1`.
3. **F4.3 — cost budget.** Report High's Newton iters/sample + OS factor vs Ultra; document the
   cost/fidelity gap in the design doc. Extend `tests/no_alloc.rs` with a High-voice gate.

## Gotchas
- `LadderVcf` corner saturates at a **~600 Hz ceiling** (48 kHz); `set_cal(1.0)` is the spectral
  LSD optimum — don't brighten via bias. Same limit applies to High.
- The spectral metric weights bins by the **gold's power** (so the noise floor above the corner
  doesn't dominate) — reuse it, don't reinvent with a flat-band LSD.
- All hot paths zero-alloc; warm-start the Newton solver outside the `assert_no_alloc` guard.
- `cargo test -p plugin303` = 57 passed, 5 ignored before F4. `#[ignore]` tests are diagnostics
  (`dump_trajectory`, `measure_pitch_map`, `corner_vs_cutoff`, `brightness_sweep`) + the
  informational `ultra_esr`.

## After F4
F5 = tune all tiers, click-free tier switching (settle-then-crossfade), finalize the 2×/4×/8× OS
switch param, host PDC (`voice.latency_samples()`), finish-branch / PR. Optional Ultra headroom
(active T20/T21/Q9 VCF stages + per-note VCO pitch secant) to tighten Ultra's 4.42 dB LSD remains
deferred.
