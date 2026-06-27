# HANDOFF — 303 Fidelity Tiers, Phase F3 (Ultra diode-ladder VCF)

**Branch:** `feat/phase2-303-fidelity-tiers` (clean, all work committed; remote
`gustavokch/vibe303`, base `main`).
**Plan:** `docs/plans/2026-06-27-303-fidelity-tiers-implementation.md` — F1+F2+F3 detailed to
F1-granularity; F4/F5 still outlined.
**Design + findings:** `docs/plans/2026-06-27-303-fidelity-tiers-design.md`.

> Supersedes the earlier M1–M8 milestone plan (M1 + M2 filter fidelity are done and live in the
> codebase; the project pivoted to the ESR-gated **fidelity-tier** axis). Phase-1 SPICE reference
> is complete — see the appendix.

**To continue:** fresh session → `superpowers:executing-plans` against the plan above, starting at
**Phase F3** (F1 and F2 are DONE). Batch = F3.1–F3.3 first.

---

## Goal

A 3-tier 303 voice fidelity axis gated by a time-domain ESR harness vs a whole-voice SPICE
reference. Low=TPT (ships), High=WDF-lite, Ultra=nodal diode-ladder. Ultra must be
ear-indistinguishable from SPICE (ESR ≤ −30 dB). All tiers ≥2× oversampled + ADAA, zero-alloc.

## Current Progress

**F1 (measure the gap) — DONE.**
- `eval/303/esr.py` (+`test_esr.py`) — BA283 lag-aligned ESR/NRMSE.
- `plugin303/src/bin/render303.rs` — headless raw-f32 renderer (`--note --sr --secs --cutoff
  --reso --out`; add `--tier` in F3.5).
- `eval/303/voice_ref_spice.py` (+`test_voice_ref.py`) — chained-deck SPICE ref: vco303 `.tran`
  → resample → vcf303 `.tran` → linear AD VCA. Pitch via secant `solve_cv`. Helpers
  `itail_to_hz`, `rreso_to_reso`.
- `eval/303/compare_voice.py` + `voice_esr_baseline.json` — per-op-point ESR table.
- **Result:** Low/TPT ESR ≈ **+2.3 dB** (near-uncorrelated with SPICE). Aliasing floor −10 dB.

**F2 (tier scaffold) — DONE. `cargo test -p plugin303` = 28 passed (7 suites).**
- `dsp/filter.rs` — `Filter303` trait + `Tier` enum (nih-plug `Enum`, id "tier").
- `dsp/oversample.rs` — `Os2x/Os4x/Os8x` polyphase FIR (Kaiser-sinc L=65), zero-alloc, latency
  32/48/56.
- `dsp/adaa.rs` — `Adaa1Tanh` (F=ln cosh) + in-test FFT alias metric.
- cutoff/reso wired into `Voice`+`render303`; `EnumParam<Tier>` read block-rate in `process()`.
- `tests/voice_esr.rs` — Rust ESR gate vs committed gold `eval/303/gold/voice_A2_mid.f32`
  (measured 2.44 dB, budget 3.5). `eval/303/gen_gold.py` regenerates gold.

**F3 (Ultra) — PLANNED (F3.1–F3.6 full granularity), not started.**

## What Worked

- **PySpice `.simulator` DOES drive ngspice-46 for `.tran`** (VCO self-starts with ramped rails).
  VCF transient uses subprocess ngspice + inline PWL drive.
- **Secant `solve_cv`** pitches the SPICE VCO to the exact Rust `midi_to_freq` — pitch mismatch is
  NOT fixed by ESR lag-align, must match first.
- **RMS-normalise both signals before ESR** (`metrics()` has no gain norm).
- **Oversampler:** odd FIR `L≡1 mod 4` → integer per-stage latency; cascade stages process the
  stream *sequentially* (single inner `Os2x`); decimator reads even phase (`hist[k+1]`).
- **nih-plug `Enum` derive:** `use nih_plug::prelude::Enum;` + bare `#[derive(Enum)]` (path-form
  derive does NOT emit the impl).
- Studied the real BA283 source `~/Git/ba283_wdf/main.cpp` to ground F3.

## What Didn't Work (don't repeat)

- **Calibrating Low/TPT cutoff/reso to SPICE is a wash** — ESR stays ~+2.5 dB. The gap is
  **topological** (polyBLEP saw + TPT vs circuit saw + diode ladder), not parametric. Low's gate
  is a regression guard, not a fidelity target. Fidelity = Ultra (F3).
- **ADAA "at 2×" test** ≈ 0 dB (the 2× FIR already removes that alias). Test ADAA at **1×**, near
  Nyquist/2 (f0=11 kHz) where the win is robust (−4.4 dB). f0 dividing sr is a degenerate metric.
- The design's **"WDF"** label is wrong — BA283 is nodal-MNA + Newton, not wave-digital. F3 plan
  corrects this.
- **mem0 MCP quota-exhausted** (resets 2026-07-01) — search/save error; ignore.

## Next Steps (F3, in order)

1. **F3.1** `dsp/expmath.rs` — port `fast_exp`/`fast_log` (`~/Git/ba283_wdf/main.cpp:129-160`).
   `f64::to_bits/from_bits`; critical underflow guard `k<=-1023 => 0.0`. Tests vs `std` <1e-9.
2. **F3.2** `dsp/diode.rs` — 1N4148 `stamp(v)->(I,g)` (vcrit-linearized) + `charge(v)->(Q,C)`
   (FC=0.5). Params in `eval/303/_vcf303_netlist_gen.py` (IS=1.42e-11, N=1.7, CJO=1.74e-12,
   VJ=0.75, M=0.333).
3. **F3.3 (RISK)** `dsp/ladder_wdf.rs` — nodal MNA + Newton, 8-diode + 4-cap ladder (topology in
   plan), gmin homotopy, fixed-N dense LU on stack, zero-alloc, warm-start. `impl Filter303`.
   Validate corner-tracks-bias + slope ≈ −21 dB/oct vs `eval/303/vcf303_gold.json`.
4. **F3.4** ADAA-vs-4×OS bench (resolves F1.5); implement winner.
5. **F3.5** wire `Tier::Ultra` → `LadderVcf` inside `Os4x`; `render303 --tier`;
   `tests/ultra_esr.rs` gate ≤ −30 dB (Ultra gold = same SPICE waveform as Low gold); coherence.
6. **F3.6** no-alloc/denormal guard, tune, F3 findings + F4 cost budget.

**Gating risk:** F3.3 per-sample Newton convergence in the RT path. Fallbacks if 4×OS misses
−30 dB: 8×OS (switch exists), ADAA2 (F3.4), or expand scope to active-stage nonlinearities.

## Commands

```bash
cargo test -p plugin303                                  # 28 tests (will grow)
cargo run --bin render303 -p plugin303 -- --out /tmp/r.f32
uv run --no-project --with PySpice --with numpy --with pytest python -m pytest eval/303/test_*.py -q
uv run --no-project --with PySpice --with numpy python eval/303/compare_voice.py   # ~3 min
```
ngspice noise (unsupported-v46, MPF104 param warnings) is harmless — filter with the grep pattern
in the `compare_voice` invocations.

---

## Appendix — Phase 1 SPICE reference (COMPLETE)

Detail in `eval/303/VCF_MAP.md`. ngspice is ground truth (ACME = NO-GO for both 303 circuits).
- **VCO** (`circuit/vco303.toml`): OSCILLATING + 1 V/oct (CV 4/5/6 → 231/477/1000 Hz). Self-start
  = ramp rails 0→V over 1 ms (PWL). Solver: `method=gear, rshunt=1e12, gmin=1e-10, reltol=1e-3,
  abstol=1e-9, vntol=1e-6, itl4=1000, trtol=7`.
- **VCF** (`circuit/vcf303.toml`): 4-rung diode ladder (8×1N4148 + .033/.033/.033/.018µF) + T38
  diff input + T20/T22 gain + T21 buffer + active Q9 CE-amp resonance. Gold:
  `eval/303/vcf303_gold.json` (f_deep ~6 kHz@32µA, reso peak ~1 kHz, slope ~−21 dB/oct, tap step
  ~14 dB). `run_vcf303_spice.py --op|--reso|--poles|--json`.
- ngspice gotchas: PySpice can't drive ngspice-46 for **AC** (stamp + subprocess + parse `wrdata`);
  unit suffixes parse as atto → plain numbers; first deck line eaten as title → `* comment`;
  `np.ptp(x)` not `ndarray.ptp()`; VCF AC bottom sink must be ideal current sink (open at AC).
