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
  - **Neural Net** (default): pure-Rust `LSTM(1, 32)` + FiLM gain conditioning +
    `Dense(1)`, weights embedded at build time.
  - **Circuit (WDF)**: nodal MNA + Newton-Raphson solver with Gummel-Poon BJT
    models and coupled inductors. Two quality tiers — `Realtime` (bit-exact
    reference) and `Fast` (near-transparent, faster).
- **Transformer-core nonlinearity** (`trafo-core` feature, on by default): a
  Jiles-Atherton output-transformer model exposed via three controls — a
  **Transformer** ON/OFF switch (OFF = bit-exact linear core) plus **Hysteresis**
  (loop width: pinning `k` + reversibility `c`) and **Saturation** (knee position
  `b_sat`) dials. The factory default (ON, both dials at noon) reproduces the
  legacy single-`trafo`-knob sound at its `0.5` default.
- Parameters: Backend, WDF Tier, Drive, Bias, Output, Mix, Circuit Gain.
- egui-based editor with Linux/X11 HiDPI scaling handling.

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
