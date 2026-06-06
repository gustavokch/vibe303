# Fidelity Ablation Matrix — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build the reusable harness (subset builder, fair-ESR/aliasing/held-out-gain metrics, target-floor check, results aggregator) and run the Core-4 OFAT ablation to localize the `val_esr ≈ 0.097` wall.

**Architecture:** New `eval/ablation/` package holds pure, unit-tested numpy metric + manifest utilities (no torch, no GPU). Training runs are orchestrated as runbook steps: train-only cells local CPU, re-gen cells on Colab GPU, each scored against a venue-matched baseline. Metric code is TDD'd; run orchestration is procedural with explicit decision gates.

**Tech Stack:** Python, numpy, scipy.signal (`resample_poly`), soundfile/wav IO, pytest, existing `train/` model+dataset, existing `eval/acme_vs_spice` & `eval/wdf_vs_spice` harnesses, ngspice (local re-gen), Colab notebook.

**Design ref:** `docs/plans/2026-06-06-ablation-matrix-design.md`

---

## Phase A — Reusable harness (TDD, local, no training)

### Task 1: ESR + AA-downsample-before-ESR

**Files:**
- Create: `eval/ablation/__init__.py` (empty)
- Create: `eval/ablation/metrics.py`
- Test: `tests/eval/ablation/test_metrics.py`

**Step 1: Write the failing test**

```python
import numpy as np
from eval.ablation.metrics import esr, esr_resampled

def test_esr_zero_on_identical():
    x = np.sin(np.linspace(0, 50, 4096)).astype(np.float64)
    assert esr(x, x) == 0.0

def test_esr_is_error_over_signal_energy():
    target = np.array([1.0, -1.0, 1.0, -1.0])
    pred = np.zeros_like(target)
    # error energy == signal energy -> 1.0
    assert abs(esr(pred, target) - 1.0) < 1e-12

def test_esr_resampled_downsamples_pred_before_scoring():
    # 88.2k pred vs 44.1k target: pred decimated by 2 must align in length
    sr_hi, sr_lo = 88200, 44100
    t_lo = np.sin(np.linspace(0, 20, sr_lo)).astype(np.float64)
    t_hi = np.sin(np.linspace(0, 20, sr_hi)).astype(np.float64)  # same signal, 2x rate
    val = esr_resampled(t_hi, t_lo, sr_from=sr_hi, sr_to=sr_lo)
    assert val < 1e-3  # near-perfect after matched-rate AA downsample
```

**Step 2: Run to verify it fails**

Run: `uv run pytest tests/eval/ablation/test_metrics.py -v`
Expected: FAIL — `ModuleNotFoundError: eval.ablation.metrics`

**Step 3: Minimal implementation**

```python
# eval/ablation/metrics.py
import numpy as np
from scipy.signal import resample_poly

def esr(pred: np.ndarray, target: np.ndarray, eps: float = 1e-12) -> float:
    pred = np.asarray(pred, dtype=np.float64)
    target = np.asarray(target, dtype=np.float64)
    n = min(len(pred), len(target))
    pred, target = pred[:n], target[:n]
    return float(((pred - target) ** 2).sum() / ((target ** 2).sum() + eps))

def esr_resampled(pred, target, *, sr_from: int, sr_to: int, eps: float = 1e-12) -> float:
    """ESR after resampling pred from sr_from to sr_to with a polyphase AA filter.
    Always score at sr_to so 88.2k and 44.1k models compare apples-to-apples."""
    pred = np.asarray(pred, dtype=np.float64)
    if sr_from != sr_to:
        from math import gcd
        g = gcd(sr_from, sr_to)
        pred = resample_poly(pred, sr_to // g, sr_from // g)
    return esr(pred, target, eps=eps)
```

**Step 4: Run to verify it passes**

Run: `uv run pytest tests/eval/ablation/test_metrics.py -v`
Expected: PASS (create `tests/eval/ablation/__init__.py` if collection needs it)

**Step 5: Commit**

```bash
git add eval/ablation/__init__.py eval/ablation/metrics.py tests/eval/ablation/
git commit -m "feat(ablation): ESR + AA-downsample-before-ESR metric"
```

---

### Task 2: Aliasing metric (high-band spectral error vs gold)

**Files:**
- Modify: `eval/ablation/metrics.py`
- Test: `tests/eval/ablation/test_metrics.py`

**Step 1: Write the failing test**

```python
from eval.ablation.metrics import high_band_error

def test_high_band_error_zero_on_identical():
    x = np.random.default_rng(0xBA283).standard_normal(44100)
    assert high_band_error(x, x, sr=44100) < 1e-9

def test_high_band_error_flags_added_hf_alias():
    sr = 44100
    base = np.sin(2*np.pi*1000*np.arange(sr)/sr)
    nyq = sr/2
    alias = base + 0.3*np.sin(2*np.pi*(0.8*nyq)*np.arange(sr)/sr)  # strong HF content
    # error concentrated above 0.5*Nyquist -> nonzero
    assert high_band_error(alias, base, sr=sr) > 1e-3
```

**Step 2: Run** — FAIL (`high_band_error` undefined)

**Step 3: Implementation**

```python
def high_band_error(pred, target, *, sr: int, split_frac: float = 0.5, eps: float = 1e-12) -> float:
    """Relative error energy ABOVE split_frac * Nyquist (where folded aliases land)."""
    pred = np.asarray(pred, dtype=np.float64)
    target = np.asarray(target, dtype=np.float64)
    n = min(len(pred), len(target))
    P = np.fft.rfft(pred[:n]); T = np.fft.rfft(target[:n])
    freqs = np.fft.rfftfreq(n, 1.0 / sr)
    hi = freqs >= split_frac * (sr / 2)
    return float((np.abs(P[hi] - T[hi]) ** 2).sum() / ((np.abs(T[hi]) ** 2).sum() + eps))
```

**Step 4: Run** — PASS

**Step 5: Commit**

```bash
git add -A && git commit -m "feat(ablation): high-band aliasing error metric"
```

---

### Task 3: Per-gain + held-out-gain ESR aggregation

**Files:**
- Create: `eval/ablation/gain_eval.py`
- Test: `tests/eval/ablation/test_gain_eval.py`

**Step 1: Failing test** — feed a fake `predict(dry, gain)->wet` callable and a list of
`(dry, gain, wet_target)` rows; assert the aggregator returns one ESR per gain and a
held-out subset mean.

```python
import numpy as np
from eval.ablation.gain_eval import per_gain_esr, held_out_gain_esr

def _identity_predict(dry, gain): return dry  # perfect on identity targets

def test_per_gain_esr_groups_by_gain():
    rng = np.random.default_rng(1)
    rows = [(rng.standard_normal(2048), g, None) for g in (0.0, 0.5, 1.0)]
    rows = [(d, g, d) for (d, g, _) in rows]  # target == dry
    out = per_gain_esr(_identity_predict, rows)
    assert set(out) == {0.0, 0.5, 1.0}
    assert all(v < 1e-9 for v in out.values())

def test_held_out_gain_esr_filters_to_unseen_gains():
    rng = np.random.default_rng(2)
    rows = [(d, g, d) for d, g in
            [(rng.standard_normal(2048), g) for g in (0.125, 0.375, 0.5)]]
    mean = held_out_gain_esr(_identity_predict, rows, trained_gains={0.0, 0.5, 1.0})
    assert mean < 1e-9  # only .125/.375 counted, both perfect here
```

**Step 2: Run** — FAIL

**Step 3: Implement** `per_gain_esr` (group rows by gain, mean ESR per gain) and
`held_out_gain_esr` (filter rows whose gain ∉ trained_gains, return mean ESR), both
delegating to `metrics.esr`.

**Step 4: Run** — PASS

**Step 5: Commit** `feat(ablation): per-gain and held-out-gain ESR aggregation`

---

### Task 4: Subset manifest builder

**Files:**
- Create: `eval/ablation/subset.py`
- Test: `tests/eval/ablation/test_subset.py`

**Step 1: Failing test** — given a manifest dataframe with columns
`dry_path,wet_path,gain_value,duration_s,stim_kind`, `build_subset(df, kinds, max_trim_s)`
keeps **all gains** for the requested representative `stim_kind`s and clamps `duration_s`.

```python
from eval.ablation.subset import build_subset

def test_subset_keeps_all_gains_for_selected_kinds():
    import pandas as pd
    rows = [(f"d{k}", f"w{k}_{g}", g, 60.0, k)
            for k in ("music","pink_burst","sweep","multisine","two_tone")
            for g in (0.0,0.25,0.5,0.75,1.0)]
    df = pd.DataFrame(rows, columns=["dry_path","wet_path","gain_value","duration_s","stim_kind"])
    sub = build_subset(df, kinds=("music","pink_burst","sweep","multisine"), max_trim_s=20.0)
    assert set(sub["stim_kind"]) == {"music","pink_burst","sweep","multisine"}
    assert set(sub["gain_value"]) == {0.0,0.25,0.5,0.75,1.0}
    assert (sub["duration_s"] <= 20.0).all()
    assert "two_tone" not in set(sub["stim_kind"])
```

**Step 2–4:** implement, run FAIL→PASS.

**Step 5: Commit** `feat(ablation): representative-subset manifest builder`

---

### Task 5: Target-floor check (C7) — orchestration wrapper

**Files:**
- Create: `eval/ablation/target_floor.py`
- Test: `tests/eval/ablation/test_target_floor.py` (unit-test the aggregation only; the
  ACME/WDF subprocess calls are integration, run by hand)

**Step 1: Failing test** — `summarize_floor(esrs: dict)` returns median per model and a
verdict flag `target_bound` when any model's median ESR ≥ `threshold`.

```python
from eval.ablation.target_floor import summarize_floor

def test_summarize_floor_flags_target_bound():
    out = summarize_floor({"acme": [0.09, 0.10], "wdf": [0.11, 0.12]}, threshold=0.05)
    assert out["acme_median"] == 0.095
    assert out["target_bound"] is True

def test_summarize_floor_clears_when_below_threshold():
    out = summarize_floor({"acme": [0.001, 0.002], "wdf": [0.003]}, threshold=0.05)
    assert out["target_bound"] is False
```

**Step 2–4:** implement (numpy median + flag), FAIL→PASS.

**Step 5:** Add a thin CLI `python -m eval.ablation.target_floor --subset <manifest>` that
invokes the existing `eval/acme_vs_spice/compare.py` and `eval/wdf_vs_spice/compare.py`
on the subset wet renders and prints the floor summary JSON. Commit
`feat(ablation): target-floor (C7) summary + CLI`.

---

### Task 6: Results aggregator + verdict table

**Files:**
- Create: `eval/ablation/report.py`
- Test: `tests/eval/ablation/test_report.py`

**Step 1: Failing test** — `delta_db(cell_esr, baseline_esr)` returns `10*log10(cell/base)`
and `rank_cells(rows)` sorts by improvement; a cell "explains" the gap when
`improvement_db >= 3` OR it closes ≥50% of `baseline→target`.

```python
import math
from eval.ablation.report import delta_db, explains_gap

def test_delta_db_halving_is_minus_3():
    assert abs(delta_db(0.05, 0.10) - (-3.01)) < 0.05

def test_explains_gap_on_3db_improvement():
    assert explains_gap(cell=0.05, baseline=0.10, target=0.01) is True

def test_does_not_explain_small_improvement():
    assert explains_gap(cell=0.092, baseline=0.097, target=0.01) is False
```

**Step 2–4:** implement, FAIL→PASS.

**Step 5:** add `render_table(cells)->markdown` producing the deliverable table
(`cell, val_esr, ΔdB, per-gain, held-out-gain, notes`). Commit
`feat(ablation): results aggregator + verdict table`.

**Gate:** `uv run pytest tests/eval/ablation/ -v` all green before any training run.

---

## Phase B — ngspice re-gen for re-gen cells (local)

### Task 7: 88.2 kHz subset render (C4)

**Files:**
- Modify: `data_gen/run_spice.py` (verify `SAMPLE_RATE` / oversample is parameterizable;
  if hardcoded, add `--sample-rate` flag — see `data_gen/run_spice.py` SR constant)
- Runbook only (no unit test for the SPICE batch; it is covered by
  `tests/data_gen/test_run_spice.py` smoke)

**Steps:**
1. Confirm the 8×-gold path: render the subset stems at 352.8 kHz (gold) and at 88.2 kHz
   (C4) and 44.1 kHz (B0), all 5 gains. Command pattern:
   `uv run python -m data_gen.run_spice --stims <subset> --gains 0.0,0.25,0.5,0.75,1.0 --sample-rate 88200 --out data/ablation/c4/`
2. Resample gold→44.1k via `eval.ablation.metrics.esr_resampled` helper / `resample_poly(...,1,8)`.
3. Verify file counts = `len(subset_stims) * 5`. Commit the render manifest (not the wav blobs — respect `.gitignore`).

Commit: `chore(ablation): C4 88.2k + gold subset renders (manifest)`

### Task 8: 9-point gain-grid subset render (C5)

**Steps:**
1. Render subset stems at 44.1 kHz across **9** gains
   `--gains 0.0,0.125,0.25,0.375,0.5,0.625,0.75,0.875,1.0`.
2. Split: **train gains** = {0,.25,.5,.75,1}, **held-out gains** = {.125,.375,.625,.875}
   → two manifests via `eval.ablation.subset` + a gain filter.
3. Commit manifests. `chore(ablation): C5 9-pt gain subset + held-out split`

---

## Phase C — Training runs (procedural, with gates)

> Each cell: fixed seed `0xBA283`, epochs 60, patience 15, identical val stems & subset
> size within a venue. Reproduce the venue baseline FIRST.

### Task 9: B0_local + C1 (local CPU)

**Steps:**
1. **B0_local** — train current config on the subset:
   `uv run python -m train.train --train-manifest data/ablation/subset_train.csv --val-manifest data/ablation/subset_val.csv --hidden 32 --conditioning film --epochs 60 --patience 15 --seed 745603`
   Record `best val_esr`. **Gate:** sanity-check it lands in the ~0.09–0.11 band
   (subset will not perfectly match full-data 0.097; it is the *reference*, not a target).
2. **C1** — identical but `--hidden 64`. Record `best val_esr`.
3. Score both with `eval.ablation` (overall, per-gain, directional held-out-gain).
4. Commit results row JSON: `chore(ablation): C1 capacity result`.

### Task 10: B0_colab + C4 + C5 (Colab GPU)

**Steps:**
1. Upload C4/C5 subset renders + manifests to gdrive.
2. In the Colab ablation notebook (adapt `train/ba283_train_colab.ipynb`):
   - **B0_colab** — current config on the 44.1k subset. Reference.
   - **C4** — train on 88.2k subset; eval with `esr_resampled(sr_from=88200, sr_to=44100)`;
     also compute `high_band_error` vs the 44.1k gold for B0 and C4.
   - **C5** — train on 9-pt train-gain manifest; eval overall **and**
     `held_out_gain_esr(..., trained_gains={0,.25,.5,.75,1})` on the held-out manifest.
3. Download result JSONs.
4. Commit: `chore(ablation): C4 aliasing + C5 gain-density results`.

### Task 11: C7 target-floor + aggregation + verdict

**Steps:**
1. Run `python -m eval.ablation.target_floor --subset data/ablation/subset_train.csv`.
   **Gate:** if `target_bound is True` (ACME/WDF vs SPICE median ≳ 0.05), STOP — the wall
   is the SPICE target. Redesign starts at data-gen, not the model. Record and surface.
2. Run `eval.ablation.report.render_table` over all result JSONs → deliverable table.
3. Re-run the **winning axis** on 2–3 seeds to confirm signal > seed noise.
4. Write a one-paragraph verdict naming the wall into
   `docs/plans/2026-06-06-ablation-results.md`. Commit.

**Terminal deliverable:** the verdict feeds the full FiLM+GRU redesign plan (separate
brainstorming/writing-plans cycle).

---

## Notes
- DRY: all ESR flows through `eval.ablation.metrics.esr`; never re-derive inline.
- YAGNI: round-2 cells (C2 LSTM, C3 α0.95, C6 mix) are NOT built until Core-4 fails to
  localize the wall.
- Respect `.gitignore`: commit manifests + result JSONs, never wav/npz blobs.
- The metric harness (Phase A) is venue-independent and the only TDD'd code; runs are
  procedural by nature.

---

## Known follow-ups (post Phase-A review)

Phase-A harness (`eval/ablation/`) is built and green (34 tests). One tracked
integration gap remains before the **C7 target-floor CLI** can run end-to-end:

- `eval/ablation/target_floor.py` `_main` shells out to
  `eval/acme_vs_spice/compare.py` and `eval/wdf_vs_spice/compare.py` assuming a
  `--manifest`/`--emit-esr` JSON contract. Those harnesses (separate `spike/*`
  branch work) do **not** yet expose that CLI: ACME's `compare.py` is absent on
  this branch and WDF's uses `--stims/--gains/--bias/--out` and prints human
  tables. Before the first real C7 run, add an `--emit-esr` JSON mode to each
  harness (or write a thin adapter) and wire the actual flags. The pure core
  (`summarize_floor`) is tested and correct; only the subprocess glue is pending.

The Phase-A library itself is mergeable independently of this glue.
