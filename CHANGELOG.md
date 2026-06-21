# Changelog

All notable changes to BA283 are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - Unreleased

First public release of the BA283 — a neural and circuit emulation of the Neve
BA283 mic-preamp output stage.

### Added
- **Two runtime-selectable inference backends**, both validated against ngspice
  renders of the real circuit:
  - **Neural Net** (active engine): pure-Rust streaming **FiLM WaveNet** (23
    dilated layers, 3 channels, 2-channel `[audio, gain]` condition,
    gain-conditioned, RF 6347), exported from stock `neural-amp-modeler` 0.13
    (conditioned by config, not a fork) as `model.cond.nam`, embedded at build
    time. Streaming/causal → **zero added
    latency**. (The legacy `LSTM(3 × 18)` + FiLM engine remains in-tree but
    dormant; it is what the in-repo `train/` pipeline produces.) This engine is
    gain-conditioned via the **Circuit Gain** knob, not **Bias** — under the
    Neural Net backend the Bias control is inert and is greyed out in the editor
    (it stays active for the Circuit/WDF backend). Weights are provisional pending
    final training; the rendered tone may change before release.
  - **Circuit (WDF)**: nodal MNA + Newton-Raphson solver with Gummel-Poon BJT
    models and coupled inductors. Four quality tiers — `Realtime` (bit-exact
    reference), `Fast` (~5×), `UltraFast` (~9×), `Warp` (~12×) — plus orthogonal
    **Precision** (High/Medium/Low) and **Oversampling** (Auto/1×/2×) dials.
- **Adaptive oversampling** (`adaptive-os` feature, compiled in by default) via a
  new **Adaptive OS** toggle (`adaptive_os`, default **off**): on the os=1 lossy
  tiers (`UltraFast`/`Warp`) under **Oversampling = Auto**, a per-channel envelope
  detector engages a resident os=4 model only on hot transients and runs os=1
  when quiet — near-os=4 alias quality (measured +32 dB cleaner than os=1 on a
  hot tone) at near-os=1 average cost (~2.4× solves vs 4× for static os=4 on
  sparse material), at zero added latency. Off by default so updating never
  silently changes the sound or CPU of an existing session — opt in via the
  toggle. The toggle is greyed out unless the tier is lossy and Oversampling is
  Auto. Off (or any other tier/OS setting) is the plain os=1 path, bit-identical
  to the non-adaptive build. Opt the whole
  mechanism out at compile time with `--no-default-features`.
- **Transformer-core nonlinearity** (`trafo-core` feature, on by default): a
  Jiles-Atherton output-transformer model exposed via three controls — a
  **Transformer** ON/OFF switch (OFF = bit-exact linear core) plus **Hysteresis**
  (loop width: pinning `k` + reversibility `c`) and **Saturation** (knee position
  `b_sat`) dials. The factory default (ON, both dials at noon) reproduces the
  legacy single-`trafo`-knob sound at its `0.5` default.
- **Neve 1073/1076-style EQ** (`EQ` ON/OFF + per-band controls): an optional
  analog-matched biquad EQ placed ahead of the model (the EQ network sits upstream
  of the BA283/AV card on the real strip, per schematic EH 10023). Switched HPF
  (Off / 50 / 80 / 160 / 300 Hz, ~18 dB/oct), switched low shelf (35 / 60 / 110 /
  220 Hz, ±16 dB), switched mid/presence bell (360 / 700 / 1.6k / 3.2k / 4.8k /
  7.2k Hz, ±18 dB, proportional-Q) and a fixed 12 kHz high shelf (±16 dB). OFF +
  flat defaults are a provable bit-exact no-op (the WDF golden-vector contract is
  unaffected); gain knobs are per-block glided to avoid zipper on a sweep. The mid
  proportional-Q table and shelf shapes are provisional pending a 1073-curve fit.
- **DPR — Dual Phase Rotator** (`dpr_on` ON/OFF + `dpr_amount`): an optional
  broadcast-style all-pass phase rotator on the output path (ahead of the
  levelNorm makeup gain and the output gain, both backends). Four cascaded first-order all-pass
  sections — the canonical CBS/Orban "phase asymmetry eliminator" topology —
  redistribute waveform asymmetry about the zero axis (reshaping the attack /
  peak symmetry) WITHOUT changing the magnitude spectrum (linear, adds no
  harmonics). The Amount knob sweeps the shared corner frequency on a log scale
  (~80 Hz at 0, ~200 Hz at the 0.5 default, ~500 Hz at 1). OFF (default) is a
  bit-exact no-op; per-channel state. The effect is signal-dependent — strongest
  on asymmetric sources, near-inaudible on symmetric ones. Note: the cascade is
  RMS-flat but phase redistribution can RAISE the peak level (inherent to phase
  rotation, independent of where it sits relative to the linear levelNorm/output
  scalars), so a hot input at unity output may clip — trim Output if so.
- Parameters: Backend, WDF Tier, Precision, Oversampling, Input Gain, Drive,
  Bias, Output, Mix, Level Norm, Link, Circuit Gain (5-position switch),
  Age (+ Re-roll),
  EQ (+ HPF / Low / Mid frequency selectors and Low / Mid / High gains),
  DPR (+ Amount).
- egui-based editor with Linux/X11 HiDPI scaling handling.

### Changed
- **WDF circuit backend: steep linear-phase FIR resampler (high-frequency fidelity).**
  The os=2 host path previously upsampled with linear interpolation and decimated
  with a 4th-order Butterworth — both gentle filters that quietly rolled off the top
  octave *on top of* the faithful circuit response: ≈6 dB @20 kHz at 48 kHz host and
  ≈9 dB at 44.1 kHz (measured, clean signal — `plugin/tests/fr_probe.rs`). These are
  now replaced by a windowed-sinc (Kaiser) polyphase upsampler + steep decimator with
  a passband flat to ~20 kHz, removing the numerical rolloff (to **0.0 dB @20 kHz**
  at 48 kHz, **−0.6 dB** at 44.1 kHz) while leaving the **real circuit FR untouched**
  (still −0.6 dB vs ngspice across 20 Hz–20 kHz, coherence 1.000). Cost: a reported
  group-delay **latency of ~80 host samples** (the WDF path was 0-PDC before), folded
  into the host PDC report and the dry/wet alignment so Mix stays phase-locked and a
  live backend switch needs no renegotiation. This figure is **tier-invariant**: the
  FIR group delay is constant across os ∈ {2,4}, and the os=1 tiers (UltraFast/Warp at
  High, which have no FIR) are **padded** up to the same value, so a live quality/tier
  switch also reports the same latency — the host never has to renegotiate PDC mid-
  stream (this nih-plug rev reports latency only once, at `initialize`). Note: switching
  to/from the WDF backend now adds ~1.7 ms of monitoring latency at 48 kHz vs the old
  0-PDC path. The legacy resampler remains available (`--legacy-resample`; used by the
  C++ golden-parity test, genuine 0-PDC) and the os=1 tiers are otherwise unchanged.
- **Stepped circuit gain + level-neutral gain-staging paradigm (BREAKING param
  changes).** The BA283 has a *stepped* 5-position gain switch and no continuous
  "drive" of the circuit; the plugin now matches that:
  - **`circuit_gain` is now a 5-position switch** (`IntParam` 0..4 → 15/20/25/30/35
    dB), replacing the continuous `FloatParam` (47–10 kΩ). **Param-type change:**
    hosts that automated `circuit_gain` as a continuous ohm value must re-map the
    lane to the discrete index. The redundant **`gain_stepped`** toggle is removed
    (the param is inherently discrete now).
  - **New gain-staging controls:** **`input_gain`** (clean pre-circuit dB trim),
    **`link`** (couples input/output in opposite directions for manual net-neutral
    staging). Signal chain is now
    `x → ×input_gain → EQ → ×drive → circuit → DPR → [levelNorm comp] → ×output → mix`,
    so `drive` pushes the **post-EQ** signal into the circuit (the saturation
    amount).
  - **`level_norm` now defaults ON** (was OFF) and governs full neutrality: at
    defaults the insert is **unity** (fixes the historical "+30 dB louder at
    default" bug). ON forces `input_gain`/`link` neutral and grays them, but
    **`output` stays live** as the post-normalization level trim (DRIVE now colours
    at constant loudness, so OUTPUT is the user's final level control); OFF restores
    faithful manual gain staging. **Behaviour change for existing in-dev sessions:**
    a default insert no longer raises level, and a session saved with levelNorm ON +
    a non-zero OUTPUT now applies that OUTPUT (it was previously forced to unity).
  - **levelNorm is a static per-position, DRIVE-swept table**
    (`GAIN_STEPS[pos].wdf_makeup` = measured `1/G_sat(drive)` across `DRIVE_GRID_DB`,
    interpolated by `wdf_makeup_at`, from `bin/measure_gi`, drift-guarded across the
    full 2-D grid by `tests/test_stepped_gain.rs`) instead of a runtime off-thread
    gain probe — the probe worker/task is removed, making levelNorm deterministic and
    alloc-free. Storing the large-signal saturated gain (not a single small-signal
    `1/G_v`) stops the `/drive` cancellation from over-attenuating once DRIVE pushes
    the circuit into saturation, so DRIVE colours at constant loudness instead of sagging.
  - **NN backend** is conditioned by the discrete position (mapped via its hardware
    R_KJ). *Staged:* a full NAM retrain for the 5 discrete positions +
    input-amplitude sweep is pending; the embedded weights remain provisional and
    still use the continuous FiLM cond map keyed by position R_KJ until then.
- **Circuit Gain and Level Norm now function on the Neural Net backend.** The
  Circuit Gain knob is no longer greyed out under the Neural Net backend — it drives
  the WaveNet's FiLM gain-conditioning, so it audibly changes level and saturation
  character. **Level Norm** now also works on the NN backend, folding loudness
  control into a single switch: OFF leaves the NN at its natural FiLM-conditioned
  level (default — bit-identical to before this change); ON levels the output to
  ~constant across gain and Drive via an offline-calibrated `1/G_nn` trim
  (`nn_norm_trim`, from `NN_GAIN_CALIB`; analytic, no off-thread probe). The
  calibration table is regenerated and validated against the embedded model by
  `tests/calib_nn_makeup.rs` (no live probe — the model is a fixed black box).
  (Level Norm now defaults **ON** for a unity insert — see the stepped-gain entry
  above; the WDF backend reads the static per-position makeup table.)
- **Neural Net path is now safety-clamped against runaway under heavy Drive.** The
  WaveNet is only valid over the level range it was trained on; far past that it
  extrapolates and (with the current provisional weights) can diverge above ~+12 dB
  Drive, which drove the raw NN output to damaging levels. Three weight-agnostic,
  always-on guards now bound it, all **bit-exact for in-domain signal** (the golden
  PyTorch-parity test still passes): an **input-domain clamp** (`NN_INPUT_CLAMP`,
  +12 dBFS) on the signal fed to the net — NeuralNet backend only, so the net
  saturates at the bound instead of extrapolating (the WDF circuit is physically
  self-limiting and is left untouched); a **finite guard** (any NaN/Inf from the
  model becomes 0, so it can never poison the mix or the host); and a
  **final-output ceiling** (`NN_OUTPUT_CEIL`, +18 dBFS) as a last-line speaker-safety
  backstop on the returned sample. Regression-guarded by
  `tests/test_nn_levers.rs::nn_output_is_bounded_under_extreme_drive`.
  *Note:* the WDF Drive-swept levelNorm makeup is **not** mirrored on the NN path
  yet — with provisional weights a Drive-swept `1/G_nn_sat` table would bake
  divergent values. The offline generator `bin/measure_nn_gi` is in place to produce
  it (one command) once final weights land; until then the NN levelNorm uses the
  small-signal `1/G_nn` trim and the clamps bound the rest.
- **Editor reflowed into a left→right signal-flow column layout.** The window is
  now **1040×660** (was a tall top-down stack that overran its fixed viewport and
  clipped at the bottom). Controls are grouped into four vertical columns ordered
  by signal flow — **INPUT · EQUALIZER · MODEL · OUTPUT** — separated by
  full-height rules, with a top bar carrying the wordmark, backend toggle, and the
  global **Level Norm** / **Link** modes (they gate controls now split across
  columns). The MODEL column leads with the backend-agnostic **Circuit Gain** +
  **Drive** (full-opacity under the Neural Net backend); the WDF-only character and
  quality controls live in a sub-frame below that fades/disables under NN. The
  body is wrapped in a scroll area so it is structurally clip-proof — the default
  size fits everything (no scrollbars), but any future control or a smaller host
  hole scrolls instead of clipping. Presentation only: no DSP, params, palette, or
  feature-gate changes; all enable/grey logic and widget helpers are reused.

### Internal (off by default, not shipped in the default build)
- `adaptive-os` feature: adaptive level-gated oversampling for the lossy
  UltraFast/Warp tiers — a per-channel envelope engages a resident os=2 model only
  on hot transients. Gated behind a cargo feature; the default build is
  bit-identical. Still under evaluation (no UI exposure, no default-on).
- `adaa-insolve` / `adaa-port` features: retained in-solve ADAA spike code, both
  measured **NO-GO** and never compiled into the shipped plugin (documented trail
  in `docs/plans/2026-06-17-*`).

### Performance
- **Off-thread WDF model rebuild** — a quality-tier / oversampling / Adaptive-OS
  switch is now **alloc-free on the audio thread**. The whole-model rebuild (which
  allocates the circuit-build scratch + the depletion-charge LUTs) for **both** the
  resident `wdf` and the adaptive `wdf_hi` siblings runs on the existing background
  worker (`run_rebuild`, mirroring the levelNorm probe); `process` only pointer-swaps
  the worker-built models in (`ChannelDsp::install_prebuilt`) and parks the retired
  models in a fixed-capacity ring for the worker to drop, so neither the alloc nor the
  free touches the audio thread. The swap is deferred a few ms until the build lands
  (the old model keeps playing) and masked by the existing 60 ms crossfade; rapid
  sweeps coalesce to the latest switch. Guarded by `test_rebuild_noalloc`
  (`assert_no_alloc`, default and `--no-default-features`). Hardening: the worker
  drains the retire ring by moving the slots out under the lock and dropping them after
  release, so the lock never spans a `free` (the audio thread's `try_lock` can't contend
  with an in-progress drop → the inline-free fallback stays unreachable); a stall
  watchdog (`REBUILD_STALL_LIMIT`) force-releases the `busy` latch if a posted rebuild is
  ever silently dropped by a full task queue, so a switch can never wedge. Coalescing /
  stale-gen drop covered by `test_rebuild_coalesce`.
- **Depletion-charge LUT** on the lossy WDF tiers: the per-junction `pow` in the
  BJT depletion-charge kernel is replaced by a precomputed 1-D table + linear
  interp (the `pow` arm only; the linear arm is unchanged). Built once off the
  audio thread and shared via `Arc` (RT-safe clone). On for `Fast` /
  `UltraFast` / `Precision::Medium`; **`Realtime` keeps the bit-exact `pow` path
  and the C++ golden-vector contract is unchanged**. Measured ≈ +9–10 % on
  Fast/UltraFast with added error vs the exact path ≈ −78 dB (well inside the
  −40 dB tier budget; guarded by `test_dep_lut`).

### Platforms & formats
- **Linux x86_64** and **macOS arm64**.
- **VST3** and **CLAP** bundles per platform.

### Known limitations
- The Linux binary requires an **x86-64-v3** CPU (AVX2 + FMA; Haswell-2013+ /
  Excavator-2015+). Older CPUs will fault with an illegal instruction.
- macOS builds are **unsigned and unnotarized** — clear the gatekeeper
  quarantine flag on first install (`xattr -dr com.apple.quarantine`).
- No Windows build, no macOS Intel/universal build in this release.
- **Parameter-id change**: the single `trafo` parameter was replaced by
  `trafo_on` / `trafo_hyst` / `trafo_sat`. Hosts key automation lanes and saved
  presets by parameter id, so any project that automated or stored a
  *non-default* `trafo` value will load the new defaults instead (which equal the
  legacy `trafo=0.5` sound). Projects left at the default are unaffected. nih-plug
  offers no id-aliasing/migration hook, so this is one-time and unavoidable.

[0.1.0]: ../../releases/tag/v0.1.0
