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
    gain-conditioned, RF 6347), exported from a `nam` 0.13 fork as
    `model.cond.nam`, embedded at build time. Streaming/causal → **zero added
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
- Parameters: Backend, WDF Tier, Precision, Oversampling, Drive, Bias, Output,
  Mix, Level Norm, Circuit Gain, Stepped Gain, Age (+ Re-roll),
  EQ (+ HPF / Low / Mid frequency selectors and Low / Mid / High gains).
- egui-based editor with Linux/X11 HiDPI scaling handling.

### Changed
- **Circuit Gain and Level Norm now function on the Neural Net backend**, matching
  the WDF backend's behaviour. The embedded WaveNet already conditions on Circuit
  Gain (FiLM) but rendered ~6 dB hot versus the WDF circuit and saturated its
  conditioning axis below 82 Ω; an offline-calibrated loudness makeup
  (`NN_GAIN_CALIB`, derived by `tests/calib_nn_makeup.rs`) now aligns the NN's
  absolute level with the WDF circuit at every gain, so A/B-ing backends is
  loudness-matched and the gain knob audibly changes level (not just tone). With
  **Level Norm** on, the NN output is leveled to ~constant across gain and Drive
  (analytic `1/G_nn` trim — no off-thread probe; the WDF probe path is unchanged).
  The Circuit Gain knob is no longer greyed out under the Neural Net backend.
  - *Behaviour note:* existing Neural Net sessions get **~6 dB quieter** at the
    default gain now that the NN is calibrated to the WDF circuit's absolute level.

### Performance
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
