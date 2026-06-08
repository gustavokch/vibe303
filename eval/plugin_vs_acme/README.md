# Plugin vs ACME — nail the de-norm + validate levelNorm

Validates the Rust plugin's **de-norm contract** (both backends) and the
**levelNorm** toggle against circuit ground truth from the
[ACME.jl](../acme_vs_spice) model (which tracks SPICE to ≈0.02 dB level / −30 dB
ESR — see `eval/acme_vs_spice`).

Two backends via `--backend`:
- `nn` (default) — the NeuralNet chain; de-norm is the ×128 constant
  (`output_ref_v / INPUT_FS_VOLTS`). Conditioned by `--bias = gain_norm`; needs the
  trained weights (`--weights`).
- `wdf` — the circuit MNA solver; de-norm is the volts↔dBFS round-trip
  (`×0.5` in, `÷0.5` out). Fed the physical `--circuit-gain = R_KJ(gain)` ohms and
  `--bias = RV1 0.77`, i.e. **identical** parameterisation to the ACME reference,
  so it should match ACME tightly (both are circuit models of the same netlist) —
  no NN magnitude error in the loop.

## What it checks

The plugin de-norms its NN output by `output_ref_v / INPUT_FS_VOLTS` (= **×128**):
recover volts (`× output_ref_v = 64`), then express in the 0 dBFS ≡ 0.5 V
convention (`/ 0.5`). The harness undoes the dBFS step (`× 0.5`) to get
`plugin_volts = model_out × 64` and compares to ACME wet volts:

- **LEVEL** `rms_db = 20·log10(plugin_rms / acme_rms)`. ≈0 confirms ×128. A
  mistaken ×64 reads **−6 dB**, ×256 reads **+6 dB** — cleanly distinguished. The
  residual is bounded by model magnitude accuracy, so use on-grid gains and a
  stimulus the model handles well, and read the *median across points* (a
  constant-factor de-norm error is systematic; model error scatters).
- **levelNorm** — re-run with `--level-norm`; the steady-state output dBFS RMS
  must match the input dBFS RMS (≈0 dB). Use a **continuous** stimulus
  (`sweep_m3`, music): the 75 ms feed-forward RMS matcher cannot track
  **burst** signals (e.g. `pink_m3_s0`, which alternates ~10 dB every 0.5 s),
  so those show several dB of residual by design.

## Conditioning

The gain-conditioned model's 2nd input is the **gain knob** (`gain_value`), with
RV1 fixed at the training bias (0.77). So the plugin `--bias <gain_norm>` is the
gain conditioning, and the ACME ground truth is run at
`R_KJ = gain_norm_to_rkj(gain_norm)`, `bias = 0.77` — identical parameterisation.

## Weights

The embedded plugin weights are an **untrained placeholder**; this harness loads
the real Colab-trained 48-wide export via the standalone's `--weights` flag
(default `~/Downloads/ba283_lstm32.json`). `model.rs` supports hidden ≤ 64, so the
48-wide model loads without rebuilding the embedded 32-wide placeholder.

## Usage

```bash
cargo build -p ba283 --release --bin ba283_standalone
# NeuralNet (×128 de-norm):
uv run python -m eval.plugin_vs_acme.compare \
    --backend nn --stims sweep_m3 --gains 0.0,0.25,0.5,0.75,1.0 \
    [--weights ~/Downloads/ba283_lstm32.json] --out eval/plugin_vs_acme/out
# WDF circuit backend (volts round-trip de-norm):
uv run python -m eval.plugin_vs_acme.compare \
    --backend wdf --stims sweep_m3 --gains 0.0,0.25,0.5,0.75,1.0 \
    --out eval/plugin_vs_acme/out_wdf
```

Needs julia + ACME (see `eval/acme_vs_spice/README.md`) and the dry corpus at
`~/GoogleDrive/ba283_spice/dry/`. Outputs `summary.json` + waveform overlays.

## Files

- `run_plugin.py` — drives `ba283_standalone` (NN), handles the dBFS↔volts
  de-norm bookkeeping (`INPUT_FS_VOLTS = 0.5`), optional `--level-norm`.
- `compare.py` — per (stim, gain): ACME truth vs plugin volts (level + ESR) and
  the levelNorm steady-state match; writes `summary.json` + a verdict.
