# DSP optimization knobs before a polyphonic design

**Goal:** list the remaining per-voice CPU knobs, with measured evidence, so the polyphony decision
(voice count per tier, voice data layout) is made against the optimized cost rather than today's.

**Status:** groups A and B implemented on `perf/pre-poly-knobs` (results in §6); group C on
`perf/tier-2x` (results in §7). Group D is open.

**Supersedes:** the open candidates in `docs/profiling/cpu-profile-report.md` §6–7 and
`2026-08-20-dsp-nodal-solver-optimization-plan.md`. Rejected ideas from those rounds are listed at the
end so they are not proposed again.

---

## 1. Baseline (2026-11-16, Apple Silicon, `main`)

`profile_voice --secs 20 --tier <t>` (acid pattern, MIDI 60–72 after the Accurate clamp, release
build). Wall clock, machine load average 13–15, so treat the percentages as ±15%.

| Tier | Realtime factor | % of one core per voice |
|---|---|---|
| Draft | 68.95× | 1.45% |
| Low | 12.43× | 8.0% |
| High | 8.32× | 12.0% |
| Best | 6.31× | 15.8% |

Component split from `sample` on the `profiling-thin` build with frame pointers, steady state only
(self time; libm calls attributed to their callers):

| Tier | Nodal ladder | Circuit VCO | Oversampling | `Voice::tick` overhead + libm | Clipper | TPT ladder |
|---|---|---|---|---|---|---|
| Draft | n/a | n/a | 32% | 32% | 18% | 16% |
| Low | 83% | n/a | 7% | 6% | 3% | n/a |
| High | 29% | 63% | 1% | 4% | 2% | n/a |
| Best | 44% | 50% | 3% | 3% | 1% | n/a |

### Newton iteration counts (`tests/newton_iters.rs`, A1, cutoff 1.2 kHz, reso 0.7)

- Ladder: 2.63 iterations per solve (37% need 2, 62% need 3), no fallbacks. That is 10.5 LU solves
  per output sample in Best.
- VCO: 2.64 iterations per sample on average. Every JFET reset (once per oscillator cycle) fails the
  fast path, runs `MAX_ITERS = 100` iterations, then falls back to the gmin homotopy (82 more
  iterations). The sample after a reset takes 38 iterations on the fast path.

### VCO cost depends on pitch (throwaway probe, standalone `Vco303`)

| Note | % of one core | Normal iterations per sample | Cost of one reset sample | Reset share of VCO time |
|---|---|---|---|---|
| 55 Hz | 4.5% | 2.35 | 57 µs | 7% |
| 110 Hz | 4.9% | 2.46 | 57 µs | 13% |
| 220 Hz | 5.9% | 2.68 | 57 µs | 22% |
| 440 Hz | 8.5% | 3.62 | 57 µs | 30% |
| 880 Hz | 11.9% | 4.18 | 57 µs | 43% |

A polysynth plays higher than an acid bassline, so the VCO knobs (K1, K4, K5) matter more for poly than
the A2 numbers suggest. One reset sample costs about 2.7 sample periods at 48 kHz.

### Startup

`Voice::new` costs 1.07e11 instructions (about 7 s) per process per sample rate, before any audio. It
builds the nominal VCO pitch LUT with the dense solver: `lu_solve::<17>` takes 50% of startup samples,
`memmove` 16%, `Vco303::newton` 18%, BJT evaluation 12%.

---

## 2. Knobs

"Measured" means a probe was run in this round. Everything else is an estimate from the profile
shares above.

### A. Exact or near-exact (no recalibration expected)

| ID | Knob | Evidence | Expected saving | Gates |
|---|---|---|---|---|
| K1a | Separate fast-path iteration cap for `Vco303::newton`, for example 40 instead of `MAX_ITERS = 100` before the homotopy | **Probe** (`MAX_ITERS = 40`): reset sample 57 µs → 39–41 µs. Homotopy still converges (82 iterations). VCO cost −9% at 440 Hz, −14% at 880 Hz. `vco_pitch`, `vco_alias`, `best_spectral` pass (LSD 3.15 / 3.52 unchanged). Probe checksums unchanged at ≤ 440 Hz (1-decimal print); the 880 Hz one moved (−758.5 → −752.5), so the dedicated cap must keep the homotopy at 100. | Best/High −4% to −8% at mid to high pitch | `vco_pitch`, `best_spectral`, `vco_alias` |
| K5 | Remove the VCO's per-iteration matrix copies: `let mut a = *base_a` (2.3 KB), then `vco_lu_solve(a, rhs)` by value with `a` kept alive for the dense retry | `_platform_memmove` = 7.6% of Best and 9.6% of High, all called from `Vco303::newton`. Factor in place and send non-finite results straight to the existing homotopy (drop the dense retry on the hot path). | Best −5%, High −7% | static-vs-dense LU tests, `best_spectral` |
| K6 | Skip the zero taps of the half-band oversampler | `build_lp(8.0, 0.25)` is an exact half-band: 16 of the 17 `he` taps and 16 of the 32 `down` pair taps are ≤ 2e-17. `Os2x::up` and `Os2x::down` multiply by them anyway. | Oversampling MACs −50%: Draft −14%, Low −3%, Best −1.5% | `oversample` unit tests, `ladder_alias` |
| K8 | Do only the active tier's per-sample work in `Voice::tick` and hoist constants | Each sample computes `map_draft_vcf` + `map_low_vcf`, `TptLadder::set_cutoff` (a `tanf` prewarp), and `set_freq` on every tier's oscillators, plus constant `exp()` calls (accent decay coefficient, self-heat alpha) and `10^(dB/20)` for the master gain. Restrict to the active and fading tiers; compute constants at parameter rate. | Draft up to −15%, Low −3%, High/Best −2% | `tier_switch`, `tier_fade_isolation`, `no_alloc` |
| K13 | Faster VCO LUT builds: static solver with dense fallback in `cv_lut_for` / `solve_luts`, or precomputed nominal LUTs for 44.1/48/88.2/96 kHz guarded by a drift test | Dense LU + `memmove` = 66% of startup. Thermal steps (22 per sample rate) and per-instance wear LUTs pay the same cost on the single LUT worker thread. | Startup roughly 2× faster (static solver); instant with precomputed tables | `vco_pitch`, `recalibration_scale` |

### B. Solver tolerance and prediction (output changes at the tolerance level)

| ID | Knob | Evidence | Expected saving | Gates |
|---|---|---|---|---|
| K2 | Loosen the ladder Newton tolerance 100× (`TOL` 1e-7, `TOL_ABS` 1e-8, `TOL_REL` 1e-6). The last iteration usually only confirms convergence: Newton converges quadratically, so the iterate before a step of size δ is already accurate to about δ. | **Probe:** iterations 2.63 → 2.01 per solve (99.25% converge in 2). Instructions retired: Low −18%, Best −7%. Cycles: Low −21%, Best −6%. `best_spectral` (3.15 / 3.52 dB, corr 0.988 / 0.989), `low_spectral` (12.48 / 12.46 dB), `vcf_resonance`, `tier_levels` and `ladder_alias` all unchanged. | Low −20%, High −7%, Best −6% | Same set; also add a noise-floor check at reso 0 |
| K3 | The same treatment for the VCO tolerance (`TOL` 1e-7, `TOL_REL` 1e-7) | Not probed. LUTs are built by the same solver, so pitch stays self-consistent; the `CV_FIT` seed may need refitting (gotcha 2). | VCO −10% to −20% | `vco_pitch`, `best_spectral` |
| K4 | Linear-extrapolation warm start, `v0 = 2·v[n−1] − v[n−2]`, disabled on the sample after a reset | VCO normal iterations rise from 2.35 (55 Hz) to 4.18 (880 Hz) because the ramp moves further per sample. The ramp is close to linear, which is the case extrapolation handles. Ladder gain is uncertain once K2 is in. Listed as P5 in the previous round but never tried. | VCO −25% at 440 Hz, −35% at 880 Hz (estimate) | `vco_pitch`, `best_spectral`, `newton_iters` |
| K1b | Make the JFET reset converge on the fast path: SPICE-style junction limiting (`pnjlim`/`fetlim` on junction voltages) instead of the per-node `VLIM = 0.5 V` clamp, or a reset-aware predictor | After K1a, resets are still 5% (55 Hz) to 33% (880 Hz) of VCO time. The reset swing is > 3 V (`CP_RESET_DELTA`) against a 0.5 V per-iteration clamp. Changes the reset trajectory, so the BLEP alignment and pitch LUT must be rechecked. | VCO up to −30% at 880 Hz | `vco_pitch`, `vco_alias`, `best_spectral`; refit `CV_FIT` |
| K10 | Fast rational `tanh` in `WarmClipper` | `tanhf` from the clipper is 7% of Draft. Previously planned (P4) but not tried. | Draft −7% | `warm_clipper` tests |

### C. Tier redesign (needs a product decision and recalibration)

| ID | Knob | Evidence | Expected saving | Cost |
|---|---|---|---|---|
| K11 | Low tier at 2× instead of 4× oversampling | Low is 83% nodal ladder. High already ships the same ladder at 2×. | Low −40% | Refit `LOW_FC_POLY`, `LOW_RESO_POLY`, `LOW_LEVEL_TRIM`; `low_spectral`; PDC unchanged (padded to Best) |
| K12 | Draft tier at 2× | Oversampling + TPT = 48% of Draft. | Draft −25% | Refit Draft polynomials and trims; `voice_esr` |
| K9 | Fast `tanh` in the TPT ladder's per-stage saturation | 16 `tanhf` per sample at 4× = 13% of Draft. Draft's gate is a regression guard, not a fidelity target. | Draft −10% | `voice_esr`, `vcf303_fit` |
| K7 | Shorter inner `Os4x` stage | The 2×↔4× stage has a transition band from 0.5·fs to 1.5·fs (base rate); 15–23 taps are enough, not 65. Also cuts reported latency from 48 to about 36 base samples. | Oversampling −30% after K6 | `ladder_alias`, `tier_alignment`, PDC tests |

### D. Decide before committing to the poly layout

| ID | Knob | Why it matters |
|---|---|---|
| P1 | Idle-voice sleep | Cost should scale with sounding voices, not allocated ones. The VCO free-runs, so a sleeping voice needs a saved oscillator state to resume from instead of the 1 ms rail ramp self-start. Needs an explicit sleep test: VCA below −120 dB and ladder settled. |
| P2 | Bus-rate decimation and one output clipper | Decimation is linear: summing the voices' 4× ladder outputs per oversampling group and decimating once is exact up to rounding, if the VCA gain is applied at the high rate. Removes N−1 down filters and N−1 clippers (18% of Draft per voice). |
| P3 | Lane-parallel voice bank (structure of arrays) | The static no-pivot LU is branch-free straight-line code, so 2 voices per NEON f64x2 or 4 per AVX2 f64x4 map one-to-one, and the ladder's tight 2–3 iteration spread wastes few lanes. The rejected BJT batch lost 33% to gather/scatter inside one voice; a lane layout has none. This decides `[Voice; N]` versus `VoiceBank<LANES>`, so prototype a 2-lane ladder microbenchmark before writing the allocator. |
| P4 | Wear LUTs per instance, not per voice | Per-voice wear seeds multiply LUT builds by N × 2 VCOs on one worker thread (K13 cost). One seed per instance keeps load time flat. |

---

## 3. Already tried; do not repeat without a new angle

| Idea | Result | Reference |
|---|---|---|
| Batch the 6 VCO BJTs into one SIMD evaluation (`solve6_batch`) | −33% Best on aarch64; reverted | `cpu-profile-report.md` §10, `e56dd45` |
| `mul_add` FMA in the static LU solvers | −1.5% Best; reverted | `cpu-profile-report.md` §11 |
| Cache the ladder's iteration-invariant matrix skeleton | Regressed on aarch64; reverted | `ffc2d46`, reverted in `e56dd45` |
| f32 Newton | Not tried. Expected to fail: the system mixes `G_PIN = 1e3` with `GMIN_BASE = 1e-12` conductances | [INFERENCE] |

---

## 4. Measurement protocol for this round

- Wall-clock `profile_voice` drifted ±20% between identical runs under load. Compare **instructions
  retired** and **cycles** from `/usr/bin/time -l`, minus a `--secs 0.01` run of the same binary to
  remove the 1.07e11-instruction startup.
- `profile_voice` only covers MIDI 60–72. Add low (A1) and high (A5) cases before judging any VCO knob,
  because VCO cost more than doubles across that range.
- Run every knob on its own against `best_spectral`, `low_spectral`, `vcf_resonance`, `tier_levels`,
  `vco_pitch`, `ladder_alias`, `no_alloc`. Record before and after LSD to two decimals.

## 5. Suggested order

1. **A:** K1a, K5, K6, K8, K13. Cheap, exact or near-exact; benefits mono now.
2. **B:** K2 first (already probed), then K4 + K3 together, then K1b.
3. **P3 prototype:** run it before writing the poly allocator; it decides the voice layout.
4. **C:** only if the poly voice counts after A and B are still short; each item needs a calibration
   pass.

Rough projection after A and B, mid pitch, percent of one core per voice (estimate, not measured):
Best 15.8% → about 10.5%, High 12% → about 7.5%, Low 8% → about 6% (about 3.5% with K11),
Draft 1.45% → about 0.9%.

## 6. Results: groups A and B (2026-11-16)

Implemented: K1a, K1b, K2, K3, K4 (VCO only), K5, K6, K8, K13. Every test binary passes
(`cargo test --release -p plugin303`, 49 binaries, same as `main`).

### Cost per voice

`profile_voice --secs 20 --tier <t>`, Apple Silicon, load average about 3. Instructions and cycles
are the mean of two interleaved runs per binary, minus a `--secs 0.01` run of the same binary.

| Tier | Instructions | Cycles | Realtime factor | % of one core per voice |
|---|---|---|---|---|
| Draft | 5.23e9 → 3.95e9 (−24%) | −24% | 69.2× → 87.3× | 1.45% → 1.15% |
| Low | 1.73e10 → 1.29e10 (−25%) | −27% | 12.3× → 16.9× | 8.1% → 5.9% |
| High | 3.07e10 → 1.95e10 (−37%) | −36% | 8.4× → 13.0× | 11.9% → 7.7% |
| Best | 3.82e10 → 2.44e10 (−36%) | −37% | 6.4× → 9.9× | 15.7% → 10.1% |

Startup (VCO pitch LUT build, one sample rate): 1.07e11 → 2.33e10 instructions (4.6× faster). The
full release test suite runs in 278 s instead of 1058 s.

Standalone `Vco303` (no ladder), Newton iterations per sample after all changes; the reset sample
no longer reaches the gmin homotopy at any pitch:

| Note | Iterations before | Iterations after | Homotopy fallbacks before | After |
|---|---|---|---|---|
| A1 (55 Hz) | 2.35 + 1 homotopy per cycle | 1.25 | 1 per cycle | 0 |
| A3 (220 Hz) | 2.68 + 1 homotopy per cycle | 1.90 | 1 per cycle | 0 |
| A5 (880 Hz) | 4.18 + 1 homotopy per cycle | 3.23 | 1 per cycle | 0 |

Ladder (Best chain, A1, cutoff 1.2 kHz): 2.63 → 2.01 solves per sample at reso 0 and 0.7; 3.00 at
A4, cutoff 3 kHz, reso 0.95.

### Quality gates

Unchanged at printed precision: `best_spectral` LSD 3.15 / 3.52 dB (corr 0.988 / 0.989),
`low_spectral` 12.48 / 12.46 dB (corr 0.978 / 0.982), `vco_pitch` errors 0.003–0.085 %. Draft and Low
renders are bit-identical to `main` before K2; High and Best differ by −125 to −159 dB before K3
(the LUT now comes from the real-time solver).

### What each knob turned out to be

- **K1b** found a cheaper fix than junction limiting. Newton from the previous cycle's reset state
  converges to the homotopy's root in 2 iterations (max difference ≤ 2e-11 V, 110 Hz to 3.5 kHz).
  The post-reset branch also exists on ordinary ramp samples, so this start is tried only after the
  ramp start fails, within two ramp steps of the last trip level, and kept only if it is a reset. The
  sample after a reset starts from the previous cycle's post-reset state: 2–3 iterations instead
  of 38.
- **K1a** is `FAST_MAX_ITERS = 40` per warm-start attempt: ramp samples need ≤ 29 up to 3.5 kHz and the
  cold post-reset sample 38. A reset now costs 42 iterations instead of about 180.
- **K4** helps only after a smooth sample. Plain extrapolation cut iterations 8–20 % at 110–880 Hz but
  added 9–39 % at 1.7–3.5 kHz (it overshoots the post-reset recovery). Extrapolating only after a
  sample that converged in ≤ 3 iterations cuts 8–26 % at every tested pitch. Not applied to the
  ladder: it already converges in 2 solves after K2, the minimum for a step-size test.
- **K3** (VCO tolerances 100× looser) changes the pitch LUT within the secant's 0.1 % target, so
  renders are no longer sample-comparable to `main`; the pitch and spectral gates are unchanged.
- **K5** changed the generated `vco_lu_solve` to factor in place (`&mut`). The codegen in
  `emit_and_check_vco_lu` emits the new signature.
- **K13** removed the dense LU from LUT builds (and the `dense_solver` flag). The dense solve stays as
  the fallback for a non-finite static step.

## 7. Results: group C (2026-11-17)

Implemented: K11 (Low at 2×), K12 (Draft at 2×), K9 (rational `tanh` in the TPT ladder), K7
(29-tap inner `Os4x` stage). Every test binary passes (`cargo test --release -p plugin303`, 49
binaries, 275 tests).

### Cost per voice

Same protocol as §6, against the §6 build (`d9f8202`):

| Tier | Instructions | Cycles | % of one core per voice |
|---|---|---|---|
| Draft | 3.95e9 → 2.73e9 (−31%) | −30% | 1.15% → about 0.8% |
| Low | 1.29e10 → 8.12e9 (−37%) | −40% | 5.9% → about 3.6% |
| High | 1.95e10 → 1.95e10 (+0.2%) | −0.3% | 7.7% (unchanged) |
| Best | 2.44e10 → 2.40e10 (−1.4%) | −1.7% | 10.1% → about 9.9% |

### Calibration

- **Curves kept.** `calibrate_vcf.py` re-fitted at 2× matches each tier's filter to Best's (Low:
  0.4–1.5 dB response error instead of 6–9 dB with the July curves, which are close to identity).
  The voice did not follow. Mean spectral distance to Best's voice over 27 patches (notes 33/45/57 ×
  cutoff 400/1000/2500 Hz × reso 0.3/0.65/0.85): Low 16.60 (4×) → 16.18 (2×, kept curves) vs 20.97
  (re-fitted at brightness 0.45) and 15.33–17.15 (re-fitted, brightness 0.20–0.35); Draft 23.15
  (4×) → 23.32 (2×, kept) vs 23.94 (re-fitted). Low's darker brightness (0.45 vs Best's 0.70)
  compensates its polyBLEP saw, which is brighter than the circuit saw; a filter-only fit undoes
  that. A fit against Best's rendered voice would be needed to do better.
- **Level trims** re-derived at 2× (`tier_levels::{draft,low}_level_trim_report`); medians 0.00 dB
  at every knot. Low's spread is the same at 2× as at 4×.
- **Gates:** `low_spectral` 12.48 → 15.36 dB (budget 16), `low_plugin_path` 12.46 → 13.73, Low vs
  Best parity 11.70 → 11.99 dB (corr 0.995), Draft vs Best 12.36 → 13.24 dB (corr 0.982),
  `voice_esr` 3.39 → 2.91 dB (budget 3.5), Draft headroom +0.47 / +2.07 dB (budget 3.5).
- **The SPICE gold is latency-sensitive.** It is 0.2 s long, so a time shift of the render moves
  its LSD by several dB. K7 cuts Best's latency from 48 to 39 samples and the clipper's by the same:
  Best's audio is unchanged (−76 dB difference after an 18-sample alignment) but `best_spectral`
  reads 2.24 dB instead of 3.15, and Low (already at 2×; K7 does not touch its path) 15.35 instead
  of 12.25. Low's gate margin (0.64 dB) is mostly this shift.

### What each knob turned out to be

- **K7:** an L = 29, Kaiser β = 9 half-band reaches −89 dB above 1.42·fs (the 65-tap stage: −91
  dB) with < 0.001 dB passband ripple. `Os4x` and `CLIPPER_LATENCY` are 39 samples instead of 48;
  the plugin reports 78 instead of 96.
- **K9:** a [7/6] Padé `tanh` clamped at 4.97; absolute error < 1e-4, f32-exact below |x| = 1.
- **K11/K12:** `LOW_OS_FACTOR` and `DRAFT_OS_FACTOR` are 2; `vcf_sweep_helper` builds each tier's
  oversampler from its factor.
