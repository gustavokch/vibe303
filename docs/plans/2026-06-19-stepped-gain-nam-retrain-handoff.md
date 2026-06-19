# Stepped-Gain NAM Retrain — Handoff (WS4)

Date: 2026-06-19
Status: pipeline STAGED (code/config only). No training, SPICE, or data-gen run
in this branch. This doc is the runbook for the offline retrain.

## Decisions needed BEFORE starting (resolve open items 1–4 below)

1. **Where to apply `wet /= G_i`** (resample-time hook) — item 1.
2. **Stepped-eval SPICE target filenames** — item 2.
3. **G_i drift tolerance** vs the embedded WDF makeup (`makeup_ref`) — item 3.
4. **NamModel cond-channel format** for stepped (scalar index vs one-hot) — item 4.

## Execution checklist (phases)

- [ ] **P0 — decisions** above resolved; free disk / RAM (ngspice 2.1 GB ceiling).
- [ ] **P1 — dry sweep**: `data_gen.stimulus --amplitude-sweep` → dry set. Verify
      amplitude grid + short clips.
- [ ] **P2 — SPICE render + G_i**: `run_spice --stepped` → wet_raw + `g_i.json`.
      **GATE:** measured `makeup` ≈ `makeup_ref` (embedded WDF `1/G_i`) within the
      chosen tolerance — else netlist drifted from WDF; STOP and reconcile.
- [ ] **P3 — resample + normalize**: apply `wet /= G_i`; build manifest; assert
      dry SR == wet SR.
- [ ] **P4 — train**: `train.train --conditioning stepped` (lstm 3×18). Watch
      val_esr; learned codebook starts at identity.
- [ ] **P5 — export**: stepped `film_codebook` + `rkj_ohms` + `output_ref_v`.
- [ ] **P6 — eval**: `compare --stepped --hot-amps 1,4,16`. **GATE:** worst
      NAM-vs-SPICE ESR comparable to prior continuous (~−28..−30 dB).
- [ ] **P7 — ship**: embed weights in `plugin/`, flip the Rust NN cond path from
      the staged continuous map to the discrete 5-entry codebook lookup
      (`wavenet.rs` / `lib.rs`), re-run `cargo test`, re-bundle, DAW A/B.

> NOTE: P7 is the only remaining Rust change — it is intentionally NOT in this
> branch (no trained weights yet). Everything before P7 is offline.

## Why

The Rust plugin's `circuit_gain` is now a **5-position hardware switch**, not a
continuous knob. Each detent is a discrete R_KJ resistor that sets the BA283's
closed-loop gain; `levelNorm` holds the output at unity so the audible effect is
the *saturation character* at that gain, not loudness. The NAM model must be
retrained to learn the BA283 saturation at exactly these 5 discrete, level-
normalized positions — valid across the input-amplitude range that the plugin's
`input_gain × drive` can produce.

The previous model was gain-CONDITIONED on a continuous `gain_norm ∈ [0,1]`
(FiLM-linear). That path is kept intact; stepped is **additive**.

## The 5 positions

| idx | R_KJ (Ω) | nominal gain (balanced) | WDF makeup = 1/G_i |
|-----|----------|--------------------------|--------------------|
| 0   | 1.0e9 (open sentinel) | 15 dB | 0.202892 |
| 1   | 1200     | 20 dB | 0.118732 |
| 2   | 470      | 25 dB | 0.073245 |
| 3   | 220      | 30 dB | 0.043799 |
| 4   | 110      | 35 dB | 0.026136 |

- R_KJ table: `data_gen.netlist.STEPPED_RKJ_OHMS` and
  `train.dataset.STEPPED_RKJ_OHMS` (kept in sync by value; mirror of the plugin).
- The makeup column is the **embedded WDF makeup table** (`1 / G_i`), recorded in
  `data_gen.netlist.STEPPED_MAKEUP`. `G_i` is the per-position small-signal output
  scale (secondary volts per input volt). WDF and NN must share this reference.
- idx 0's R_KJ is the "open" sentinel (`1e9 Ω`), matching the plugin's open
  detent — effectively no K↔J shunt, lowest gain.

## Data-gen recipe

### 1. Dry input-amplitude sweep

`data_gen/stimulus.py --amplitude-sweep` writes a per-amplitude dry set
(`build_amplitude_sweep`). One broadband base (log sweep + two-tone IMD,
peak-normalized to 1.0) is scaled to each amplitude in `STEPPED_AMP_DB`
= `(-24, -12, -6, 0, 6, 12, 24, 36, 48)` dB relative to the
`0 dBFS ≡ 0.5 V` SPICE convention. So `+48 dB` peaks at `0.5 · 10^(48/20) ≈ 126 V`
at the netlist input (far into clipping — intentional; the model must cover hot
drive, and level-norm pulls it back to unity).

Clips are SHORT (default 8 s, `--amp-duration-s`) because:
- ngspice has a ~2.1 GB output-buffer ceiling; hot/clipping points densify the
  gear timestep and pile up the in-RAM buffer.
- `run_spice` runs at os=1 (`OVERSAMPLE = 2`) and `.save v(SEC_IN)` only (the
  ~40× buffer cut is already in `simulate_one`).

### 2. SPICE render at the 5 positions + G_i record

`data_gen/run_spice.py --manifest <dry_dir> --stepped` →
`run_stepped_batch`:

1. For each position, `measure_gi(rkj)` runs a low-level 1 kHz probe
   (`_GI_PROBE_PEAK_V = 0.005 V`, well below clipping) and returns
   `rms(SEC_IN) / rms(input)` = the **linear** secondary gain `G_i`.
2. Writes `data/wet_raw/g_i.json`:
   `[{idx, rkj_ohms, g_i, makeup = 1/g_i, makeup_ref = STEPPED_MAKEUP[idx]}, ...]`.
   `makeup_ref` is the embedded WDF value for a **drift check** — if the measured
   `makeup` diverges from `makeup_ref`, the netlist has drifted from the WDF
   reference and the targets must NOT be silently re-scaled.
3. Delegates the wet renders to `run_gain_batch` at exactly `STEPPED_RKJ_OHMS`,
   fixed bias `BIAS_NORM_FIXED = 0.77`. Wet WAVs are written RAW (volts).

### 3. Resample + normalize by G_i

`data_gen/resample.py --raw-dir data/wet_raw --out-dir <wet> --manifest <csv>
--no-split` (single-stimulus / few-stimulus stepped sets use `--no-split` and a
caller-side temporal split, per the existing convention). Asserts dry SR == wet
SR (enforced by `write_aligned_dry`'s resample).

The wet target for position `i` must be divided by `G_i` (from `g_i.json`) so the
NN trains in a per-position level-normalized domain — the SAME normalization the
plugin's level-norm + WDF makeup table (`1/G_i`) applies. **This division is the
one manual hook to wire when the offline retrain runs** (apply `wet /= G_i` at
resample time keyed on the `gain_value`/R_KJ in the filename; `g_i.json` carries
the map). The continuous-sweep path leaves wet raw and relies on `OUTPUT_REF_V`;
stepped additionally divides by G_i so each detent lands at unity small-signal.

`gain_value` in the manifest = the R_KJ ohms the target was rendered at (from the
`_gain<rkj>` filename). The dataset snaps it to the nearest of the 5 positions.

## The `stepped` conditioning mode

- `train/model.py`: `BA283LSTM(conditioning="stepped")`. The LSTM runs on AUDIO
  alone (input_size 1, like `film`). A learned **codebook** `film_codebook =
  nn.Embedding(5, 2H)` (zero-init → identity at start) holds one `(gamma, beta)`
  affine per position. Forward: `idx = round(x[...,1])`, look up the codebook,
  apply `out = dense((1 + gamma) * y + beta)`. `film`/`concat` are byte-for-byte
  unchanged.
- `train/dataset.py`: `BA283Dataset(..., conditioning="stepped")` emits the
  discrete class INDEX (0..4) in conditioning column `x[:,1]` instead of the
  continuous `gain_value`. The index is `nearest_stepped_index(gain_value)` —
  snap-to-grid in log-resistance (the gain axis is log-linear in R_KJ).
  `make_loader(..., conditioning=...)` forwards the mode; `train/train.py` passes
  it to both loaders + accepts `--conditioning stepped`.
- Only `model_type=lstm` supports stepped (the default). `train/ssm.py` (s4d/s6)
  still validates `film`/`concat` only — out of scope for this retrain.

## Export format

`train/export.py` (schema `ba283-lstm-v2`), when `conditioning == "stepped"`:

- Adds a `film_codebook` layer: `{ type, positions: 5, shapes: {weight:[5,2H]},
  weight: [[...2H...] × 5], rkj_ohms: STEPPED_RKJ_OHMS }`. Each row is
  `[gamma(0:H) | beta(H:2H)]`; the plugin indexes by switch position 0..4.
- NO `film` layer (the codebook IS the per-position map).
- Doc-level: `conditioning="stepped"`, `stepped_positions=5`,
  `stepped_rkj_ohms=STEPPED_RKJ_OHMS`, `stepped_equation`, and the single fixed
  output reference `output_ref_v` (= `OUTPUT_REF_V` = 64 V; model output is
  normalized, plugin recovers volts as `model_out * output_ref_v`, then level-
  norm + the `1/G_i` makeup table land each detent at unity).

## Eval changes

`eval/nam_vs_acme/compare.py --stepped` (additive; the continuous 9-gain sweep is
unchanged and default):

- Scores at the 5 `STEPPED_RKJ_OHMS` positions; the NAM conditioning channel
  carries the discrete index, ACME uses the matching R_KJ, SPICE target looked up
  as `T3K-60s_pos{idx}_amp{amp}.wav`.
- **Hot-input-amplitude check**: `--hot-amps` (default `1,4,16` linear input
  gains) drives each position into clipping. Same 3-way contract (level-matched
  ESR, 20 Hz HP, lag-align).
- Gate = WORST NAM-vs-SPICE ESR over every (position, amplitude) point. Writes
  `summary_stepped.json`.

## Exact commands (offline)

```bash
# 1. Dry input-amplitude sweep (96 kHz, short clips)
python -m data_gen.stimulus --amplitude-sweep --amp-duration-s 8 \
    --out data/dry_stepped --overwrite

# 2. SPICE render at the 5 positions + record G_i -> data/wet_raw/g_i.json
python -m data_gen.run_spice --manifest data/dry_stepped --stepped \
    --fixed-bias 0.77 --workers <N>

# 3. Resample wet to grid + manifest (apply wet /= G_i from g_i.json — see
#    "Resample + normalize by G_i"). --no-split for few-stimulus sets.
python -m data_gen.resample --raw-dir data/wet_raw \
    --out-dir data/wet_stepped --manifest data/stepped/manifest.csv \
    --dry-dir data/dry_stepped --no-split
#    (then a temporal split if needed: train.temporal_split / resample helper)

# 4. Train the stepped model
python -m train.train --model lstm --conditioning stepped \
    --manifest data/stepped/train.csv --val-manifest data/stepped/val.csv \
    --layers 3 --hidden 18   # (match the NAM-faithful 3x18 sizing)

# 5. Export (stepped codebook + rkj table + output_ref_v)
#    Use the project's existing export entrypoint with the trained checkpoint;
#    export_rtneural_json auto-detects conditioning="stepped" and emits the
#    film_codebook layer.

# 6. Eval: 5 discrete positions + hot-input-amplitude check
python -m eval.nam_vs_acme.compare --stepped --nam <stepped.nam> \
    --hot-amps 1,4,16 --bias 0.77 --out eval/nam_vs_acme/out_stepped
```

## Assumptions / open items (flagged)

1. **`wet /= G_i` application point.** `run_stepped_batch` MEASURES + RECORDS
   `G_i` (`g_i.json`) but writes wet WAVs raw (it delegates to `run_gain_batch`).
   The actual per-target division is left as a small resample-time hook to wire
   when the retrain runs, because resample.py's manifest builder is shared with
   the continuous path and I did not want to fork it destructively. The map is in
   `g_i.json` keyed by R_KJ/idx. **Confirm where you want the division applied.**
2. **SPICE target filenames for eval.** `_score_one_stepped` expects
   `T3K-60s_pos{idx}_amp{amp:g}.wav`. The stepped eval reamp set does not exist
   yet (offline); the naming is a proposal — adjust if the reamp step names them
   differently.
3. **G_i ≈ 1/makeup gate.** The embedded WDF makeup values are taken as the
   reference; `g_i.json` carries `makeup_ref` for a drift check but does NOT hard-
   fail on mismatch. Decide the tolerance / whether to fail the batch.
4. **NamModel conditioning channel for stepped.** The eval feeds the discrete
   index as the `gain` channel to `NamModel.run`. If the exported `.nam` expects
   the index differently (e.g. one-hot), adjust `run_nam.py` / the export.
5. **SSM (s4d/s6) not extended** to stepped — out of scope; LSTM/NAM only.
6. **`STEPPED_RKJ_OHMS` duplicated** in `data_gen.netlist` and `train.dataset`
   (by value, to avoid a train→data_gen import). Keep them in sync if edited.
