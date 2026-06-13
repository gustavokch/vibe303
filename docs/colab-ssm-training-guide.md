# Training the BA283 SSM models in Colab — step-by-step

Notebook: [`colab/ba283_ssm_training.ipynb`](../colab/ba283_ssm_training.ipynb). Trains the
LSTM baseline + the S4D / S6 (Mamba-style) state-space stack on the gain-conditioned SPICE
dataset, then scores every model apples-to-apples against the **ACME-vs-SPICE fidelity floor**
on held-out-gain ESR.

## Before you start (prerequisites)

- **GPU runtime** — Runtime → Change runtime type → T4 GPU. Step 3 asserts CUDA and hard-fails
  without it.
- **`rclone.conf`** — a local rclone config with a remote pointing at the Drive that holds the
  dataset. You upload it in step 2.
- **GitHub token** — a PAT with read access to the private `gustavokch/ba283-vst3` repo. Pasted
  in step 3.
- **Dataset on Drive** — `<remote>:ba283_ablation/full9pt` must exist (9-point gain set, `train`
  + `val` manifests, 44.1 kHz). This is the gain-conditioned SPICE wet target. Results are
  written under `<remote>:ba283_ssm/`.

## Run order (cells top → bottom)

1. **Open the notebook** in Colab from the repo (or upload the `.ipynb`).

2. **Install rclone + upload conf.** Runs the install, then prompts a file upload — pick your
   `rclone.conf`. Prints `REMOTE = <first remote>:`. Confirm it is the remote you expect (it
   takes the first one `rclone listremotes` returns).

3. **Clone repo + deps + GPU check.** Prompts `GitHub token:` — paste your PAT. Clones the repo,
   runs `git pull`, pip-installs `soundfile pandas tensorboard scipy`, and prints the GPU name.
   The SSMs are pure-PyTorch, so no `mamba-ssm` / CUDA-kernel dependency.

4. **Config.** No edit needed for a default run. Defines:
   - `EPOCHS=60`, `PATIENCE=15`, `SEED=745603`, `TARGET=0.01` (~ −20 dB redesign goal).
   - `GAINS = {0, 0.25, 0.5, 0.75, 1.0}` — the *trained* gains. The 4 values between them are
     held out and drive the honest gain-interpolation metric.
   - `CONFIGS` — `LSTM_REF, S4D_0, S4D_1, S6_0, S6_1`.
   Edit only to change paths, gains, or the model stack.

5. **Stage the gain set.** Pulls `full9pt` from Drive to `/content/full`, remaps the train/val
   manifests, and asserts SR = 44100. Prints the manifest paths. Run once per session.

6. **Train** (the main step). Form fields:
   - `SELECTED_CONFIG_KEYS_STR` — `ALL` for the whole stack, or a comma list (e.g.
     `S6_0,S6_1`).
   - `RUN_HPO` — `False` for a plain stack run; `True` runs a GPU-only random hyperparameter
     search over `HPO_MODEL`.
   - `WINDOW=2048` — the recurrent SSM scan is a Python loop, so keep this modest.

   Each config trains via `python -m train.train …`, saves `ckpt_<key>/best.pth` +
   `logs_<key>/`, and backs them up to `<remote>:ba283_ssm/`. **Resumable** — re-running skips
   any config that already has a `best.pth`.

   HPO path (`RUN_HPO=True`): runs `HPO_TRIALS` random trials at `HPO_EPOCHS` each, ranks by
   held-out-gain ESR, writes `hpo_out/leaderboard.{json,md}`, then trains the best params to full
   `EPOCHS` as `<model>_best`.

7. **Score + verdict.** Form fields:
   - `EXISTING_PATH_RCLONE` — leave `""` normally. Set it to a remote subpath to pull
     pre-trained checkpoints (train on one runtime, score on another).
   - `MATCH_TOL_DB=3.0` — how close to the ACME floor counts as a ✅ match.

   Scores every config and prints a verdict table against the **ACME-vs-SPICE floor**
   (≈ −31.6 dB, read live from `eval/acme_vs_spice/out/summary.json`). The ✅ column is judged on
   **held-out-gain ESR** within `MATCH_TOL_DB` of the floor; `val_esr` is shown for reference but
   is stem-overlap optimistic — do not rank on it. ΔdB < 0 means tighter to SPICE than the ACME
   analytical model. Saves `ssm_save/ssm_results.json` + `ssm_table.md` to
   `<remote>:ba283_ssm/results`.

## Gotchas

- **Disconnect mid-train** — re-run steps 2 → 6. Backed-up checkpoints skip; finished configs are
  not retrained. Alternatively pull them with `EXISTING_PATH_RCLONE` in step 7.
- **No CUDA** — step 3 hard-fails; fix the runtime type first.
- **SSM is slow** — the Python-loop scan is slower than cuDNN LSTM. Lower `WINDOW` / `HPO_EPOCHS`
  if you hit the Colab time limit.
- **Wrong remote** — step 2 picks the first remote alphabetically. With multiple remotes, check
  the printed `REMOTE =` line.
- **Scope** — export / Rust inference is out of scope here (LSTM-only schema). An SSM win feeds a
  separate `ba283-ssm-v1` export + Rust kernel effort.
