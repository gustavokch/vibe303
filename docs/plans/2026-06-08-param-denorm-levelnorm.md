# Param de-norm + levelNorm (2026-06-08)

Two fixes to plugin parameter behaviour, plus a new `level_norm` toggle.

## 1. Output de-norm — resolves the #30 contract

The trained model declares (`plugin/weights/*.json` meta + `train/dataset.py`): `input_scale = 2.0`,
`output_ref_v = 64.0`. The stimulus convention (`data_gen/stimulus.py`) is **0 dBFS ≡ 0.5 V** at the
circuit input (dry block peak-normalized to 0.5 V).

Working both stages through:

- **Input — already correct, left untouched.** `model_in = x · (0.5 V/dBFS) · input_scale(2.0) = x`.
  The `input_scale` exactly cancels the 0.5 V/dBFS convention, so the NeuralNet input is fed `x`
  directly. We must **not** pre-multiply by `input_scale` (that would double the drive).
- **Output — was missing, now applied.** `out_dBFS = model_out · output_ref_v / 0.5 = model_out · 128`.
  Previously only `out_gain` was applied, leaving the output ≈42 dB too quiet.

Constant: `dsp::INPUT_FS_VOLTS = 0.5`. NeuralNet wet `= forward · output_ref_v / INPUT_FS_VOLTS`. WDF
is fed/observed in real volts: input `driven · INPUT_FS_VOLTS`, output `wdf_out / INPUT_FS_VOLTS`.
Both backends reference the same 0 dBFS ≡ 0.5 V scale and are level-matched. The de-norm lives in the
shared `ChannelDsp` so the plugin and the offline standalone stay identical.

**Default is faithful (hot).** The BA283's real voltage gain (~+50 dB at the secondary) means a
0 dBFS input clips a DAW. This is authentic; tame it with `output_db` (down to −24 dB) or `levelNorm`.
The `--raw` standalone path and the WDF golden test bypass `ChannelDsp` and are unaffected.

## 2. levelNorm — RMS match output to input

`level_norm: BoolParam` (default **off**). When on, `plugin/src/levelnorm.rs::LevelNorm` scales the
wet signal so its RMS tracks the input RMS:

- Stereo-linked: one gain from the summed channel energy, applied to both (stereo image preserved).
  Owned by the plugin; per frame it reads the gain, processes both channels, then updates the
  estimators from each channel's pre-norm `ChannelDsp::last_wet`.
- Feed-forward `g = sqrt(in_ms / wet_ms)` from one-pole mean-square envelopes (τ ≈ 75 ms), clamped
  −80 dB..+48 dB, gain-slewed, held toward unity below an input energy floor. Unconditionally stable;
  zero-alloc. Applied **before** `output_db`/`mix` (they remain user trim).
- Always updated (even when off) so toggling on is seamless. Settle time ≈ 1 s when ducking the
  ~+50 dB faithful output; steady-state matches input to ±0 dB.

Mirrored in the offline standalone via `--level-norm`.

## 3. Zipper fix — WDF block params glide

`circuit_gain` and the WDF `bias` are block-rate (each change rebuilds `G0`). The plugin now glides
them per block (one-pole, τ ≈ 20 ms, snap within an epsilon) so a fast knob drag becomes small
per-block steps that re-settle cleanly instead of stepping. NeuralNet `drive`/`bias`/`output`/`mix`
were already per-sample smoothed.

## Flagged, out of scope

- **Bias interpolation between trained gain points** is a training-data-density issue (FiLM
  interpolates continuously; fidelity between discrete points is model-limited). Plugin domain (0..1)
  and smoothing are already correct. Follow-up: retrain with a denser gain grid.
- **circuit_gain inert on the NeuralNet backend** is structural (the NN was not trained on R_KJ) and
  cannot be hidden in the generic/headless host UI without a custom editor.

## Verification

`cargo test -p ba283` (42 passed, 3 ignored — incl. `nn_output_is_denormed_by_output_ref_over_input_fs`,
`test_levelnorm` convergence/silence/clamp, `level_norm` default-off). Standalone smoke: faithful
output is hot; `--level-norm` steady-state matches input RMS to ±0 dB. Zero-alloc tests still green.

### Ground-truth A/B — `eval/plugin_vs_acme/`

Nails the ×128 constant and levelNorm against circuit ground truth from ACME.jl (which tracks SPICE
to ≈0.02 dB level / −30 dB ESR). Per (stim, gain): ACME wet volts vs `plugin_volts = out_dBFS × 0.5`
(undoing the dBFS step to expose `model_out × 64`). On `sweep_m3` across the 5 trained gains with the
real 48-wide model:

- **×128 confirmed** — on the model-faithful gains (ESR ≤ −10 dB: g0.25/0.75/1.0) the level lands at
  +1.66 / +2.29 / −2.33 dB vs ACME volts (median +1.66, within ±3 dB and **not** at −6 dB = ×64 or
  +6 dB = ×256). The residual is the model's magnitude error.
- **levelNorm confirmed** — output dBFS RMS matches input to within 0.21 dB across all gains
  (continuous material; bursty pink shows several dB because a 75 ms RMS matcher cannot track bursts).
- Surfaced a **model** gap (not a plugin gap): the 48-model collapses at min gain (g0.0, near-silent)
  and is weak at g0.5 — a training-coverage issue (see [[ba283-output-normalization]]).

**WDF backend** (`--backend wdf`, fed physical `R_KJ(gain)` + RV1 0.77 = identical parameterisation
to ACME) is the decisive de-norm proof — no learned magnitude error, so `plugin_volts = wdf_volts`:
across all 5 gains the level lands at **−0.02 / −0.02 / −0.01 / +0.01 / +0.07 dB** vs ACME volts
(median −0.01, peaks match <1%), ESR −23 dB, levelNorm −0.14…−0.21 dB, every point circuit-faithful
(no collapse). This confirms the volts↔dBFS scaling (`×0.5` in / `÷0.5` out) is exact and that the
WDF backend tracks the circuit across the full gain range.

These required two enabling changes:
- `model.rs` generalized to a runtime hidden width ≤ `HIDDEN_MAX` (64) so the 48-wide trained export
  loads alongside the embedded 32-wide placeholder; all golden/noalloc tests stay green (still
  zero-alloc — fixed `[f32; HIDDEN_MAX]` storage, loops bounded by `self.hidden`).
- `ba283_standalone --weights <path>` loads a weights JSON at runtime (via `Ba283Model::from_json_path`
  + `ChannelDsp::set_model`) so the A/B uses the real trained model without rebuilding the embedded one.
