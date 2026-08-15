# vibe303 — Souped-up TB-303

Circuit-grounded emulation of the Roland TB-303 bass synthesizer, built in Rust with [nih-plug](https://github.com/robbert-vdh/nih-plug) and [egui](https://github.com/emilk/egui). Features a SPICE-validated analog core, multi-tier DSP engine (TPT up to full WDF ladder), authentic accent/glide dynamics, an authentic decoupled 16-step sequencer, and a collapsible "Souped-Up" modulation/extended-voice drawer.

- **Version:** 0.1.0
- **Formats:** VST3, CLAP, Standalone
- **Platforms:** macOS (Apple Silicon), Linux (x86_64 AVX2)

---

## Key Features

### 1. Multi-Tier Circuit Modeling
- **Low / Eco Tier:** Real-time optimized TPT (Topology-Preserving Transform) diode ladder filter.
- **High Tier:** 4x oversampled non-linear filter with saturation modeling.
- **Ultra Tier:** Full Wave Digital Filter (WDF) with iterative non-linear Newton-Raphson BJT/diode solvers, capacitor ESR, thermal noise, component aging, and tolerance drift LUT.
- **Topology Switching:**
  - **Accurate:** Exact Roland TB-303 circuit domain with active Q9 feedback scaling and authentic non-self-oscillating resonance behavior.
  - **Fun:** Extended resonance range with self-oscillation, variable drive, and additional filter poles.

### 2. Sound Engine & Voice Architecture
- **Oscillator Core:** Antialiased PolyBLEP / ADAA oscillators with classic Saw, Square, and continuous Saw↔Square sweep.
- **Souped-Up Levers:** Dual oscillator section with independent pitch/level controls, sub-oscillator, and pulse-width modulation (PWM).
- **Diode-Ladder VCF:** Switchable 2-pole (12 dB/oct), 3-pole (18 dB/oct), and 4-pole (24 dB/oct) slopes with passband resonance gain compensation.
- **Envelope & Accent Circuit:**
  - SPICE-calibrated MEG and VEG RC envelope discharge curves.
  - Authentic accent dynamics with capacitor sweep memory accumulation and decay ducking.
  - Optional **Advanced Envelope** mode with full MEG/VEG ADSR mini-faders and VCA Gate/Env selector.
- **Analog Drift & Aging:** Simulated component tolerances (0% to 10%), aging years, thermal noise, and repeatable deterministic seeds.

### 3. Authentic 16-Step Sequencer
- **Dual Sequencing Modes:**
  - **Direct Grid:** Modern 16-step DAW-style step sequencer.
  - **Decoupled (Roland TB-303):** Authentic hardware mode with decoupled pitch and timing (Note / Tie / Rest) entry buffers.
- **Timebases:** Straight 16th notes and Triplet 12th notes with analog clock jitter/swing simulation.
- **Per-Step Parameters:** Pitch (C1–C4), Octave transpose (-2..+2), Step Gate (Note, Tie, Rest), Accent, Slide (hardware glide modeling), Velocity, and Gate Length.
- **Interactive UI:** Step LED playhead, interactive piano roll popup, mini keyboard preview, and host transport sync.

### 4. Hardware-Inspired Flat GUI
- Custom vector interface designed with `nih_plug_egui`.
- Textured aluminum finish with metallic dials, physical engineering units readout (`cents`, `Hz`, `dB`, `%`, `ms`, `BPM`), and tactile switches.
- Retractable **Souped Drawer** on the right panel, neatly organizing extended synth parameters while dimming inactive controls in Accurate mode.

### 5. Real-Time Audio Performance
- **100% Real-Time Safe:** Allocation-free audio processing thread guarded by `assert_no_alloc`.
- Thread-safe atomics and triple buffering for GUI-to-audio communication.

---

## Project Structure

```
vibe303/
├── circuit/               # Circuit-IR definitions (vco303.toml, vcf303.toml)
├── tools/
│   ├── circuitgen/        # IR → SPICE / ACME netlist generator & drift guard
│   └── schemagen/         # SPICE-deck and schematic parser
├── eval/
│   ├── 303/               # ngspice & ACME validation testbenches and calibration scripts
│   └── acme_vs_spice/     # Julia ACME environment for circuit parity
├── plugin303/             # Main Rust plugin crate (nih-plug)
│   ├── src/
│   │   ├── dsp/           # Audio engine: VCO, WDF/TPT ladder, envelopes, sequencer, oversampling
│   │   ├── gui/           # egui interface: dials, panels, step strip, piano roll popup
│   │   ├── params.rs      # Plugin parameter tree & persistence
│   │   └── lib.rs         # Plugin lifecycle and VST3/CLAP entrypoints
│   └── tests/             # Extensive DSP integration and regression test suite
├── xtask/                 # Plugin bundler runner
└── docs/                  # Design documents, circuit analysis, and calibration specs
```

---

## Building and Running

### Prerequisites
- **Rust:** Stable toolchain (edition 2021)
- **Python (Optional, for SPICE validation):** Python 3.12+ with `uv`
- **System Libraries (Linux):** `libasound2-dev`, `libjack-dev`, `libx11-dev`, `libxcb1-dev`, `libgl1-mesa-dev`

### Running the Standalone Synth
```bash
cargo run --bin standalone
```

### Running Tests
```bash
# Run all DSP, sequencer, and realtime safety tests
cargo test

# Run allocation-free audio thread verification
cargo test --test no_alloc
```

### Building VST3 and CLAP Bundles
```bash
# Bundle release binaries for current architecture
cargo xtask bundle plugin303 --release

# Bundled artifacts will be located in:
# target/bundled/TB303.vst3
# target/bundled/TB303.clap
```

### Circuit IR & SPICE Verification
```bash
# Validate circuit IR integrity
uv run --no-project python -m tools.circuitgen --check
uv run --no-project --with pytest python -m pytest tests/circuitgen/ -q

# Run ngspice validation runs
uv run --no-project --with PySpice --with numpy python eval/303/run_vco303_spice.py 5
uv run --no-project --with PySpice --with numpy python eval/303/run_vcf303_spice.py --reso
```

---

## CI / Cross-Compilation

GitHub Actions workflows provide automated building and packaging:
- **Linux:** Native `x86_64` (AVX2 / `x86-64-v3`) VST3/CLAP builds.
- **macOS:** Apple Silicon (`aarch64-apple-darwin`) builds.
