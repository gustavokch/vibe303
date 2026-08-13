# Accurate Topology / Ultra Tier Implementation Report

Date: 2026-08-11
Branch: `feat/accurate-fun-topology`
Scope: `plugin303/src/dsp/{voice,vco,ladder_wdf,bjt,diode,jfet,oversample,linalg_static}.rs`, `plugin303/src/params.rs`, `plugin303/src/lib.rs`

## 1. Two Independent Selectors

The plugin exposes two parameters that are easy to confuse. They are orthogonal.

| Parameter | Type | Values | Role |
|---|---|---|---|
| Quality Tier | `Tier` (`dsp/filter.rs:17`) | Low (TPT) / High (WDF-lite) / Ultra (WDF) | Selects the DSP fidelity path. |
| Circuit Topology | `CircuitTopology` (`params.rs:21`) | Accurate (default) / Fun | Gates which front-panel controls are honored and which VCO calibration applies. |

- **Accurate** restricts the instrument to stock TB-303 behavior: single oscillator, saw or square, fixed 50 % pulse width, 4-pole VCF, no VCF drive, classic envelopes, VCA in Envelope mode, pitch clamped to MIDI 24–72 (`voice.rs:479-498`, `voice.rs:594-600`). `OscWave::Sweep` is coerced to Saw.
- **Fun** unlocks the extended controls: oscillator 2, sub-oscillator, detune, Sweep waveform morph, variable pulse width, 2/3/4-pole taps, VCF drive, advanced ADSR envelopes, pitch range 0–108.

The Ultra tier is the only path that runs the full circuit solver for the oscillator. The remainder of this report describes the Ultra signal chain and how the Accurate/Fun topology switch interacts with it.

## 2. Ultra Tier Signal Chain

Per base-rate sample (`Voice::render_tier`, `voice.rs:803-862`):

```
Vco303 (nodal MNA VCO, base rate) → ×VCO_GAIN (2.607) → + 2-tap BLEP
    → mix with sub/osc2 → ×drive
    → Os4x upsample (4×, Kaiser polyphase FIR)
    → 4× LadderVcf tick (nodal MNA diode ladder at 192 kHz @ 48 kHz base)
    → Os4x decimate → ÷drive → CompDelay → output
```

Design points:

- The VCO runs at **base rate**, not 4×. Its only hard discontinuity is the JFET reset edge; that edge is corrected by a calibrated 2-tap first-order BLEP computed inside `Vco303::tick()`. The comment at `voice.rs:792-802` records the residual limitation: smooth-ramp alias is negligible; a full 4× VCO is a possible future `Tier::UltraHd`.
- The **filter** runs at 4× inside the oversampler, which band-limits the ladder nonlinearity.
- `VCO_GAIN = 2.607` (`voice.rs:24`) matches the solver's ~1.12 V p-p saw to the SPICE gold's 2.92 V p-p drive into the VCF.
- Per-tier brightness calibration: Ultra uses `ULTRA_BRIGHT = 0.70` on the cutoff→bias map, tuned to minimize spectral LSD (3.87 dB) against the SPICE gold (`voice.rs:26-34`).

## 3. The Nodal VCO (`dsp/vco.rs`)

`Vco303` is a per-sample **Modified Nodal Analysis + Newton-Raphson** solver of the TB-303 VCO core (netlist: `circuit/vco303.toml`):

- **Devices**: 6 BJTs (2N3904/2N3906, Gummel-Poon transport model with Early effect and high-injection, `dsp/bjt.rs`), one MPF104 JFET reset switch (Shichman-Hodges channel + gate junction diodes, `dsp/jfet.rs`), one 1N4148 diode (`dsp/diode.rs`), integrator cap C3 plus 4 more caps with trapezoidal companion models.
- **System size**: 21 nodes, 4 driven rails (`vp=12 V`, `vh=5.333 V`, `cvb=−4.7062+CV`, `cv=0`), leaving **17 unknowns** (`NU = 17`).
- **Operation**: relaxation oscillator — current ramps C3, the JFET discharges it when the comparator trips. Rails ramp 0→V over 1 ms at note-on for self-start (mirrors the SPICE harness PWL). A stuck-detector re-kicks node CP if it stops moving (`vco.rs:947-957`).
- **Newton loop** (`vco.rs:900-940`): warm-started from the previous sample, damped steps (`VLIM = 0.5 V`), mixed absolute/relative convergence (1e-9 + 1e-7·|v_max|), max 100 iterations. On stall it descends a **gmin homotopy** (1e-1 → 1e-2 → … → 0); if gmin=0 is never reached it holds the last physical operating point rather than emit a leakage-contaminated sample.
- **Linear solve**: `vco_lu_solve` in `dsp/linalg_static.rs` — a generated straight-line, natural-order, no-pivot LU specialized to the Jacobian's static fill pattern (~10× fewer ops than dense). A permanent test (`vco.rs:1271`) guards that it matches dense partial-pivot LU to ≪ Newton tolerance across pitches and the reset edge. Regeneration is via the ignored codegen test `emit_and_check_vco_lu`.
- **BLEP**: when node CP jumps up by more than `CP_RESET_DELTA = 3.0 V`, the fractional reset time α is computed and two taps are emitted: tap 0 into `blep_now` (added to the current output sample by the caller), tap 1 into `blep_pending` (auto-injected on the next tick). Step amplitude is calibrated to SPICE OR_SWING × VCO_GAIN = 2.92 V. The square wave is synthesized from the saw sign (±0.56 V) with its own BLEP corrections at both edges.
- **Output**: node `o_r`, DC-blocked by a ~7.6 Hz one-pole high-pass (rate-invariant coefficient).
- **Zero-alloc hot path**: all matrices are fixed-size stack arrays.

### Pitch calibration (F6.6 + Phase 4)

Pitch enters as the control voltage `cv` driving the `cvb` rail. `set_freq` does a log-frequency lookup with linear interpolation in a 37-anchor LUT spanning MIDI 0–108 in 3-semitone steps. The LUT content depends on topology — this is the "conditionalized topology calibration" of commit `4b57d7e`:

- **Fun** (`is_fun = true`): each anchor is solved by a **secant iteration** against the solver's own realized frequency (`secant_cv`, `vco.rs:397`) — render the VCO, measure zero crossings, refine CV to ≲0.1 %. Fun mode also boosts the JFET `beta` ×1.5 (`set_fun`, `vco.rs:672`), which changes the reset timing and therefore requires the measured calibration.
- **Accurate** (`is_fun = false`): anchors use the closed-form quadratic `cv_for_freq` in log₂(freq) (`CV_FIT`, `vco.rs:1110`), calibrated offline against this solver's measured `freq(cv)` — not SPICE's, because reset timing differs ~1.4×. Residual pitch error is ~1.4 % (worse at the low end) and is handled by the tier-alignment polynomial (§5).

The full store is **22 LUTs** (one per thermal-noise step 0–21) held in a double-buffered `VcoLutBuffer` published by `Arc` swap, so the audio thread reads a snapshot lock-free while a background thread rebuilds. Rebuilds are triggered by `recalculate_luts_async` when age/tolerance/topology change. A process-global `LUT_CACHE` keyed by (sample rate, thermal step, is_fun) avoids redundant secant builds; the fast path is a pure cache hit (~2 µs). A sequence counter drops stale background results. RT-safety: the audio-thread `reset()` clears the VCO in place and never touches the cache; LUT builds happen only from `initialize()` or background threads.

Analog character hooks: `set_analog_wear` applies seeded Gaussian component deviations (resistor/cap multipliers clamped ±10 %, BJT β degradation with age) and precomputes conductances `g_mults` so the per-sample stamp loop has no divisions (commit `4ee31b9`). `set_thermal` re-temperatures the device models and swaps the active LUT. Per-sample thermal noise is a small current injected at the integrator node CP.

## 4. The Nodal Diode-Ladder VCF (`dsp/ladder_wdf.rs`)

`LadderVcf` is the Ultra (and High) tier filter. The module name retains `wdf` for historical continuity; the method is **nodal Newton, not wave-digital**.

- **Topology** (from `eval/303/_vcf303_netlist_gen.py`): two rails (L/R), 4 rungs of 1N4148 diodes each (8 total), shunt caps bridging the rails per rung (33 nF ×3, 18 nF ×1). Cutoff is the **bias tail current** `I_tail` injected at the rail tops: each diode's small-signal resistance `r_d = nVt/I_d` makes the corner proportional to bias. Signal drives the bottom differentially; output is read across the rail tops.
- **System**: 12 nodes (10 rail nodes + 2 split input nodes `IN_L`/`IN_R`), dense LU via `lu_solve::<12>` with partial pivoting; a generated static LU (`ladder_lu_solve`) also exists and is guarded by `static_lu_matches_dense_over_operating_points`.
- **Nonlinearity**: only the 8 diode companions are iteration-dependent; everything else (caps, bias, pins, feedback) is iteration-invariant. The 8 diode stamps are evaluated branchlessly in one batch (`stamp8`, `diode.rs:101`) so LLVM vectorizes the exp evaluations 4-wide. Beyond `vcrit` the Shockley exp is frozen to its C1-continuous linear extension so an overshot Newton step cannot explode.
- **Cutoff map**: `I_tail = nVt · 2π · fc · C_ref · cal`. The `cal` brightness factor is the per-tier tuning knob (Ultra 0.70, High 0.50).
- **Resonance (F6.3)**: an active Q9 common-emitter feedback model. The output differential is fed back, inverted, into `IN_R` **in-loop** (zero delay): `v[IN_R] = kfb·(v[TL]−v[TR]) + cfb`, with `kfb = reso · RESO_GAIN (12.0)` and `cfb` a DC-block term from a one-pole high-pass tracker (~2 Hz corner, modeling the C23/C_out coupling caps). Because Q9 is modeled as linear class-A, this is a linear stamp — Newton still iterates only the diode nonlinearity. The loop regenerates the peak near fc (tracking the ngspice fixture to ~5 %) and self-limits above ~+14 dB instead of self-oscillating, matching the stock 303. The active feedback also lifts the audible corner past the passive ladder's structural ~600 Hz ceiling (test `corner_clears_600hz_ceiling`).
- **Unity passband**: `out_gain = 1/LADDER_DC_GAIN` (measured 0.0401) so Ultra/High match the Low tier's unity DC gain — without it they sit ~28 dB below Low.
- **Pole taps**: 2/3/4-pole outputs read the differential at rungs B/A/T respectively.
- **Convergence strategy**: same pattern as the VCO — warm-started Newton, mixed tolerance, gmin homotopy on stall, hold-last-good on failure.

### High tier

High (`voice.rs:774-791`) reuses the same `LadderVcf` at **2×** oversampling but keeps the cheap polyBLEP saw source, skipping the nodal VCO Newton. `HIGH_LEVEL_TRIM = 1.890` (+5.5 dB) equalizes loudness to Ultra so a tier switch is timbre-only. Cutoff/resonance for the Low and High paths are remapped through fitted 2-variable cubic polynomials (`LOW_FC_POLY`, `HIGH_FC_POLY`, etc., `voice.rs:42-102`) so all three tiers track the same knob positions.

## 5. Tier Switching, Latency, and Pitch Alignment

- **Crossfade**: `set_tier` starts a 10 ms equal-power crossfade (`XFADE_SECS`, `voice.rs:678-700`). A tier change requested mid-fade is queued in `pending_tier` and launched when the current fade completes, so a fast sweep never hard-cuts a still-weighted path.
- **Constant latency**: each tier's output passes through a `CompDelay` padding it to the maximum oversampler group delay (Ultra's 4×). Reported plugin latency never changes at runtime — no host graph re-init on tier switches, and crossfaded paths are time-aligned (`voice.rs:702-708`).
- **Pitch alignment** (commit `4b57d7e`): in Accurate topology, the polyBLEP oscillators (Low/High) get a cubic detune `dp(pitch)` (`voice.rs:896-906`) matching the Ultra VCO's measured pitch-tracking deviation from the `CV_FIT` quadratic, so crossfading between tiers does not produce a pitch jump. In Fun topology `dp = 0` because the secant-calibrated LUT is already exact. Note the VCO itself always receives the pitch without `dp` (`vco1_f`), while the polyBLEP oscillators receive pitch + `dp`.

## 6. Robustness and RT-Safety Summary

| Concern | Mechanism |
|---|---|
| Newton stall / singular matrix | gmin homotopy descent (1e-1 → 0); hold previous operating point on total failure |
| Exp overflow on overshot steps | `vcrit` C1-continuous linear extension (diode); damped Newton steps (`VLIM = 0.5 V`) |
| Oscillator latch-up at start | Rail ramp self-start + CP numerical kick + stuck detector |
| Reset-edge aliasing | 2-tap calibrated BLEP, second tap auto-injected next sample |
| Filter nonlinearity aliasing | 4× (Ultra) / 2× (High) polyphase Kaiser FIR oversampling |
| Allocations on audio thread | None: fixed-size stack matrices, generated static LU, `Arc`-snapshot LUTs |
| Heavy LUT builds off-thread | Background thread + sequence-counter staleness check + process-global cache |
| `reset()` from audio thread | In-place state clear; LUT cache untouched (documented at `voice.rs:620-631`) |
| Codegen solver correctness | Permanent tests: static LU vs dense LU across operating points (VCO and ladder) |

## 7. Known Limitations (recorded in code comments)

1. VCO runs at base rate; full 4× VCO (`Tier::UltraHd`) deferred (`voice.rs:798-802`).
2. Accurate-mode pitch relies on the quadratic `CV_FIT` (~1.4 % residual), compensated in the other tiers by the `dp` polynomial rather than corrected in the VCO itself.
3. The passive ladder's dominant pole sits below the set cutoff; audio-range corners are reached via the active Q9 resonance, not the passive corner (`ladder_wdf.rs:76-97`).
4. Absolute corner↔fc calibration to SPICE gold is open; per-tier `cal` brightness factors are spectral-LSD fits, not circuit-derived values.
