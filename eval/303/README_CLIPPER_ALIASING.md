# WarmClipper Aliasing Evaluation Tooling

Tooling to evaluate and plot aliasing fold-down energy and total harmonic distortion (THD) of the `WarmClipper` output stage.

## Components

1. **Rust Signal Generator & DSP Runner (`plugin303/src/bin/eval_clipper.rs`)**
   - Renders `WarmClipper` DSP directly using raw f32 audio samples.
   - Configurable options: `--sr`, `--os` (2x/4x), `--bypass`, `--drive`, `--thresh`, `--bass`, `--tone`, `--asym`, `--mix`, `--in-gain`, `--out-gain`, `--tilt`, `--dynamics`, `--signal` (sine/sweep/raw), `--freq`, `--amp`, `--secs`, `--in`, `--out`.
   - Control dimensions: `--mix` dry/wet 0.0..1.0 (default 1.0), `--in-gain`/`--out-gain` pre/post gain dB -18..18 (default 0.0), `--tilt` pre/post shelf dB -6..6 (default 0.0), `--dynamics` transient ducking 0.0..1.0 (default 0.0).

2. **Python Analysis & Dataviz Pipeline (`eval/303/eval_clipper_aliasing.py`)**
   - Runs `eval_clipper` subprocesses across signal configurations.
   - Extracts harmonic energy vs off-grid alias energy to compute Alias-to-Signal Ratio (ASR dB).
   - Generates theme-aware, dataviz-compliant PNG plots and JSON metric summary.

## Usage

### 1. Build the Rust CLI binary
```bash
cargo build --release --bin eval_clipper
```

### 2. Run the evaluation & plotting suite
```bash
uv run python eval/303/eval_clipper_aliasing.py
```

Optional arguments:
```bash
uv run python eval/303/eval_clipper_aliasing.py --sr 96000 --f0 2500
```

### 3. CLI usage for manual DSP testing
Run custom sine tone through 4x oversampled clipper:
```bash
target/release/eval_clipper --sr 48000 --os 4x --freq 2500.0 --drive 68000 --out test_out.f32
```

## Outputs

All artifacts land in `eval/303/results/clipper_aliasing/`:
- `clipper_psd_spectrum.png`: PSD spectrum comparison (Bypass vs 2x OS vs 4x OS) for off-grid test frequency.
- `clipper_asr_vs_freq.png`: Alias floor (ASR dB) across fundamental frequency sweep (100 Hz to 10 kHz).
- `clipper_aliasing_vs_drive.png`: THD & ASR vs Drive saturation resistance ($22\text{ k}\Omega \to 100\text{ k}\Omega$).
- `clipper_control_sweeps.json`: THD & ASR across the control dimensions (mix 0..1, dynamics 0..1 at max drive, tilt ±6 dB).
- `summary.json`: Numeric metrics summary.
