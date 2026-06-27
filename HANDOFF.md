# HANDOFF — 303 Fidelity Tiers, F1–F5 COMPLETE (Low / High / Ultra)

**Branch:** `feat/phase2-303-fidelity-tiers` (clean, all work committed; remote
`gustavokch/vibe303`, base `main`).
**Plan:** `docs/plans/2026-06-27-303-fidelity-tiers-implementation.md` — F1+F2+F3 detailed to
F1-granularity; F4/F5 still outlined.
**Design + findings:** `docs/plans/2026-06-27-303-fidelity-tiers-design.md`.

> Supersedes the earlier M1–M8 milestone plan (M1 + M2 filter fidelity are done and live in the
> codebase; the project pivoted to the ESR-gated **fidelity-tier** axis). Phase-1 SPICE reference
> is complete — see the appendix.

**To continue:** F1, F2, F3 + F3-VCO, **F4 (High)**, and **F5 (tune + ship)** are DONE. The three-
tier axis is complete, gated, and shippable. Only the **optional Ultra active-stage headroom**
remains (deferred) — see Next Steps. Use `superpowers:executing-plans` against the plan above.

---

## Goal

A 3-tier 303 voice fidelity axis vs a whole-voice SPICE reference. Low=TPT (ships), High=WDF-lite,
Ultra=nodal circuit (VCO + diode-ladder VCF). **Ultra acceptance redefined (F3-VCO V5):** a
*spectral character* match (Welch LSD + dB-spectrum correlation), not sample-exact ESR ≤ −30 dB —
that is structurally unreachable for a real-time bespoke chain vs ngspice's adaptive solver. All
tiers ≥2× oversampled, zero-alloc.

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

**F3 (Ultra) — DONE. Ships under a spectral gate. `cargo test -p plugin303` = 57 passed, 5 ignored.**
- F3.1 `dsp/expmath.rs` — `fast_exp/fast_log/fast_pow_pos` (BA283 port).
- F3.2 `dsp/diode.rs` — 1N4148 `stamp`/`charge`.
- F3.3 `dsp/ladder_wdf.rs` — nodal diode-ladder VCF (`LadderVcf impl Filter303`), Newton+gmin,
  trapezoidal caps, stack LU, zero-alloc. **Structural limit:** corner saturates ~600 Hz ceiling
  (no active stages) — see design doc.
- F3.4 `tests/ladder_alias.rs` — OS-only decision (ADAA N/A for the implicit solve); ladder
  self-anti-aliases for steady tones.
- F3.5 — `Tier::Ultra` wired voice → `Os4x` → `LadderVcf`; `render303 --tier`.

**F3-VCO (V1–V5) — DONE.** User pivoted Ultra to model the **circuit VCO** (whole-voice gold =
circuit-VCO saw → VCF; polyBLEP saw can't reach it).
- V1 `dsp/bjt.rs` (Gummel-Poon NPN/PNP), V2 `dsp/jfet.rs` (MPF104), V3 `dsp/vco.rs` (21-node nodal
  relaxation oscillator, `dsp/linalg.rs` generic LU). Self-oscillates, zero-alloc.
- V4: `Vco303` chained into Ultra voice + pitch cal (~1.4 %). **Two structural walls** (design doc):
  VCO ~1.4× fast vs ngspice + reset-shape diff; VCF corner ceiling.
- **V5 (acceptance redefined → spectral, user decision):** sample-exact ESR vs ngspice is
  unreachable for a real-time bespoke chain. New gate `tests/ultra_spectral.rs` (+ offline
  `eval/303/coherence.py`): Welch gold-power-weighted LSD + dB-spectrum corr. **Achieved LSD =
  4.42 dB, corr = 0.977** (budget LSD≤5.5, corr≥0.95). Ultra ships circuit-faithful.
- F3.6: Ultra-chain no-alloc gate + NaN/bounded sweep guard.

**F4 (High / WDF-lite) — DONE.** High = polyBLEP saw → `LadderVcf` @ 2× OS (skips the costly nodal
VCO Newton, keeps the nonlinear diode-ladder filter). Sits between Low and Ultra on both axes.
- F4.1 `voice.rs` — `ladder_hi`+`os2` fields, `Tier::High` path, cutoff/reso fan-out, High latency.
- F4.2 `tests/high_spectral.rs` — spectral gate **LSD ≤ 32, corr ≥ 0.95** (achieved 28.78 dB /
  0.971; Low's 42.16 / 0.931 both fail → real discriminator). Welch LSD+corr helpers extracted to
  `tests/common/mod.rs`; `ultra_spectral.rs` refactored onto it.
- F4.3 `cost_bench` + no-alloc High gate + design-doc cost table. **High ~4× cheaper than Ultra and
  RT-viable (29 % of one core); Ultra is 117 % — offline-grade per voice.**

**F5 (tune + ship) — DONE.** Three-tier axis complete, gated, shippable.
- **OS-switch param dropped (user decision): OS is tier-intrinsic** (High 2×, Ultra 4×), each ladder
  calibrated to its rate. A global OS param would conflict with per-tier tuning.
- F5.1 click-free **equal-power crossfade** (~10 ms) on `set_tier`. Each tier owns independent osc
  state (`osc_hi` Saw added) so Low+High render together during a fade without phase double-advance.
  `set_tier_immediate` for the initial/persisted selection (no fade). Gate `tests/tier_switch.rs`
  (switch-window step ≤ 2.5× steady, 4 pairs) + `no_alloc` dual-path crossfade gate.
- F5.2 host **PDC**: `voice.latency_samples()` reported via `set_latency_samples` in `initialize` +
  on tier change in `process` (guarded on prev). Low 0 / High 2×OS / Ultra 4×OS delay.
- `cargo test -p plugin303`: **65 passed, 8 ignored.**

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

## Next Steps

F1–F5 are DONE. The three-tier fidelity axis (Low / High / Ultra) is complete, gated, click-free,
and PDC-reported. Only optional polish remains:

1. **Ultra headroom (optional, deferred)** — close the residual ~4.4 dB LSD by modeling the
   **active VCF stages** (T20/T21 readout + Q9 resonance) so the corner clears its ~600 Hz ceiling,
   plus a **per-note VCO pitch secant** (this solver runs ~1.4× fast vs ngspice). This is the
   "expand VCF active stages" path. Only needed if a tighter spectral match is wanted; would lift
   High with it (shared `LadderVcf` core). 8× OS for Ultra would ride on this work.
2. **Ship** — finish-branch / PR (in progress), then host-test in a DAW (VST3/CLAP), wire the
   accent/glide/decay levers (M4/M6, separate axis from fidelity tiers).

**Key gotchas for the next session:**
- VCF corner has a **~600 Hz structural ceiling** at 48 kHz (passive ladder only) — raising
  `LadderVcf::set_cal` past ~1 distorts the spectral envelope (it's the LSD optimum at 1.0). Don't
  chase corner via bias; it needs the active stages.
- VCO pitch cal `CV_FIT` is fit to **this solver** (not SPICE) — re-run `characterize_freq_cv` if
  the VCO solver changes. `dump_trajectory`, `measure_pitch_map`, `corner_vs_cutoff`,
  `brightness_sweep` are `#[ignore]` diagnostics.
- Spectral gate: `tests/ultra_spectral.rs` (binding) + `eval/303/coherence.py` (offline). The
  `ultra_esr.rs` −30 dB ESR test is `#[ignore]` informational — sample-exact vs ngspice is NOT the
  gate (structurally unreachable for a real-time chain).

## Commands

```bash
cargo test -p plugin303                                  # 57 passed, 5 ignored
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
