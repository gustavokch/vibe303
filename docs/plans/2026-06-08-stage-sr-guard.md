# Plan — staging dry/wet sample-rate guard

## Goal

Make the 96 kHz-dry / 44.1 kHz-wet mismatch (the bug that silently collapsed
FULL9/C5 to zero-output, ESR≈1.0 — see `ba283-dry-resample-bug`) **fail loud at
stage time** instead of after a multi-hour training run. After
`stage_rclone(FULL='ba283_ablation/full9pt')` + `remap_manifests`, assert that
every checked dry/wet pair shares one sample rate and it equals the expected rate.

## Why a guard, not just a one-liner

The collapse was invisible: training ran to completion, produced a checkpoint,
and only the ESR number (1.0 flat) hinted at it — and that was misread as "the
wall." A pre-train assertion converts a silent multi-hour waste into an immediate,
named failure. The invariant is cheap (read a few WAV headers) and absolute
(dry SR must equal wet SR for the frame-indexed loader to pair windows in time).

## Approach

Reusable helper in `train/stage.py` (where `stage_rclone`/`remap_manifests` live),
called from the notebook stage cells. Helper + test, not an inline lambda, so the
subset notebook and any future build reuse the same check and it is covered.

### 1. `train/stage.py` — `assert_matched_sr()`

```python
def assert_matched_sr(manifest, expected_sr: int | None = 44100,
                      n_check: int = 12) -> int:
    """Assert staged dry/wet pairs share one sample rate (== expected_sr if given).

    The training loader (train.dataset.BA283Dataset) reads FRAME-indexed windows
    assuming dry and wet share a rate; a dry left at a different rate than its wet
    (e.g. 96 kHz SPICE-source dry vs 44.1 kHz wet) makes (dry[i], wet[i]) span
    different durations -> decorrelated -> the model collapses to zero output.
    This fails loud at stage time instead. Samples up to n_check rows spread
    across the manifest (head/middle/tail) so one bad stem surfaces without
    reading every file. Raises RuntimeError naming the offending pair.
    """
```

- Accept a CSV path or a DataFrame (notebook passes `MAN['train']`, a path).
- Read `sf.info(dry).samplerate` and `sf.info(wet).samplerate` per sampled row
  (resolve paths via the same `_resolve_path` the loader uses, or absolute).
- Sample `n_check` rows by even stride over the manifest (not just the head) so a
  single off-rate stem is likely caught; if `stim_kind` present, bias the sample
  to cover each kind once.
- Fail conditions (RuntimeError, name the file + rates):
  - `dry_sr != wet_sr` for any sampled pair, OR
  - `expected_sr is not None and dry_sr != expected_sr`.
- Return the confirmed shared rate on success (usable for logging / `sr_eval`).

### 2. Wire into the notebooks

- `colab/ba283_redesign_colab.ipynb` cell 8 (stage), after `remap_manifests`:
  ```python
  from train.stage import stage_rclone, remap_manifests, assert_matched_sr
  ...
  MAN = remap_manifests(DEST, names=('train','val'), out_dir=DEST)
  sr = assert_matched_sr(MAN['train'], expected_sr=44100)
  assert_matched_sr(MAN['val'], expected_sr=44100)
  print(f"sample-rate guard OK: dry==wet=={sr} Hz")
  ```
- `colab/ba283_ablation_colab.ipynb`: same guard after each subset stages, BUT
  `expected_sr` is per-subset (base44k=44100, c4_88k=88200, c5_9pt=44100). Pass
  the subset's known wet rate (already in `SUBSETS[...]`), or `expected_sr=None`
  to only enforce dry==wet and let the rate float per subset.

### 3. Test — `tests/train/test_stage.py`

- `test_assert_matched_sr_passes_when_equal`: write a tiny manifest of 44.1k
  dry+wet pairs → returns 44100, no raise.
- `test_assert_matched_sr_raises_on_mismatch`: 96k dry + 44.1k wet → RuntimeError
  matching the filename and both rates.
- `test_assert_matched_sr_enforces_expected`: matched 88.2k pair with
  `expected_sr=44100` → raises (rate-pinning path).

## Files to touch

- `train/stage.py` — add `assert_matched_sr`.
- `tests/train/test_stage.py` — 3 cases above.
- `colab/ba283_redesign_colab.ipynb` — cell 8 guard (2 lines).
- `colab/ba283_ablation_colab.ipynb` — per-subset guard (optional, same helper).

## Verification

- `uv run pytest tests/train/test_stage.py -q` green.
- Notebook code cells compile; the guard line references valid `MAN` keys.
- Negative proof: point the guard at the OLD broken full9pt layout (dry@96k) and
  confirm it raises — but the Drive data is already repaired, so this is a unit
  test (96k-dry fixture), not a live re-break.

## Out of scope

- Re-rendering / re-staging data (already repaired, PR #26).
- Any model/training change.
