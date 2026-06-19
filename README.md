# BA283

Neural **and** circuit emulation of the Neve BA283 mic-preamp output stage, shipped as a VST3 plugin.

- **Version:** 0.1.0
- **Vendor:** desmen
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
The default backend is **Circuit (WDF)**; the Neural Net is the learned
alternative.

## The two backends

### Neural Net

Hand-coded pure-Rust streaming **FiLM WaveNet** matching a `nam` 0.13-fork
export: 23 dilated causal conv layers, 3 channels, 2-channel condition
`[audio, gain]` (gain-conditioned), per-layer `LeakyReLU(0.01)`, receptive field
6347. Streaming and causal, so it adds **zero latency**. Zero-alloc and
realtime-safe — all weight/state storage is heap-boxed once at construction; the
audio path only indexes fixed scratch + ring buffers. Bit-exact with the golden
reference in steady state.

→ `plugin/src/wavenet.rs`, weights `plugin/weights/model.cond.nam`

(The older NAM-style `LSTM(3 x 18)` engine — `plugin/src/model.rs`, schema
`ba283-lstm-v2` — is still compiled but dormant; it is the model the in-repo
`train/` pipeline produces. See [ML / data pipeline](#ml--data-pipeline).)

### Circuit (WDF, default)

A nodal MNA + Newton-Raphson solver with Gummel-Poon BJT models and coupled
inductors, ported from [`gustavokch/ba283_wdf`](https://github.com/gustavokch/ba283-wdf).
Circuit-faithful: tracks SPICE at ESR ≈ −30 dB, coherence 1.0. Includes a
Jiles-Atherton model of the LO1166 output-transformer core (feature `trafo-core`,
on by default) and per-component aging.

Four quality tiers trade accuracy for speed:

- `Realtime` — bit-exact reference, all speed levers off (2× oversampled).
- `Fast` — predict + Schur + freeze-caps, near-transparent, ~5× faster.
- `UltraFast` — adds cheap transport-exp and drops to 1×, ~9× faster.
- `Warp` — also drops the BJT junction-charge model, ~12× faster.

The two 1× tiers (`UltraFast`, `Warp`) can opt into **Adaptive OS** (below),
which engages a resident `os=4` model on hot transients to claw back their
aliasing without paying the oversampling cost on quiet material.

Two orthogonal dials further tune the WDF backend. **Precision** is the
device-model fidelity axis — it steps the BJT charge-storage model down from the
tier baseline (and never up), independent of the tier's iteration strategy and
the oversampling rate:

- `High` — identity; preserves the bit-exact Realtime/golden contract.
- `Medium` — junction charge model → depletion-only, plus the cheap low-order
  transport exp (~−3%; the depletion step itself is near-zero CPU, fidelity-only).
- `Low` — drop the charge model entirely (the dominant lever, ~−26% on Fast),
  plus the cheap transport exp.

Steps are monotone (a tier already at the lowest charge model, e.g. `Warp`,
stays there). Loss stays within the harness budget (added ESR vs Realtime+High
≤ −50 dB; the ACME model floor is ~−23 dB).

**Oversampling** (`Auto`/`1×`/`2×`) is the antialiasing-rate axis: `Auto` defers
to the tier, `1×`/`2×` force the internal rate.

**Adaptive OS** is the level-gated companion to the `1×` lossy tiers. A
peak-follower envelope on the driven input arms a resident `os=4` model on hot
transients (where `os=1` aliases most) and holds `os=1` when quiet — so you pay
the oversampling cost only on the transients that need it. It ships **off**
(opt-in) and is inert unless the tier is `UltraFast`/`Warp` and Oversampling is
`Auto`. See [`docs/adr/0001-adaptive-oversampling.md`](docs/adr/0001-adaptive-oversampling.md).

→ `plugin/src/wdf/` (solver module). Architecture decisions are recorded under
[`docs/adr/`](docs/adr/).

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
`--drive-db`, `--output-db`, `--mix`, `--backend nn|wdf`,
`--tier realtime|fast|ultra-fast|warp`, `--precision high|medium|low`,
`--level-norm`, `--raw`, …).

## Install

Grab the zip for your platform from the [Releases](../../releases) page. Each
archive contains both the **VST3** and **CLAP** bundles. Verify the download
against `SHA256SUMS`, then unzip and copy the bundles into your plugin path.

**Linux x86_64:**

```bash
unzip BA283-v0.1.0-linux-x86_64.zip
cp -r BA283.vst3 ~/.vst3/
cp -r BA283.clap ~/.clap/
```

**macOS arm64:**

```bash
unzip BA283-v0.1.0-macos-arm64.zip
cp -r BA283.vst3 ~/Library/Audio/Plug-Ins/VST3/
cp -r BA283.clap ~/Library/Audio/Plug-Ins/CLAP/
# Builds are unsigned/unnotarized — clear the gatekeeper quarantine flag:
xattr -dr com.apple.quarantine ~/Library/Audio/Plug-Ins/VST3/BA283.vst3
xattr -dr com.apple.quarantine ~/Library/Audio/Plug-Ins/CLAP/BA283.clap
```

### System requirements

- **Linux:** x86_64 CPU with the **x86-64-v3** feature level (AVX2 + FMA —
  Intel Haswell 2013+ / AMD Excavator 2015+ or newer). The release binary is
  compiled for this floor; older CPUs will fault with an illegal instruction.
- **macOS:** Apple Silicon (arm64). Intel Macs are not built for v0.1.

To build from source instead, see [Build](#build) above.

## Usage in a DAW

Load **BA283** and drive its parameters. Groups marked _WDF-only_ are inert
when the Neural Net backend is selected.

**Backend & signal**

| Parameter | Range / values | Default |
|-----------|----------------|---------|
| **Backend** | `Circuit (WDF)` / `Neural Net` | Circuit (WDF) |
| **Drive** | −12 .. +24 dB | 0 dB |
| **Bias** | 0 .. 1 (TR3 idle-current trim) | 0.5 |
| **Output** | −24 .. +12 dB | 0 dB |
| **Mix** | 0 .. 100% (wet) | 100% |

**WDF quality** _(WDF-only)_

| Parameter | Range / values | Default |
|-----------|----------------|---------|
| **WDF Tier** | `Realtime` / `Fast` / `UltraFast` / `Warp` | Realtime |
| **Precision** | `High` / `Medium` / `Low` | High |
| **Oversampling** | `Auto` / `1×` / `2×` | Auto |
| **Adaptive OS** | bool — level-gated `os=4` on hot transients (1× lossy tiers only) | Off |

**Circuit** _(WDF-only)_

| Parameter | Range / values | Default |
|-----------|----------------|---------|
| **Circuit Gain** | `R_KJ` shunt, 47 .. 10 000 Ω | 220 Ω |
| **Stepped Gain** | bool — snap to 5 hardware positions (15/20/25/30/35 dB) | Off |
| **Age** | 0 .. 1 component drift (vintage wear) | 0 |
| **Age Re-roll** | momentary — pick a new random unit | — |

**Transformer core** _(WDF-only, feature `trafo-core`)_

| Parameter | Range / values | Default |
|-----------|----------------|---------|
| **Transformer** | On / Off (Jiles-Atherton LO1166 core) | On |
| **Hysteresis** | 0 .. 1 (loop width) | 0.5 |
| **Saturation** | 0 .. 1 (knee position) | 0.5 |

**EQ** _(both backends, Neve 1073/1076, pre-model)_

| Parameter | Range / values | Default |
|-----------|----------------|---------|
| **EQ** | On / Off (bit-exact when off) | Off |
| **HPF** | `Off` / 50 / 80 / 160 / 300 Hz | Off |
| **Low-Shelf** | freq 35/60/110/220 Hz · gain ±16 dB | 0 dB |
| **Mid / Presence** | freq 360 Hz..7.2 kHz · gain ±18 dB (proportional-Q) | 0 dB |
| **High-Shelf** | gain ±16 dB (fixed 12 kHz) | 0 dB |

**Other**

| Parameter | Range / values | Default |
|-----------|----------------|---------|
| **Level Norm** _(WDF-only)_ | On / Off — RMS-match output to input | Off |
| **UI Scale** | `Auto` / 100–200% (not automatable) | Auto |

The EQ sits **before** the modeled stage, matching the real channel strip
(`MIC → EQ → BA283/AV → LO1166 → OUT`), and is bit-exact when off. Full
parameter definitions live in `plugin/src/lib.rs`.

### GUI

BA283 ships a custom egui editor (660×900). The **CIRCUIT** panel dims and
disables when the Neural Net backend is active (its params are WDF-only).
HiDPI follows the host by default (`UI Scale = Auto`), with manual 100–200%
overrides for hosts that misreport their scale.

## ML / data pipeline

The neural backends are trained offline against the real circuit. The
**shipped** WaveNet (`model.cond.nam`) is exported by a custom `nam` 0.13 fork.
The in-repo pipeline below trains the **dormant** LSTM/GRU engine
(`plugin/src/model.rs`) and emits the `ba283-lstm-v2` JSON schema:

```
data_gen/   SPICE: parameterized netlist → ngspice transient render (dry/wet pairs)
   │        netlist.py · run_spice.py · stimulus.py · resample.py
   ▼
train/      TBPTT training, perceptual loss (pre-emphasised ESR + DC), FiLM gain conditioning
   │        train.py → best.pth
   ▼
train/export.py   serialize → self-describing JSON (schema ba283-lstm-v2)
   ▼
plugin/weights/ba283_lstm_3x18.json   embedded into the plugin at build time (dormant LSTM path)
```

Current dataset: `ba283_spice_gain` (gain-conditioned, leakage-free split).
Cloud training is available via `colab/ba283_train_colab.ipynb`.

- **How (operator runbook):** [`docs/training_runbook.md`](docs/training_runbook.md)
- **Ablation studies (Colab):** [`docs/colab_ablation_runbook.md`](docs/colab_ablation_runbook.md)
- **Why (concepts — perceptual ESR + DC loss, TBPTT, FiLM):** [`docs/training_explained.md`](docs/training_explained.md)

The training objective is the Wright & Välimäki perceptual loss (arXiv:1911.08922):
pre-emphasised ESR (`α = 0.8613`, 48 kHz matched-z of the paper's 0.85) plus a DC
term (`--dc-weight`, on by default) that catches the constant offset pre-emphasis is
blind to. An opt-in IEC A-weighting pre-emphasis filter is available via
`--preemph-filter aweight`.

## Evaluation

- [`eval/wdf_vs_spice/`](eval/wdf_vs_spice/README.md) — validates the WDF solver
  against SPICE (ESR, NRMSE, Welch FR coherence).
- [`eval/acme_vs_spice/`](eval/acme_vs_spice/README.md) — ACME.jl circuit-model
  feasibility spike vs SPICE.
- [`eval/nam_vs_acme/`](eval/nam_vs_acme/README.md) — 3-way scoring of the NAM /
  Neural Net export against ACME + SPICE references.

## Repository map

| Path | Purpose |
|------|---------|
| `plugin/` | Rust VST3 plugin (nih-plug): `lib.rs`, `wavenet.rs` (active NN), `model.rs` (dormant LSTM), `wdf/` (solver), `eq.rs`, `editor.rs`, `dsp.rs`, weights |
| `xtask/` | nih-plug build runner (`cargo xtask bundle ba283`) |
| `train/` | PyTorch training pipeline (`train.py`, `model.py`, `export.py`) |
| `data_gen/` | SPICE dataset generation (PySpice + ngspice) |
| `eval/` | Validation harnesses (`wdf_vs_spice`, `acme_vs_spice`) |
| `colab/` | Google Colab training notebooks |
| `docs/` | Concept guide, runbook, design plans |
| `docs/adr/` | Architecture decision records (`0001` = adaptive oversampling) |
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
