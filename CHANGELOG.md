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

### Removed
- The BA283 Neve-preamp product (plugin crate, WaveNet/LSTM model, WDF engine, training
  and data-generation pipelines, colab notebooks, datasets, ba283 eval suites and docs).
  The repo is now the standalone 303 project; the shared `circuitgen` circuit-IR pipeline
  and ngspice/ACME validation harness were kept. Removed code remains in git history.
