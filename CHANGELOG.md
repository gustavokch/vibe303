# Changelog

All notable changes to vibe303 are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - Unreleased

### Added
- Windows `x86_64` VST3 + CLAP build targets. New `build-plugins-windows`
  GitHub Actions workflow builds on a hosted `windows-latest` runner in two CPU
  variants (`x86-64-v3` AVX2, `x86-64-v2` compatibility), and
  `scripts/build-windows-x86_64.ps1` builds and installs the same bundles
  locally. Windows bundles ship with the GUI but without the standalone binary.
- Phase 1 — TB-303 analog circuit model: `circuit/vco303.toml`, `circuit/vcf303.toml`,
  emitted to SPICE + ACME via `tools/circuitgen` and ngspice-validated under `eval/303/`
  (VCO 1 V/oct; VCF 4-pole LP, bias-tracking cutoff, active-Q9 resonance, tap-select poles).
- Phase 2 — TB-303 nih-plug voice design + implementation plan under `docs/plans/`.

### Changed
- Quality tiers renamed in fidelity order: Draft (was Low), Low (was High),
  High (was Ultra 2x), Best (was Ultra). The GUI QUALITY switch, the host
  parameter value names, `render303 --tier` and `profile_voice --tier` use the
  new names; `--tier` takes names only (`draft|low|high|best`), not indices.
  The QA state `quality_high` now selects the High tier; the capture suite uses
  `quality_low` for what `quality_high` captured before. Sessions saved before
  this change: a saved Ultra (index 2) now loads as High; a saved Ultra 2x
  (index 3) now loads as Best. DAW automation lanes that stored index 2 or 3
  change the same way.
- `eval/303/calibrate_vcf.py` fits the Draft and Low control maps against Best
  and prints the `DRAFT_*_POLY` / `LOW_*_POLY` constants for `voice.rs`. Its
  probe binary `vcf_sweep_helper` is restored and builds each tier's filter the
  way the voice does (oversampling factor and brightness calibration).

### Fixed
- The High tier was padded for Best's 4x oversampling latency instead of its
  own 2x latency, so its output came out 15 samples early (about 0.3 ms at
  48 kHz) and a High crossfade blended misaligned paths. High now has its own
  compensation delay, and every tier reaches the output at the reported
  latency.
- A crossfade between tiers that shared DSP state ticked that state twice per
  sample for the 10 ms fade. High <-> Best ran the circuit VCO at double rate
  (an audible pitch jump, then a phase offset); Low <-> High and the circuit
  tiers also shared the 2x oversampler and the sub oscillator. Each tier now
  owns its oversampler and nodal ladder, and the circuit VCOs and sub
  oscillator tick once per sample, also during a High <-> Best fade.

### Removed
- The BA283 Neve-preamp product (plugin crate, WaveNet/LSTM model, WDF engine, training
  and data-generation pipelines, colab notebooks, datasets, ba283 eval suites and docs).
  The repo is now the standalone 303 project; the shared `circuitgen` circuit-IR pipeline
  and ngspice/ACME validation harness were kept. Removed code remains in git history.
