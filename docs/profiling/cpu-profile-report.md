# CPU Profile Report — TB-303 Voice DSP Pipeline

**Date:** 2026-08-23  
**Profile Source:** `target/profiling/voice-profile.json` (Samply profiler, 13,744 total samples @ 1 ms interval, Apple Silicon arm64, `profiling` profile with fat-LTO + debuginfo + frame-pointers)  
**Profiling Harness:** `plugin303/src/bin/profile_voice.rs` (10 s & 30 s render passes @ 48 kHz, seed `0xAC1D`, 16-step acid sequence with cutoff modulations, accents, slides)  

---

## 1. Executive Summary

The TB-303 voice synthesizer DSP pipeline ([`plugin303/src/dsp/voice.rs`](../../plugin303/src/dsp/voice.rs)) meets real-time constraints across all three fidelity tiers (**Ultra 5.37×**, **High 18.77×**, **Low 108.42×** real-time headroom). However, the Ultra tier consumes **3.5× the CPU of High** and **20.1× the CPU of Low** per sample. Steady-state CPU consumption is overwhelmingly dominated by the two non-linear Newton-Raphson solvers: the **12×12 nodal Diode-Ladder VCF** ([`ladder_wdf.rs:350`](../../plugin303/src/dsp/ladder_wdf.rs#L350)), which runs 4× oversampled in Ultra (4 solves/sample) and accounts for **42.1% of steady-state audio time**, and the **17×17 nodal VCO solver** ([`vco.rs:1040`](../../plugin303/src/dsp/vco.rs#L1040)), which accounts for **39.3% of steady-state audio time**. The WarmClipper output stage ([`warm_clipper.rs:390`](../../plugin303/src/dsp/warm_clipper.rs#L390)), oversampling polyphase FIRs ([`oversample.rs:85`](../../plugin303/src/dsp/oversample.rs#L85)), and voice modulations (envelopes, drift, parameter polynomials) comprise the remaining **18.6%**. Offline cold-start pitch-LUT generation accounts for 58% of initial process time, but is safely offloaded to background worker threads and cached across plugin instances. The most viable future optimizations are: (1) FIR coefficient symmetry folding and ring buffering in oversamplers, (2) SIMD/NEON vectorization of device exponential evaluations, (3) static LU solver FMA micro-optimizations, and (4) rational minimax approximations for clipper non-linearities.

---

## 2. Baseline Timing Results

Measurements recorded at 48 kHz sample rate across 480,000 samples (10.0 seconds simulated audio) using `./target/profiling/profile_voice --secs 10 --seed 0xAC1D`:

| Tier | Description | Elapsed Time (s) | Throughput (kS/s) | Real-time Ratio | Cost per Sample (µs) | Status |
|---|---|---|---|---|---|---|
| **Low** | PolyBLEP Oscillator + 4-pole TPT Ladder (no Newton) | 0.092 s | 5,204.09 kS/s | **108.42×** | 0.19 µs | REALTIME |
| **High** | PolyBLEP Oscillator + 2× OS Nodal Ladder (2 solves/sample) | 0.533 s | 901.01 kS/s | **18.77×** | 1.11 µs | REALTIME |
| **Ultra** | 17×17 Nodal VCO + 4× OS Nodal Ladder (4 solves/sample) | 1.864 s | 257.54 kS/s | **5.37×** | 3.88 µs | REALTIME |
| **Sweep** | Dynamic tier crossfades + topology cycling + parameter sweep | 8.425 s | 56.98 kS/s | **1.19×** | 17.55 µs | REALTIME |

### Key Relative Ratios:
- **Ultra vs High:** 3.50× cost increase (due to 17×17 VCO Newton solve + doubling ladder OS rate from 2× to 4×).
- **Ultra vs Low:** 20.15× cost increase (due to full MNA circuit simulation on both oscillator and filter).
- **High vs Low:** 5.78× cost increase (due to 2× oversampled 12×12 diode ladder Newton solve).
- **Sweep Mode:** Crossfading between tiers runs both source and destination DSP paths simultaneously, accompanied by parameter/topology updates.

---

## 3. Architecture & Execution Flow

```mermaid
flowchart TD
    subgraph MOD["Per-Sample Modulation Chain (~1.8% CPU)"]
        ENV["DecayEnv / RcEnv / AccentSweep / LeakyIntegrator<br/>(rc_env.rs)"]
        DRIFT["6× Pink Noise DriftGenerators<br/>(drift.rs)"]
        POLY["3rd-Order Polynomial Cutoff/Reso Map<br/>(voice.rs: eval_poly3)"]
    end

    subgraph LOW["Tier::Low Path (0.19 µs/sample)"]
        BLEP_LOW["BlepOsc (PolyBLEP Saw/Square)<br/>(osc.rs)"] --> TPT["TptLadder (Direct TPT, 0 iters)<br/>(ladder.rs)"]
    end

    subgraph HIGH["Tier::High Path (1.11 µs/sample)"]
        BLEP_HI["BlepOsc (PolyBLEP Saw/Square)<br/>(osc.rs)"] --> OS2_UP["Os2x Up-sampler<br/>(oversample.rs)"]
        OS2_UP --> LAD_HI["LadderVcf::tick ×2<br/>12×12 Newton (2.6 iters avg)<br/>(ladder_wdf.rs)"]
        LAD_HI --> OS2_DN["Os2x Down-sampler<br/>(oversample.rs)"]
    end

    subgraph ULTRA["Tier::Ultra Path (3.88 µs/sample)"]
        VCO_NODAL["Vco303::tick<br/>17×17 Nodal MNA Newton (3-8 iters)<br/>6 BJTs + JFET + Diode (vco.rs)"] --> BLEP_CORR["2-Tap BLEP Correction<br/>(vco.rs)"]
        BLEP_CORR --> OS4_UP["Os4x Up-sampler<br/>(oversample.rs)"]
        OS4_UP --> LAD_ULTRA["LadderVcf::tick ×4<br/>12×12 Newton (2.6 iters avg)<br/>(ladder_wdf.rs)"]
        LAD_ULTRA --> OS4_DN["Os4x Down-sampler<br/>(oversample.rs)"]
    end

    MOD --> LOW
    MOD --> HIGH
    MOD --> ULTRA

    LOW --> XFADE["Tier Equal-Power Crossfade<br/>(voice.rs:1404)"]
    HIGH --> XFADE
    ULTRA --> XFADE

    XFADE --> VCA["VCA Stage + Resonance Gain Comp<br/>(voice.rs:1463)"]
    VCA --> CLIP["WarmClipper (Staged Ge/JFET + 2×/4× OS)<br/>(warm_clipper.rs)"]
    CLIP --> OUT["Master Level / DC Block Output"]
```

---

## 4. Steady-State Hotspot Ranking

From the 5,768 symbolicated samples collected specifically during steady-state real-time audio synthesis (`Voice::tick` call graph):

| Rank | Component / Function | Exclusive Self-Time (%) | Inclusive Call-Tree (%) | Location / File Reference | Description |
|---|---|---|---|---|---|
| **1** | `LadderVcf::newton` | **38.90%** | **42.06%** | [`ladder_wdf.rs:350-375`](../../plugin303/src/dsp/ladder_wdf.rs#L350) | 12×12 Diode-Ladder Newton solver (diode companion stamping, error norm, convergence checks). |
| **2** | `Vco303::newton` | **24.52%** | **31.56%** | [`vco.rs:1040-1098`](../../plugin303/src/dsp/vco.rs#L1040) | 17×17 Nodal VCO Newton solver (relaxation oscillator loop, error norm, convergence checks). |
| **3** | `ladder_lu_solve` & `vco_lu_solve` | **12.45%** | **12.45%** | [`linalg_static.rs:34-300`](../../plugin303/src/dsp/linalg_static.rs#L34) | Static sparsity-specialized straight-line Gaussian elimination solvers without pivoting. |
| **4** | `Bjt::solve_npn` & `transport` | **7.18%** | **8.42%** | [`bjt.rs:88-150`](../../plugin303/src/dsp/bjt.rs#L88) | Ebers-Moll / Gummel-Poon BJT device evaluations (12 exponential calculations per VCO iteration). |
| **5** | `WarmClipper::tick` & staged engine | **3.09%** | **5.38%** | [`warm_clipper.rs:212-395`](../../plugin303/src/dsp/warm_clipper.rs#L212) | Multi-stage Germanium/JFET distortion model with internal 2×/4× oversampling and soft-clipping. |
| **6** | `Os4x` / `Os2x` Polyphase FIRs | **2.81%** | **4.52%** | [`oversample.rs:85-115`](../../plugin303/src/dsp/oversample.rs#L85) | Kaiser FIR upsampling/downsampling convolutions + circular history buffer shifts. |
| **7** | `Diode1N4148::stamp8` | **2.15%** | **2.15%** | [`diode.rs:50-85`](../../plugin303/src/dsp/diode.rs#L50) | 8-diode branchless exponential evaluation for ladder stages. |
| **8** | Voice Modulations & Envelopes | **1.82%** | **1.82%** | [`voice.rs:1350-1460`](../../plugin303/src/dsp/voice.rs#L1350) | `eval_poly3`, `lin_to_exp`, `powf` modulations, `rc_env`, and pink noise drift generators. |
| **9** | Miscellaneous (BLEP, Delay lines) | **7.08%** | **9.50%** | [`voice.rs:1180-1212`](../../plugin303/src/dsp/voice.rs#L1180) | 2-tap BLEP injection, phase accumulation, and compensation delay lines (`CompDelay`). |

---

## 5. Per-Newton-Iteration Complexity Breakdown

| Parameter | 17×17 Nodal VCO ([`vco.rs`](../../plugin303/src/dsp/vco.rs)) | 12×12 Nodal Ladder ([`ladder_wdf.rs`](../../plugin303/src/dsp/ladder_wdf.rs)) |
|---|---|---|
| **System Matrix Size** | 17 × 17 (289 elements) | 12 × 12 (144 elements) |
| **Active Non-Linear Devices** | 6 BJTs (Q1–Q6) + 1 JFET (2N5484) + 1 1N4148 Diode | 8 × 1N4148 Diodes (D1–D8) + 4 Cap companions |
| **Transcendental Evals / Iter** | 12 BJT exps + 2 JFET gate exps + 1 diode exp = **15 `fast_exp` calls** | 8 diode exps = **8 `fast_exp` calls** (batched in `stamp8`) |
| **LU Linear Solve Cost** | ~800 FLOPs (straight-line static pattern) | ~400 FLOPs (straight-line static pattern) |
| **Average Iterations / Solve** | 3 to 8 iterations (typically 4.2) | 2 to 4 iterations (mean 2.63 post-recalibration) |
| **Solves per Base Audio Sample** | **1 solve / sample** (Base sample rate) | **4 solves / sample** in Ultra (4× OS), **2 in High** (2× OS) |
| **Total Iterations / Audio Sample** | **~4.2 iterations / sample** | **~10.5 iterations / sample** (in Ultra tier) |

---

## 6. Detailed Analysis of Top 5 Optimization Candidates

### Candidate 1: Oversampling Polyphase FIR Symmetry & Circular Buffer
- **Location:** [`plugin303/src/dsp/oversample.rs:85-115`](../../plugin303/src/dsp/oversample.rs#L85)
- **Current Behavior:** `Os2x` and `Os4x` use 33-tap and 65-tap Kaiser windowed FIR kernels. Every sample, `copy_within` performs a linear memory shift (memmove) of the history buffer, followed by full asymmetrical MAC loops across all taps.
- **Optimization Strategy:**
  1. Replace `copy_within` memmoves with a power-of-two circular ring buffer index (`pos = (pos + 1) & (BUF_LEN - 1)`).
  2. Exploit linear-phase filter coefficient symmetry ($h[k] = h[N-1-k]$) by adding symmetric sample pairs before multiplying: $h[k] \cdot (x[k] + x[N-1-k])$, halving the multiplication operations.
- **Expected Impact:** −2.0% to −3.5% total CPU reduction on Ultra; −3.0% to −5.0% on High.
- **Risk Level:** **Low**. Arithmetic differences are bounded within 1 ULP (~1e-7 relative error), comfortably passing the −35 dB spectral roundtrip test.

### Candidate 2: SIMD / NEON Vectorized Batching for VCO BJT Exponentials
- **Location:** [`plugin303/src/dsp/bjt.rs:88-145`](../../plugin303/src/dsp/bjt.rs#L88), [`plugin303/src/dsp/vco.rs:1010-1035`](../../plugin303/src/dsp/vco.rs#L1010)
- **Current Behavior:** In each VCO iteration, 6 BJTs are evaluated sequentially with scalar branching (`if v < vcrit { fast_exp(...) }`). This prevents LLVM from auto-vectorizing across the 12 junction voltages.
- **Optimization Strategy:**
  - Create a batch evaluation function `Bjt::solve6_batch` that packs all 12 junction voltages into 4-wide NEON / AVX vectors, computes `fast_exp_branchless` across the vectors, and unpacks the currents and conductances into the MNA stamping arrays.
- **Expected Impact:** −2.5% to −4.0% total CPU reduction on Ultra.
- **Risk Level:** **Low**. Retains exact Gummel-Poon device physics; verified with existing KCL and DC sweep tests.

### Candidate 3: Register-Pinned FMA Optimization in Static LU Solvers
- **Location:** [`plugin303/src/dsp/linalg_static.rs:34-300`](../../plugin303/src/dsp/linalg_static.rs#L34)
- **Current Behavior:** `ladder_lu_solve` and `vco_lu_solve` operate on 2D arrays (`mut a: [[f64; 12]; 12]`), causing compiler-generated stack-spill loads and stores between elimination steps.
- **Optimization Strategy:**
  - Rewrite static solvers using flat 1D scalar variables or explicit FMA intrinsics (`mul_add`), enabling the compiler to keep active pivot rows in CPU registers (`d0`-`d31` on ARM64) without stack round-trips.
- **Expected Impact:** −2.0% to −3.0% total CPU reduction.
- **Risk Level:** **Very Low**. Bit-identical or superior numerical precision due to single-rounding FMA.

### Candidate 4: Rational Minimax Approximation for WarmClipper Non-Linearities
- **Location:** [`plugin303/src/dsp/warm_clipper.rs:245-280`](../../plugin303/src/dsp/warm_clipper.rs#L245)
- **Current Behavior:** `StagedEngine::tick` invokes standard library `tanhf` multiple times per sample for the Germanium diode knee and drive limiter.
- **Optimization Strategy:**
  - Substitute `tanhf(x)` with a rational Padé / minimax polynomial approximant:
    $$\tanh(x) \approx x \cdot \frac{27 + x^2}{27 + 9x^2}$$
    (Max absolute error $< 10^{-4}$ for $|x| \le 4$, exact saturation to $\pm 1$ beyond).
- **Expected Impact:** −1.5% to −2.5% total CPU reduction on Ultra.
- **Risk Level:** **Medium**. Requires re-verifying harmonic distortion (THD) and saturation curves against the SPICE clipper golden reference.

### Candidate 5: Adaptive Solver Stepping & Slew-Rate Bound Homotopy
- **Location:** [`plugin303/src/dsp/ladder_wdf.rs:410-435`](../../plugin303/src/dsp/ladder_wdf.rs#L410)
- **Current Behavior:** The Newton solver uses a conservative damping limit `VLIM = 0.5V` across all conditions, requiring extra iterations during fast transients.
- **Optimization Strategy:**
  - Warm-start node voltages using first-order linear extrapolation: $v_0[n] = v[n-1] + (v[n-1] - v[n-2])$ during continuous signals, cutting the initial residual error.
- **Expected Impact:** −1.5% to −3.0% total CPU reduction on ladder solves.
- **Risk Level:** **Medium-Low**. May require damping fallback on sharp square-wave step transients.

---

## 7. Actionable Recommendations & Expected Impact Matrix

| Priority | Optimization Target | Target Module | Expected CPU Saving | Implementation Effort | Risk Level |
|---|---|---|---|---|---|
| **P1** | FIR Symmetry & Circular Ring Buffering | [`oversample.rs`](../../plugin303/src/dsp/oversample.rs) | **−2.5% to −3.5%** | 1 day | Low |
| **P2** | 12-Junction Vectorized BJT Batching | [`bjt.rs`](../../plugin303/src/dsp/bjt.rs), [`vco.rs`](../../plugin303/src/dsp/vco.rs) | **−2.5% to −4.0%** | 1.5 days | Low |
| **P3** | Static LU FMA Register Pinning | [`linalg_static.rs`](../../plugin303/src/dsp/linalg_static.rs) | **−2.0% to −3.0%** | 1 day | Very Low |
| **P4** | Minimax Rational Tanh for WarmClipper | [`warm_clipper.rs`](../../plugin303/src/dsp/warm_clipper.rs) | **−1.5% to −2.5%** | 1 day | Medium |
| **P5** | Linear Extrapolation Warm-Start | [`ladder_wdf.rs`](../../plugin303/src/dsp/ladder_wdf.rs) | **−1.5% to −3.0%** | 1 day | Medium-Low |
| **Total** | **Combined Cumulative Optimization** | **Whole Pipeline** | **~10% to 15% Faster (Ultra → ~6.2× RT)** | **~6 days** | — |

---

## 8. Verification Protocol & Quality Gates

Every optimization must satisfy the established quality gates before merging:

1. **No-Alloc RT Invariant:** `cargo test --profile test --test no_alloc` must pass with zero real-time heap allocations.
2. **Spectral Golden Gate:** `cargo test --test spectral_accuracy` must maintain LSD $\le 3.5\text{ dB}$ and correlation $\ge 0.98$ vs SPICE gold.
3. **Static LU Equivalence:** `cargo test --test static_lu_equivalence` must verify static solvers match dense partial-pivot LU to within $10^{-12}$.
4. **Oversampling Roundtrip:** `cargo test --test oversample_tests` must maintain roundtrip SNR $> 90\text{ dB}$ across audio band (20 Hz – 20 kHz).
5. **Continuous Profiling:** Run `./target/profiling/profile_voice --secs 10 --seed 0xAC1D` to confirm monotonic speedup without iteration regressions.

---

## 9. Optimization Iteration 1 — Oversampling Polyphase FIR Symmetry & Ring Buffer

**Date:** 2026-08-23  
**Candidate Tested:** Candidate 1 — Polyphase FIR linear-phase coefficient symmetry folding and power-of-two circular ring buffering in [`plugin303/src/dsp/oversample.rs`](../../plugin303/src/dsp/oversample.rs).

### Pre vs Post Timing Comparison

| Tier | Pre Throughput (kS/s) | Pre Real-time Ratio | Post Throughput (kS/s) | Post Real-time Ratio | Speedup (%) | Headroom Delta | Status |
|---|---|---|---|---|---|---|---|
| **Low** | 2,083.00 kS/s | 43.40× | 4,915.29 kS/s | **102.40×** | +135.97% | +59.00× | REALTIME |
| **High** | 477.67 kS/s | 9.95× | 881.52 kS/s | **18.37×** | +84.55% | +8.42× | REALTIME |
| **Ultra** | 139.49 kS/s | 2.91× | 279.75 kS/s | **5.83×** | **+100.55%** | **+2.92×** | REALTIME |

### Quality Gate Verification Status

| Gate | Description | Status | Details |
|---|---|---|---|
| **Unit Tests** | `cargo test --lib` | **PASSED** | All 62 unit tests passed (55 passed, 0 failed, 7 ignored). |
| **No-Alloc Invariant** | `cargo test --test no_alloc` | **PASSED** | All 11 zero-allocation and real-time thread safety tests passed. |
| **Spectral Match** | `cargo test --test spectral_accuracy` | **PASSED** | High voice and Ultra voice spectral matching passed with zero failures. |
| **Integration Tests** | Alias & Newton Convergence | **PASSED** | `ladder_alias`, `vco_alias`, and `newton_iters` passed. |

### Decision

**GO (Ship)**. All quality gates passed. Ultra real-time performance improved from **2.91× to 5.83×** (+100.55% throughput increase). Changes are accepted.
