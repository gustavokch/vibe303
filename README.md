# BA283

Neural **and** circuit emulation of the Neve BA283 mic-preamp output stage, shipped as a VST3 plugin.

- **Version:** 0.1.0
- **Vendor:** [gruber.dev.br](https://gruber.dev.br) · contact@gruber.dev.br
- **License:** TBD

---

## What it is

The BA283 is the output stage of a classic Neve mic preamp: a three-transistor
(TR1/TR2 BC184C + TR3 2N3055) global-feedback, d.c.-coupled amplifier driving a
Marinair/Carnhill LO1166 output transformer. Gain is set by a resistor shunt
(`R_KJ`, ~220 Ω–10 kΩ → ~14–33 dB) and bias trims TR3's idle current for
symmetric clipping.

This project reproduces that stage as a plugin with **two runtime-selectable
inference backends**, both validated against ngspice renders of the real circuit.

## The two backends

### Neural Net (default)

Hand-coded pure-Rust `LSTM(1, 32)` + FiLM gain conditioning + `Dense(1)`,
learned from SPICE-rendered dry/wet pairs. Zero-alloc and realtime-safe; the
trained weights are embedded into the binary at build time. The Rust inference
matches the PyTorch reference to `< 1e-5`.

→ `plugin/src/model.rs`, weights `plugin/weights/ba283_lstm32.json`

### Circuit (WDF)

A nodal MNA + Newton-Raphson solver with Gummel-Poon BJT models and coupled
inductors, ported from [`gustavokch/ba283_wdf`](https://github.com/gustavokch/ba283_wdf).
Circuit-faithful: tracks SPICE at ESR ≈ −30 dB, coherence 1.0. Offers two
quality tiers — `Realtime` (bit-exact reference, all speed levers off) and
`Fast` (near-transparent, faster).

→ `plugin/src/wdf.rs`

## Build

**Prerequisites:** Rust `1.93.0` (auto-pinned by `plugin/rust-toolchain.toml` if
you use rustup).

```bash
cargo xtask bundle ba283 --release      # → VST3 bundle under target/bundled/
```

Standalone CLI renderer (offline processing, no DAW):

```bash
cargo run --release --bin ba283_standalone -- --input dry.wav --output wet.wav
# WDF backend at a given gain shunt:
cargo run --release --bin ba283_standalone -- \
    --input dry.wav --output wet.wav --backend wdf --circuit-gain 220 --tier fast
```

Run `ba283_standalone` with no args to see the full flag list (`--bias`,
`--drive-db`, `--output-db`, `--mix`, `--backend nn|wdf`, `--tier realtime|fast`,
`--raw`, …).

## Install

The release build emits a VST3 bundle in `target/bundled/`. On Linux, copy it
into your plugin path:

```bash
cp -r target/bundled/BA283.vst3 ~/.vst3/
```

Linux (`x86_64-unknown-linux-gnu`) is the tested target.

## Usage in a DAW

Load **BA283**, then drive these parameters:

| Parameter | Notes |
|-----------|-------|
| **Backend** | `Neural Net` (default) or `Circuit (WDF)` |
| **WDF Tier** | `Realtime` / `Fast` — WDF-only, inert for Neural Net |
| **Drive** | input drive (dB) |
| **Bias** | TR3 bias trim, 0–1 |
| **Output** | output trim (dB) |
| **Mix** | dry/wet |
| **Circuit Gain** | `R_KJ` shunt (ohms) — WDF-only, inert for Neural Net |

Full parameter definitions live in `plugin/src/lib.rs`.

## ML / data pipeline

The Neural Net backend is trained offline against the real circuit:

```
data_gen/   SPICE: parameterized netlist → ngspice transient render (dry/wet pairs)
   │        netlist.py · run_spice.py · stimulus.py · resample.py
   ▼
train/      TBPTT training, composite ESR+MSE loss, FiLM gain conditioning
   │        train.py → best.pth
   ▼
train/export.py   serialize → self-describing JSON
   ▼
plugin/weights/ba283_lstm32.json   embedded into the plugin at build time
```

Current dataset: `ba283_spice_gain` (gain-conditioned, leakage-free split).
Cloud training is available via `colab/ba283_train_colab.ipynb`.

- **How (operator runbook):** [`docs/training_runbook.md`](docs/training_runbook.md)
- **Why (concepts — ESR, TBPTT, FiLM):** [`docs/training_explained.md`](docs/training_explained.md)

## Evaluation

- [`eval/wdf_vs_spice/`](eval/wdf_vs_spice/README.md) — validates the WDF solver
  against SPICE (ESR, NRMSE, Welch FR coherence).
- [`eval/acme_vs_spice/`](eval/acme_vs_spice/README.md) — ACME.jl circuit-model
  feasibility spike vs SPICE.

## Repository map

| Path | Purpose |
|------|---------|
| `plugin/` | Rust VST3 plugin (nih-plug): `lib.rs`, `model.rs`, `wdf.rs`, `dsp.rs`, weights |
| `xtask/` | nih-plug build runner (`cargo xtask bundle ba283`) |
| `train/` | PyTorch training pipeline (`train.py`, `model.py`, `export.py`) |
| `data_gen/` | SPICE dataset generation (PySpice + ngspice) |
| `eval/` | Validation harnesses (`wdf_vs_spice`, `acme_vs_spice`) |
| `colab/` | Google Colab training notebooks |
| `docs/` | Concept guide, runbook, design plans |
| `tests/` | Python + Rust tests |
| `checkpoints/` | Training checkpoints (`best.pth`, `last.pth`) |

## Development

**Rust:**

```bash
cargo test
```

**Python (3.12, [uv](https://docs.astral.sh/uv/)):**

```bash
uv sync
uv run pytest        # tests in tests/
uv run ruff check .
```
