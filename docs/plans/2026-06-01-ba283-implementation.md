# BA283 Neural Preamp Emulation — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Ship a real-time VST3 plugin that emulates the Neve BA283 mic preamp via a small GRU trained on SPICE-derived audio, with a user-controllable analog bias parameter, achieving a ≤ −25 dB perceptual null vs SPICE ground truth.

**Architecture:** Three-stage pipeline. Python + PySpice/ngspice generates 96 kHz wet audio targets by running continuous transient simulations of the BA283 netlist across a sweep of RV1 bias points. PyTorch trains a `GRU(input_size=2, hidden=32)` followed by `Linear(32, 1)`, where the second input dimension is the bias conditioning scalar, with ESR+MSE loss and TBPTT. Rust + nih_plug + rtneural-rs hosts the trained weights in a zero-alloc, dual-mono VST3 with internal 2× polyphase oversampling when host SR < 96 kHz.

**Tech Stack:**
- Python 3.12, uv, PySpice 1.5+, ngspice 42+, numpy, scipy, soundfile, torch (CPU), pytest, ruff
- Rust stable (1.80+), nih_plug, rtneural-rs, serde_json, atomic_float, assert_no_alloc
- ngspice CLI installed system-wide; Linux x86_64-v3 build target

**Source-of-truth design:** [`docs/plans/2026-06-01-ba283-design.md`](2026-06-01-ba283-design.md)

---

## Conventions Used in This Plan

- **TDD always:** every functional step has a failing test, a passing test, and a commit. Audio numerical code uses tiny fixture inputs (10–1000 samples, sine/impulse) so assertions stay deterministic and fast.
- **File paths are exact** and relative to repo root `/home/gustavo/Git/ba283-vst3`.
- **Commit messages** follow Conventional Commits: `feat(scope): …`, `test(scope): …`, `chore(scope): …`, `refactor(scope): …`.
- **Run commands as written.** If a command differs from "Run:" in this plan, stop and ask before improvising.
- **Never advance past a phase gate** (see Phase summaries) — if a gate fails, back up and fix.
- **Skills to use:** @superpowers:test-driven-development (every functional task), @superpowers:systematic-debugging (whenever a test fails unexpectedly), @superpowers:verification-before-completion (before declaring a phase done).

---

## Phase Gates

| Gate | Pass criterion |
|------|----------------|
| **Gate 1** (Phase 1 done) | `pytest tests/data_gen/ -v` green AND `python -m data_gen.run_spice --smoke` renders one (stim × bias) pair without ngspice non-convergence |
| **Gate 2** (Phase 2 done) | `pytest tests/train/ -v` green AND `python -m train.train --epochs 1 --smoke` runs to completion AND `python -m train.verify_roundtrip` passes (`max(\|diff\|) < 1e-5`) |
| **Gate 3** (Phase 3 done) | `cargo test --release` green AND `cargo run --release --bin ba283_standalone -- --input tests/fixtures/impulse.wav --output /tmp/out.wav --bias 0.5` produces a non-silent WAV |
| **Gate 4** (Phase 4 done) | `python tests/verify_numerical.py` exits 0 (broadband null ≤ −22 dB on all 3 held-out stimuli) |

---

# Phase 0 — Repo Bootstrap

## Task 0.1: Initialize Python project (uv)

**Files:**
- Create: `pyproject.toml`
- Create: `.python-version`
- Create: `data_gen/__init__.py`
- Create: `train/__init__.py`
- Create: `tests/__init__.py`
- Create: `tests/data_gen/__init__.py`
- Create: `tests/train/__init__.py`

**Step 1: Write `pyproject.toml`**

```toml
[project]
name = "ba283"
version = "0.1.0"
description = "Neve BA283 mic preamp neural emulation"
requires-python = ">=3.12,<3.13"
dependencies = [
    "numpy>=2.0",
    "scipy>=1.13",
    "soundfile>=0.12",
    "PySpice>=1.5",
    "torch>=2.4",
    "tensorboard>=2.17",
    "pandas>=2.2",
]

[dependency-groups]
dev = [
    "pytest>=8.0",
    "pytest-xdist>=3.6",
    "pytest-timeout>=2.3",
    "ruff>=0.6",
]

[tool.ruff]
line-length = 100
target-version = "py312"

[tool.ruff.lint]
select = ["E", "F", "I", "B", "UP", "SIM", "RUF"]
ignore = ["E501"]

[tool.pytest.ini_options]
testpaths = ["tests"]
addopts = "-ra --strict-markers"
```

**Step 2: Pin Python version**

Write `.python-version`:
```
3.12
```

**Step 3: Create empty package markers**

Write `data_gen/__init__.py`, `train/__init__.py`, `tests/__init__.py`, `tests/data_gen/__init__.py`, `tests/train/__init__.py` — all empty.

**Step 4: Install + verify**

Run:
```bash
uv sync
uv run python -c "import numpy, scipy, soundfile, PySpice, torch; print('ok')"
uv run pytest --co -q
```
Expected: `ok` printed; pytest reports `0 tests collected`.

**Step 5: Commit**

```bash
git add pyproject.toml .python-version data_gen/__init__.py train/__init__.py tests/__init__.py tests/data_gen/__init__.py tests/train/__init__.py uv.lock
git commit -m "chore: bootstrap Python project with uv"
```

## Task 0.2: Verify ngspice availability

**Files:**
- Create: `tests/data_gen/test_environment.py`

**Step 1: Write the failing test**

```python
# tests/data_gen/test_environment.py
import re
import shutil
import subprocess


def test_ngspice_on_path():
    assert shutil.which("ngspice") is not None, "ngspice CLI must be installed"


def test_ngspice_version_at_least_42():
    out = subprocess.check_output(["ngspice", "--version"], text=True)
    match = re.search(r"ngspice-(\d+)", out)
    assert match is not None, f"could not locate ngspice version tag in:\n{out}"
    major = int(match.group(1))
    assert major >= 42, f"ngspice major version {major} < 42"
```

**Step 2: Run**

Run: `uv run pytest tests/data_gen/test_environment.py -v`
Expected: PASS (or FAIL if ngspice not installed — stop and install via system package manager).

**Step 3: Commit**

```bash
git add tests/data_gen/test_environment.py
git commit -m "test(data_gen): assert ngspice CLI present and version >= 42"
```

## Task 0.3: Initialize Rust crate

**Files:**
- Create: `plugin/Cargo.toml`
- Create: `plugin/src/lib.rs` (stub)
- Create: `plugin/src/bin/standalone.rs` (stub)
- Create: `plugin/rust-toolchain.toml`
- Create: `Cargo.toml` (workspace root)

**Step 1: Workspace Cargo.toml**

```toml
# Cargo.toml
[workspace]
members = ["plugin"]
resolver = "2"

[profile.release]
lto = "fat"
codegen-units = 1
opt-level = 3
```

**Step 2: plugin/Cargo.toml**

```toml
[package]
name = "ba283"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
nih_plug = { git = "https://github.com/robbert-vdh/nih-plug.git", rev = "f36931f7af4646065488a9845d8f8c2f95252c23", features = ["vst3", "standalone"] }
serde_json = "1"
atomic_float = "1"
# TODO(Task 3.2): pick neural inference backend (pure-Rust hand-coded GRU+Dense vs experimental rtneural-rs git dep).
# Cannot add until Task 3.2 selects + verifies the runtime; placeholder removed so Phase 0 scaffold can build.

[dev-dependencies]
assert_no_alloc = "1.1"

[[bin]]
name = "ba283_standalone"
path = "src/bin/standalone.rs"
```

**Note:** the neural inference backend is deferred to Task 3.2. crates.io search at the time of Task 0.3 returned no `rtneural` crate, and the only candidate (`rcelha/rtneural-rs` on GitHub) is 0-star experimental. Phase 0 scaffold therefore builds without an inference dep; Task 3.2 will decide between (a) pure-Rust hand-coded GRU(2,32)+Dense(1) forward pass (preferred — fully real-time-safe, ~80 LOC, no deps) and (b) the experimental git crate.

**Step 3: rust-toolchain.toml**

```toml
# plugin/rust-toolchain.toml
[toolchain]
channel = "1.93.0"
components = ["rustfmt", "clippy"]
targets = ["x86_64-unknown-linux-gnu"]
```

**Step 4: stub lib.rs**

```rust
// plugin/src/lib.rs
//! BA283 neural preamp emulation - VST3 plugin (stub).
```

**Step 5: stub standalone bin**

```rust
// plugin/src/bin/standalone.rs
fn main() {
    println!("ba283 standalone stub");
}
```

**Step 6: Verify build**

Run: `cargo build --release`
Expected: builds without error.

**Step 7: Commit**

```bash
git add Cargo.toml plugin/Cargo.toml plugin/rust-toolchain.toml plugin/src/lib.rs plugin/src/bin/standalone.rs Cargo.lock
git commit -m "chore(plugin): scaffold Rust workspace + nih_plug crate"
```

---

# Phase 1 — Data Generation

## Task 1.1: Build the BA283 netlist (parameterized by bias)

**Files:**
- Create: `data_gen/netlist.py`
- Create: `tests/data_gen/test_netlist.py`

### Step 1.1.1: Write the failing test

```python
# tests/data_gen/test_netlist.py
import pytest
from data_gen.netlist import build_netlist


def test_build_netlist_returns_circuit_with_expected_nodes():
    circuit = build_netlist(bias_norm=0.5)
    netlist_str = str(circuit)
    for token in ("Vsupply", "BC184C", "2N3055", "LO1166", "Rterm", "C_esr"):
        assert token in netlist_str, f"netlist missing token {token!r}\n{netlist_str}"


def test_bias_changes_rv1_split():
    low = str(build_netlist(0.1))
    high = str(build_netlist(0.9))
    assert low != high


@pytest.mark.parametrize("bias", [-0.1, 1.1, 2.0])
def test_bias_out_of_range_raises(bias):
    with pytest.raises(ValueError):
        build_netlist(bias_norm=bias)
```

### Step 1.1.2: Run test to verify it fails

Run: `uv run pytest tests/data_gen/test_netlist.py -v`
Expected: FAIL — `ModuleNotFoundError`.

### Step 1.1.3: Implement `build_netlist`

Create `data_gen/netlist.py`. Must include:
- `build_netlist(bias_norm: float) -> Circuit` raises `ValueError` if `bias_norm` not in `[0.0, 1.0]`.
- `2N3055` and `BC184C` SPICE model cards verbatim from the design doc.
- LO1166 `.SUBCKT` body verbatim from the design doc Appendix A.2 (ngspice XSPICE `lcouple` + `core` PWL B-H model; the PSpice `CORE(MS/A/C/K)` + `K`-line idiom does not exist in ngspice).
- Bias mapping: `R_upper = 4700 * (1 - bias_norm)`, `R_lower = 4700 * bias_norm` (clamp each to ≥ 1 Ω).
- Comment: classic BA283 uses BC184 throughout; 2N3055 on TR3 is per user direction.
- Every electrolytic (C7, C4, gain-cap `Cgain`) modelled as cap in series with named `R_esr_*` of 1.0 Ω.
- 600 Ω termination resistor `Rterm SEC_IN 0 600`.
- `if __name__ == "__main__": print(str(build_netlist(0.5)))`.

### Step 1.1.4: Run tests to verify pass

Run: `uv run pytest tests/data_gen/test_netlist.py -v`
Expected: 5 PASS (3 parametrized + 2 standalone).

### Step 1.1.5: Smoke ngspice on the netlist

Note: ngspice cannot DC-`.op` the XSPICE transformer (the lcouple/core code-model windings have no DC operating point → singular matrix); use a `uic` transient.

Run:
```bash
uv run python -c "
from data_gen.netlist import build_netlist
c = build_netlist(0.5)
sim = c.simulator()
analysis = sim.transient(step_time=1e-5, end_time=2e-3, use_initial_condition=True)
import numpy as np
v = np.array(analysis['SEC_IN'])
print('SEC_IN samples:', v.shape, 'finite:', bool(np.all(np.isfinite(v))), 'std:', float(np.std(v)))
print('node A (rail):', float(np.array(analysis['A'])[-1]))
"
```
Expected: prints finite SEC_IN samples (non-empty, `finite: True`) and node A near 24.0. If non-convergence — fix topology before proceeding.

### Step 1.1.6: Commit

```bash
git add data_gen/netlist.py tests/data_gen/test_netlist.py
git commit -m "feat(data_gen): BA283 netlist parameterized by RV1 bias"
```

## Task 1.2: Stimulus generation

**Files:**
- Create: `data_gen/stimulus.py`
- Create: `tests/data_gen/test_stimulus.py`

### Step 1.2.1: Write the failing test

```python
# tests/data_gen/test_stimulus.py
import numpy as np
import pytest

from data_gen.stimulus import (
    SAMPLE_RATE,
    log_sine_sweep,
    multisine_schroeder,
    pink_noise_bursts,
    two_tone_imd,
)


def test_sample_rate_is_96k():
    assert SAMPLE_RATE == 96_000


def test_log_sine_sweep_peak_amplitude():
    sweep = log_sine_sweep(duration_s=0.5, f_start=100.0, f_end=1000.0, level_db=-6.0)
    assert sweep.dtype == np.float32
    assert sweep.shape == (int(0.5 * SAMPLE_RATE),)
    target_peak = 10 ** (-6.0 / 20.0)
    assert np.max(np.abs(sweep)) == pytest.approx(target_peak, rel=0.05)


def test_pink_noise_bursts_alternates_loud_and_silent():
    bursts = pink_noise_bursts(duration_s=2.0, burst_ms=200, gap_ms=800, peak_db=-6.0)
    assert np.any(np.abs(bursts) > 0.01)
    assert np.any(np.abs(bursts) < 1e-6)


def test_two_tone_has_two_frequencies():
    sig = two_tone_imd(duration_s=0.5, f1=200.0, f2=300.0, level_db=-6.0)
    spec = np.abs(np.fft.rfft(sig))
    freqs = np.fft.rfftfreq(len(sig), 1 / SAMPLE_RATE)
    top_two = freqs[np.argsort(spec)[-2:]]
    assert {round(f, -1) for f in top_two} == {200.0, 300.0}


def test_multisine_schroeder_low_crest_factor():
    sig = multisine_schroeder(duration_s=0.5, num_tones=20, f_low=100.0, f_high=5000.0)
    crest = np.max(np.abs(sig)) / np.sqrt(np.mean(sig**2))
    assert crest < 3.0
```

### Step 1.2.2: Run test to verify it fails

Run: `uv run pytest tests/data_gen/test_stimulus.py -v`
Expected: FAIL.

### Step 1.2.3: Implement `data_gen/stimulus.py`

Implement:
- `SAMPLE_RATE = 96_000`
- `log_sine_sweep(duration_s, f_start, f_end, level_db) -> np.ndarray[np.float32]` using `scipy.signal.chirp(method="logarithmic")`.
- `pink_noise_bursts(duration_s, burst_ms, gap_ms, peak_db)` — FFT-coloured white noise enveloped by rectangular bursts.
- `two_tone_imd(duration_s, f1, f2, level_db)`.
- `multisine_schroeder(duration_s, num_tones, f_low, f_high)` — linear-spaced tones with Schroeder phases `φ_k = -k(k-1)π/N`. (Schroeder phases only minimize crest factor for equally-spaced tones; log spacing breaks the crest<3 contract — 3.42 vs 1.92. Broadband log coverage is supplied by the sweeps + pink bursts in the block.)
- `filtered_impulses(duration_s)` — 10 impulses + step responses.
- `load_user_music(paths) -> np.ndarray` — load via `soundfile`, resample to 96 kHz via `scipy.signal.resample_poly`, peak-normalize to 0.5 V, concatenate with 50 ms silence between.
- `load_dry(path) -> (samples, 96000)`.
- `build_synthetic_block()` — 4 sweeps (30 s each at −18/−12/−6/−1 dBFS) + 1 min pink bursts + 1 min IMD + 1 min impulses + 3 min multisine, peak-normalized to 0.5 V. (Kept for reference; no longer used by `main()`.)
- `build_synthetic_segments() -> dict[str, np.ndarray]` — emits ~11 per-segment stimuli (4 sweeps, 2 pink, 2 two-tone, impulse, 2 multisine), each scaled via `_scale_to_dbfs` to a peak of `0.5 * 10**(level_db/20)` so relative drive levels are preserved (level-dependent nonlinearity coverage) while staying ≤ 0.5 V. Each is its own file so the leakage-free-by-file manifest split (Task 1.4) has enough distinct files for an 80/10/10 split.
- `main()`: writes one WAV per `build_synthetic_segments()` entry to `data/dry/<name>.wav`, and (if `--music-paths` passed) each music clip as its own `data/dry/music_<i>_<stem>.wav` (loaded via `load_dry`, scaled to −6 dBFS). Clobber-guarded by `--overwrite`.

### Step 1.2.4: Run tests

Run: `uv run pytest tests/data_gen/test_stimulus.py -v`
Expected: 5 PASS.

### Step 1.2.5: Commit

```bash
git add data_gen/stimulus.py tests/data_gen/test_stimulus.py
git commit -m "feat(data_gen): synthetic + music stimulus library at 96 kHz"
```

## Task 1.3: SPICE execution

**Files:**
- Create: `data_gen/run_spice.py`
- Create: `tests/data_gen/test_run_spice.py`
- Create: `tests/fixtures/short_sine_96k.wav` (generated in step 1)

### Step 1.3.1: Generate test fixture WAV

Run:
```bash
uv run python -c "
import numpy as np, soundfile as sf, os
os.makedirs('tests/fixtures', exist_ok=True)
sr = 96_000
t = np.arange(int(0.05 * sr)) / sr
x = 0.3 * np.sin(2*np.pi*1000*t).astype(np.float32)
sf.write('tests/fixtures/short_sine_96k.wav', x, sr, subtype='FLOAT')
print('wrote', x.shape)
"
```

### Step 1.3.2: Write the failing test

```python
# tests/data_gen/test_run_spice.py
from pathlib import Path

import numpy as np

from data_gen.run_spice import simulate_one


FIXTURE = Path("tests/fixtures/short_sine_96k.wav")


def test_simulate_one_returns_time_and_voltage_arrays(tmp_path):
    out = simulate_one(dry_wav=FIXTURE, bias=0.5, out_npz=tmp_path / "wet.npz")
    assert out.exists()
    data = np.load(out)
    assert "t" in data and "v" in data
    assert data["t"].shape == data["v"].shape
    assert data["t"][-1] >= 0.05


def test_simulate_one_voltage_is_finite(tmp_path):
    out = simulate_one(dry_wav=FIXTURE, bias=0.5, out_npz=tmp_path / "wet.npz")
    v = np.load(out)["v"]
    assert np.all(np.isfinite(v))
    assert np.std(v) > 1e-4
```

### Step 1.3.3: Run test to verify it fails

Run: `uv run pytest tests/data_gen/test_run_spice.py -v`
Expected: FAIL — module missing.

### Step 1.3.4: Implement `data_gen/run_spice.py`

Implement:
- `simulate_one(dry_wav, bias, out_npz) -> Path`:
  1. Load WAV via `soundfile`, assert SR == 96000.
  2. Build netlist via `build_netlist(bias)`.
  3. Write a sidecar `time value` ASCII file (one line per 96 kHz sample, `time = i / 96000`) next to `out_npz` for the dry stimulus. Single path for every duration — no inline PWL, no `PWL file=` (the latter is unsupported in ngspice 46).
  4. Inject via the XSPICE `filesource` code model: `a_in %vd([INPUT 0]) inputsrc` plus `.model inputsrc filesource (file="<abs_path>" ...)`.
  5. Configure simulator with `.options reltol=1e-4 abstol=1e-9 vntol=1e-6 gmin=1e-13 method=gear`.
  6. Do NOT run `.op` — the XSPICE transformer has no DC operating point. Run the transient with `use_initial_condition=True` (uic); rely on the 50 ms head margin + Task 1.4's 50 ms trim for bias settling.
  7. `tran(step_time=1/(96000*8), end_time=duration+0.05, use_initial_condition=True)`.
  8. Extract `analysis['SEC_IN']` → `(t, v)`.
  9. `np.savez_compressed(out_npz, t=t, v=v)`.
  10. Return `out_npz`.
- `run_batch(manifest_paths, biases, workers)` via `multiprocessing.Pool`. On `NgSpiceCommandError` / `RuntimeError`: append to `data/failed.csv`, do not crash.
- Per-run stderr captured to `data/spice_logs/<stem>_bias<value>.log`.
- `main()` argparse: `--manifest`, `--biases 0.0,0.25,0.5,0.75,1.0`, `--workers` (default `max(1, cpu_count() - 2)`), `--smoke` (runs one pair against `tests/fixtures/short_sine_96k.wav`), `--single <wav> --bias <float> --out <wav>` for Phase 4.

### Step 1.3.5: Run tests

Run: `uv run pytest tests/data_gen/test_run_spice.py -v --timeout=120`
Expected: 2 PASS within 30–90 s.

### Step 1.3.6: Smoke test the CLI

Run: `uv run python -m data_gen.run_spice --smoke`
Expected: writes `data/wet_raw/short_sine_96k_bias0.50.npz`, exit 0.

### Step 1.3.7: Commit

```bash
git add data_gen/run_spice.py tests/data_gen/test_run_spice.py tests/fixtures/short_sine_96k.wav
git commit -m "feat(data_gen): SPICE batch runner with continuous transient sim"
```

## Task 1.4: Resample + manifest

**Files:**
- Create: `data_gen/resample.py`
- Create: `tests/data_gen/test_resample.py`

### Step 1.4.1: Write the failing test

```python
# tests/data_gen/test_resample.py
from pathlib import Path

import numpy as np
import pandas as pd
import soundfile as sf

from data_gen.resample import resample_to_grid, split_manifest, write_manifest


def test_resample_lands_on_96k_grid(tmp_path):
    t = np.cumsum(np.random.default_rng(0).uniform(5e-7, 1.5e-6, size=20_000))
    v = np.sin(2 * np.pi * 1000 * t).astype(np.float32)
    npz = tmp_path / "raw.npz"
    np.savez(npz, t=t, v=v)
    out = tmp_path / "wet.wav"
    resample_to_grid(npz, out, target_sr=96_000, head_trim_ms=50)
    data, sr = sf.read(out, dtype="float32")
    assert sr == 96_000
    assert data.shape[0] > 0
    assert 0.9 <= np.max(np.abs(data)) <= 1.1


def test_write_manifest_columns(tmp_path):
    rows = [
        {"dry_path": "a.wav", "wet_path": "a_bias0.50.wav", "bias_value": 0.5, "duration_s": 1.0, "stim_kind": "sweep"},
    ]
    out = tmp_path / "manifest.csv"
    write_manifest(rows, out)
    text = out.read_text()
    assert "dry_path,wet_path,bias_value,duration_s,stim_kind" in text.splitlines()[0]
    assert "a.wav" in text


def test_split_by_stimulus_file_no_leakage(tmp_path):
    rows = []
    for stim in ["sweepA", "sweepB", "sweepC", "noiseA", "noiseB"]:
        for bias in [0.0, 0.5, 1.0]:
            rows.append({
                "dry_path": f"dry/{stim}.wav",
                "wet_path": f"wet/{stim}_bias{bias:.2f}.wav",
                "bias_value": bias,
                "duration_s": 60.0,
                "stim_kind": "sweep" if "sweep" in stim else "noise",
            })
    manifest = tmp_path / "manifest.csv"
    pd.DataFrame(rows).to_csv(manifest, index=False)
    split_manifest(manifest, tmp_path, ratios=(0.6, 0.2, 0.2), seed=0)
    train = pd.read_csv(tmp_path / "train.csv")
    val = pd.read_csv(tmp_path / "val.csv")
    test = pd.read_csv(tmp_path / "test.csv")
    train_stems = set(train["dry_path"])
    val_stems = set(val["dry_path"])
    test_stems = set(test["dry_path"])
    assert train_stems.isdisjoint(val_stems)
    assert train_stems.isdisjoint(test_stems)
    assert val_stems.isdisjoint(test_stems)
```

### Step 1.4.2: Run test to verify it fails

Run: `uv run pytest tests/data_gen/test_resample.py -v`
Expected: FAIL.

### Step 1.4.3: Implement `data_gen/resample.py`

- `resample_to_grid(npz_path, out_wav, target_sr=96_000, head_trim_ms=50)`:
  1. Load `(t, v)`.
  2. Uniform grid `tg = np.arange(t[0], t[-1], 1/target_sr)`.
  3. `scipy.interpolate.PchipInterpolator(t, v)(tg)`.
  4. Trim first `head_trim_ms * target_sr / 1000` samples.
  5. `soundfile.write(out_wav, v_uniform.astype(np.float32), target_sr, subtype="FLOAT")`.
- `write_manifest(rows, out_csv)`: `pandas.DataFrame(rows).to_csv(out_csv, index=False)` with explicit column order.
- `split_manifest(manifest_csv, out_dir, ratios=(0.8, 0.1, 0.1), seed=42)`: split by `dry_path`, write `train.csv`, `val.csv`, `test.csv`.
- `main()` argparse: `--raw-dir`, `--out-dir`, `--manifest`, `--dry-dir` (default `data/dry`). `dry_path` is written as `dry_dir/<stem>.wav` so the manifest resolves to the real dry stimulus.

### Step 1.4.4: Run tests

Run: `uv run pytest tests/data_gen/test_resample.py -v`
Expected: 3 PASS.

### Step 1.4.5: Commit

```bash
git add data_gen/resample.py tests/data_gen/test_resample.py
git commit -m "feat(data_gen): PCHIP resample to 96k grid + leakage-free manifest split"
```

## Task 1.5: Phase 1 gate check

### Step 1.5.1: Run full Phase 1 test suite

Run: `uv run pytest tests/data_gen/ -v`
Expected: all green.

### Step 1.5.2: End-to-end gate (short-stimulus pipeline)

Run: `uv run python tests/gate_phase1.py`

Fast end-to-end on 6 short (~0.15 s) dry stimuli of varied generators/levels
× 3 biases (`[0.0, 0.5, 1.0]`). Generates dry WAVs → SPICE batch → PCHIP
resample → manifest → leakage-free 4/1/1 split. Asserts a non-empty,
pairwise-disjoint train/val/test split and a decodable 96 kHz wet WAV.
Artifacts land under gitignored `data/gate/` (cleaned at start for
deterministic re-runs). Expected: prints `PHASE 1 GATE: PASS`, exit 0.

(Replaces the original single-fixture `run_spice --smoke` smoke: that produced
only one dry file, so `split_manifest` — which now requires ≥3 unique stimuli
after the per-segment-file + split-guard changes — could not form a valid
3-way split.)

### Step 1.5.3: Gate 1 verification

@superpowers:verification-before-completion. The gate script
(`tests/gate_phase1.py`) itself asserts and exits 1 on any failure:
- `data/gate/manifest.csv` exists with ≥1 data row (18 expected: 6 dry × 3 biases).
- `train.csv`/`val.csv`/`test.csv` all exist and are non-empty.
- The three split `dry_path` sets are pairwise disjoint (leakage-free).
- At least one wet WAV decodes via `soundfile.read` at sr==96000 with finite, >0 samples.
- SPICE failures are tolerated up to 20% of pairs (failures logged to `data/gate/failed.csv`); a larger failure rate fails the gate.

Also confirm `uv run pytest tests/data_gen/` is all green. If any fails — back up and fix.

### Step 1.5.4: Commit

```bash
git commit --allow-empty -m "chore: pass Phase 1 gate (data generation)"
```

---

# Phase 2 — PyTorch Training

## Task 2.1: Dataset

**Files:**
- Create: `train/dataset.py`
- Create: `tests/train/test_dataset.py`
- Create: `tests/fixtures/manifest_tiny.csv` (generated in step 1)

### Step 2.1.1: Generate tiny fixture manifest

Run:
```bash
uv run python -c "
import numpy as np, soundfile as sf, pandas as pd, os
os.makedirs('tests/fixtures/fixt_data', exist_ok=True)
sr = 96_000
n = sr * 2
t = np.arange(n) / sr
for stim in ['sweepA', 'sweepB']:
    dry = (0.4 * np.sin(2*np.pi*440*t)).astype(np.float32)
    sf.write(f'tests/fixtures/fixt_data/{stim}.wav', dry, sr, subtype='FLOAT')
    for bias in [0.0, 0.5, 1.0]:
        wet = (np.tanh(2*dry) * 0.5).astype(np.float32)
        sf.write(f'tests/fixtures/fixt_data/{stim}_bias{bias:.2f}.wav', wet, sr, subtype='FLOAT')
rows = []
for stim in ['sweepA', 'sweepB']:
    for bias in [0.0, 0.5, 1.0]:
        rows.append({
            'dry_path': f'tests/fixtures/fixt_data/{stim}.wav',
            'wet_path': f'tests/fixtures/fixt_data/{stim}_bias{bias:.2f}.wav',
            'bias_value': bias,
            'duration_s': 2.0,
            'stim_kind': 'sweep',
        })
pd.DataFrame(rows).to_csv('tests/fixtures/manifest_tiny.csv', index=False)
print('ok')
"
```

### Step 2.1.2: Write the failing test

```python
# tests/train/test_dataset.py
from pathlib import Path

import torch

from train.dataset import BA283Dataset, make_loader


MANIFEST = Path("tests/fixtures/manifest_tiny.csv")


def test_dataset_returns_correct_shapes():
    ds = BA283Dataset(MANIFEST, window=4096, stride=2048)
    x, y = ds[0]
    assert x.shape == (4096, 2)
    assert y.shape == (4096, 1)
    assert x.dtype == torch.float32
    assert y.dtype == torch.float32


def test_bias_constant_across_window():
    ds = BA283Dataset(MANIFEST, window=4096, stride=2048)
    x, _ = ds[0]
    bias_col = x[:, 1]
    assert torch.all(bias_col == bias_col[0])


def test_dataset_covers_all_files_and_biases():
    ds = BA283Dataset(MANIFEST, window=4096, stride=2048)
    seen_bias = set()
    for i in range(len(ds)):
        x, _ = ds[i]
        seen_bias.add(round(float(x[0, 1]), 4))
    assert seen_bias == {0.0, 0.5, 1.0}


def test_input_scale_applied():
    ds = BA283Dataset(MANIFEST, window=4096, stride=2048)
    x, _ = ds[0]
    audio = x[:, 0]
    assert 0.7 < audio.abs().max().item() < 0.9


def test_make_loader_yields_batches():
    loader = make_loader(MANIFEST, batch_size=4, window=4096, stride=2048, num_workers=0, shuffle=False)
    batch_x, batch_y = next(iter(loader))
    assert batch_x.shape == (4, 4096, 2)
    assert batch_y.shape == (4, 4096, 1)
```

### Step 2.1.3: Run test to verify it fails

Run: `uv run pytest tests/train/test_dataset.py -v`
Expected: FAIL.

### Step 2.1.4: Implement `train/dataset.py`

- `class BA283Dataset(torch.utils.data.Dataset)`:
  - `__init__(self, manifest_csv, window=4096, stride=2048, input_scale=2.0)`.
  - Read manifest with `pandas`, validate columns.
  - Pre-compute index of `(row_idx, sample_start)` pairs by walking each row's wet WAV length.
  - `__getitem__` opens WAVs lazily via `soundfile.SoundFile(...).seek(...).read(window, dtype="float32")`.
  - `x[:, 0] = dry * input_scale`, `x[:, 1] = bias_value` broadcast, `y[:, 0] = wet * input_scale`.
- `make_loader(manifest_csv, *, batch_size=64, window=4096, stride=2048, num_workers=6, shuffle=True) -> DataLoader` with `pin_memory=True`, `persistent_workers=(num_workers > 0)`.

### Step 2.1.5: Run tests

Run: `uv run pytest tests/train/test_dataset.py -v`
Expected: 5 PASS.

### Step 2.1.6: Commit

```bash
git add train/dataset.py tests/train/test_dataset.py tests/fixtures/manifest_tiny.csv tests/fixtures/fixt_data/
git commit -m "feat(train): conditioned BA283Dataset with bias broadcast"
```

## Task 2.2: Model

**Files:**
- Create: `train/model.py`
- Create: `tests/train/test_model.py`

### Step 2.2.1: Write the failing test

```python
# tests/train/test_model.py
import torch

from train.model import BA283GRU


def test_forward_output_shape():
    model = BA283GRU(hidden=32)
    x = torch.randn(4, 4096, 2)
    y, h = model(x)
    assert y.shape == (4, 4096, 1)
    assert h.shape == (1, 4, 32)


def test_param_count_in_expected_range():
    model = BA283GRU(hidden=32)
    total = sum(p.numel() for p in model.parameters())
    assert 3000 < total < 4000


def test_state_dict_keys_match_rtneural_expectations():
    model = BA283GRU(hidden=32)
    keys = set(model.state_dict().keys())
    required = {
        "gru.weight_ih_l0",
        "gru.weight_hh_l0",
        "gru.bias_ih_l0",
        "gru.bias_hh_l0",
        "dense.weight",
        "dense.bias",
    }
    assert required <= keys, f"missing keys: {required - keys}"


def test_initial_hidden_state_can_be_passed():
    model = BA283GRU(hidden=32)
    x = torch.randn(2, 1024, 2)
    h0 = torch.zeros(1, 2, 32)
    y, h1 = model(x, h0)
    assert y.shape == (2, 1024, 1)
    assert h1.shape == (1, 2, 32)


def test_output_is_finite():
    model = BA283GRU(hidden=32)
    x = torch.zeros(1, 16, 2)
    x[..., 0] = 0.5
    with torch.no_grad():
        y, _ = model(x)
    assert torch.all(torch.isfinite(y))
```

### Step 2.2.2: Run test to verify it fails

Run: `uv run pytest tests/train/test_model.py -v`
Expected: FAIL.

### Step 2.2.3: Implement `train/model.py`

```python
# train/model.py
import torch
from torch import nn


class BA283GRU(nn.Module):
    def __init__(self, hidden: int = 32) -> None:
        super().__init__()
        self.hidden = hidden
        self.gru = nn.GRU(
            input_size=2,
            hidden_size=hidden,
            num_layers=1,
            batch_first=True,
        )
        self.dense = nn.Linear(hidden, 1)

    def forward(
        self, x: torch.Tensor, h: torch.Tensor | None = None
    ) -> tuple[torch.Tensor, torch.Tensor]:
        y, h_out = self.gru(x, h)
        return self.dense(y), h_out
```

### Step 2.2.4: Run tests

Run: `uv run pytest tests/train/test_model.py -v`
Expected: 5 PASS.

### Step 2.2.5: Commit

```bash
git add train/model.py tests/train/test_model.py
git commit -m "feat(train): BA283GRU(hidden=32) conditioned model"
```

## Task 2.3: Training loop

**Files:**
- Create: `train/loss.py`
- Create: `tests/train/test_loss.py`
- Create: `train/train.py`
- Create: `tests/train/test_train_smoke.py`

### Step 2.3.1: Write the failing test (loss)

```python
# tests/train/test_loss.py
import torch

from train.loss import composite_loss, esr, pre_emphasis


def test_pre_emphasis_high_passes_signal():
    sr = 96_000
    t = torch.arange(sr) / sr
    low = torch.sin(2 * 3.14159 * 50 * t).unsqueeze(0).unsqueeze(-1)
    high = torch.sin(2 * 3.14159 * 10_000 * t).unsqueeze(0).unsqueeze(-1)
    low_e = pre_emphasis(low, alpha=0.85)
    high_e = pre_emphasis(high, alpha=0.85)
    assert high_e.pow(2).mean() > 10 * low_e.pow(2).mean()


def test_esr_zero_when_pred_equals_target():
    t = torch.randn(2, 100, 1)
    assert esr(t, t).item() < 1e-6


def test_composite_loss_decreases_when_pred_improves():
    target = torch.randn(2, 100, 1)
    bad = target + torch.randn_like(target)
    good = target + 0.01 * torch.randn_like(target)
    l_bad = composite_loss(bad, target).item()
    l_good = composite_loss(good, target).item()
    assert l_good < l_bad
```

### Step 2.3.2: Run test to verify it fails

Run: `uv run pytest tests/train/test_loss.py -v`
Expected: FAIL.

### Step 2.3.3: Implement `train/loss.py`

```python
# train/loss.py
import torch
import torch.nn.functional as F


def pre_emphasis(x: torch.Tensor, alpha: float = 0.85) -> torch.Tensor:
    return x[:, 1:, :] - alpha * x[:, :-1, :]


def esr(pred: torch.Tensor, target: torch.Tensor) -> torch.Tensor:
    return ((pred - target) ** 2).sum() / ((target ** 2).sum() + 1e-8)


def composite_loss(
    pred: torch.Tensor, target: torch.Tensor, *, esr_weight: float = 0.75, alpha: float = 0.85
) -> torch.Tensor:
    pe = pre_emphasis(pred, alpha)
    te = pre_emphasis(target, alpha)
    return esr_weight * esr(pe, te) + (1 - esr_weight) * F.mse_loss(pred, target)
```

### Step 2.3.4: Run loss tests

Run: `uv run pytest tests/train/test_loss.py -v`
Expected: 3 PASS.

### Step 2.3.5: Write the failing test (training smoke)

```python
# tests/train/test_train_smoke.py
from pathlib import Path

from train.train import train_main


def test_smoke_run_creates_checkpoint(tmp_path):
    ckpt_dir = tmp_path / "ckpts"
    log_dir = tmp_path / "runs"
    train_main(
        train_manifest=Path("tests/fixtures/manifest_tiny.csv"),
        val_manifest=Path("tests/fixtures/manifest_tiny.csv"),
        epochs=1,
        batch_size=2,
        lr=5e-3,
        hidden=32,
        warmup_samples=512,
        window=2048,
        stride=2048,
        num_workers=0,
        checkpoint_dir=ckpt_dir,
        log_dir=log_dir,
        device="cpu",
        smoke=True,
    )
    assert (ckpt_dir / "best.pth").exists()
```

### Step 2.3.6: Run test to verify it fails

Run: `uv run pytest tests/train/test_train_smoke.py -v`
Expected: FAIL.

### Step 2.3.7: Implement `train/train.py`

`train_main(...)` exposes every knob shown in the smoke test. For each epoch:
- Iterate train loader. Per batch:
  1. Split window into warmup (first `warmup_samples`) + train region (rest).
  2. `with torch.no_grad(): _, h = model(warmup)`.
  3. `pred, _ = model(rest, h)`.
  4. `loss = composite_loss(pred, rest_target)`.
  5. `loss.backward(); optimizer.step(); optimizer.zero_grad()`.
- Validation pass under `torch.no_grad()`, aggregate `val_esr = esr(all_pred, all_target)`.
- Log to TensorBoard via `torch.utils.tensorboard.SummaryWriter`.
- Save `checkpoint_dir / f"epoch_{e:03d}.pth"` only on val_esr improvement; copy to `best.pth`.
- Early stop on patience=10.
- `CosineAnnealingLR(T_max=epochs)`.
- If `smoke`: limit to first 2 batches per epoch.
- Hardened for unattended multi-hour runs: gradient clipping (`clip_norm`) guards GRU divergence; non-finite-loss batches are skipped and a NaN `val_esr` is never checkpointed as best; checkpoints are written atomically (tmp + `os.replace`); a resumable `last.pth` bundle (model/optimizer/scheduler/epoch/best/patience) supports `--resume`; `--seed` for reproducibility; best.pth weights are reloaded into the in-memory model at the end; epoch-mean train loss is logged.

CLI `main()` argparse: `--train-manifest`, `--val-manifest`, `--epochs`, `--batch`, `--lr`, `--hidden`, `--warmup`, `--window`, `--stride`, `--workers`, `--checkpoint-dir`, `--log-dir`, `--device`, `--smoke`.

### Step 2.3.8: Run smoke test

Run: `uv run pytest tests/train/test_train_smoke.py -v --timeout=180`
Expected: PASS within ~60 s on CPU.

### Step 2.3.9: Commit

```bash
git add train/loss.py train/train.py tests/train/test_loss.py tests/train/test_train_smoke.py
git commit -m "feat(train): TBPTT training loop with ESR + MSE pre-emphasis loss"
```

## Task 2.4: RTNeural export + round-trip verification

**Files:**
- Create: `train/export.py`
- Create: `train/verify_roundtrip.py`
- Create: `tests/train/test_export.py`

### Step 2.4.1: Write the failing test

```python
# tests/train/test_export.py
import json
from pathlib import Path

from train.export import export_rtneural_json
from train.model import BA283GRU


def test_exported_json_has_expected_layers(tmp_path):
    model = BA283GRU(hidden=32)
    out = tmp_path / "ba283.json"
    export_rtneural_json(model, out, meta={"sample_rate": 96000, "input_scale": 2.0})
    text = out.read_text().lower()
    assert "gru" in text
    assert "dense" in text


def test_exported_weights_byte_count_in_expected_range(tmp_path):
    model = BA283GRU(hidden=32)
    out = tmp_path / "ba283.json"
    export_rtneural_json(model, out, meta={"sample_rate": 96000, "input_scale": 2.0})
    data = json.loads(out.read_text())
    flat_total = sum(_count_floats(v) for v in _walk(data))
    expected_total = 3 * (32 * 2) + 3 * (32 * 32) + 3 * 32 + 3 * 32 + 32 + 1
    assert flat_total >= expected_total


def _walk(node):
    if isinstance(node, list):
        yield node
        for v in node:
            yield from _walk(v)
    elif isinstance(node, dict):
        for v in node.values():
            yield from _walk(v)


def _count_floats(node):
    if isinstance(node, list):
        if node and all(isinstance(x, (int, float)) for x in node):
            return len(node)
        return sum(_count_floats(v) for v in node)
    return 0
```

### Step 2.4.2: Run test to verify it fails

Run: `uv run pytest tests/train/test_export.py -v`
Expected: FAIL.

### Step 2.4.3: Implement `train/export.py`

- Inspect `rtneural` Python helper. If installed: use it. Otherwise hand-roll JSON per rtneural-rs schema:
  ```json
  {
    "in_shape": [null, 2],
    "layers": [
      {"type": "gru", "shape": [null, 32], "activation": "tanh", "recurrent_activation": "sigmoid", "weights": [W_ih, W_hh, b]},
      {"type": "dense", "shape": [null, 1], "activation": "linear", "weights": [W, b]}
    ]
  }
  ```
- Signature: `def export_rtneural_json(model: BA283GRU, out_path: Path, meta: dict) -> None`.
- Also write `out_path.with_suffix(".meta.json")` containing `meta`.

**Critical:** the exact layout MUST match what rtneural-rs reads in Task 3.2. The round-trip test in Task 3.2 is the contract. If unsure, skim rtneural-rs source (`json_parser.rs`) before guessing.

### Step 2.4.4: Run tests

Run: `uv run pytest tests/train/test_export.py -v`
Expected: 2 PASS.

### Step 2.4.5: Implement `train/verify_roundtrip.py`

Stub at this phase; extended in Task 3.2 after the Rust CLI exists.

```python
# train/verify_roundtrip.py
import argparse
from pathlib import Path

import numpy as np
import torch

from train.model import BA283GRU


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--checkpoint", type=Path, default=Path("checkpoints/best.pth"))
    ap.add_argument("--out-ref", type=Path, default=Path("/tmp/pytorch_ref.npy"))
    args = ap.parse_args()

    model = BA283GRU(hidden=32)
    state = torch.load(args.checkpoint, map_location="cpu", weights_only=True)
    model.load_state_dict(state)
    model.train(False)

    rng = np.random.default_rng(0)
    x = rng.standard_normal((1, 1000, 2)).astype(np.float32)
    x[..., 1] = 0.5
    with torch.no_grad():
        y, _ = model(torch.from_numpy(x))
    np.save(args.out_ref, y.numpy())
    print(f"saved {args.out_ref}, shape={y.shape}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
```

### Step 2.4.6: Commit

```bash
git add train/export.py train/verify_roundtrip.py tests/train/test_export.py
git commit -m "feat(train): rtneural JSON export + roundtrip stub"
```

## Task 2.5: Phase 2 gate check

### Step 2.5.1: Run full Phase 2 suite

Run: `uv run pytest tests/train/ -v`
Expected: all green.

### Step 2.5.2: Commit gate

```bash
git commit --allow-empty -m "chore: pass Phase 2 gate (training pipeline tested)"
```

**Note:** the actual full training run on real BA283 data is a long-running data-science task (~2–4 hours on CPU once Phase 1 has rendered 60 wet WAVs). Not a code change; documented in `docs/training_runbook.md` (Task 2.6).

## Task 2.6: Training runbook

**Files:**
- Create: `docs/training_runbook.md`

### Step 2.6.1: Write runbook

Cover:
1. Prereqs: Phase 1 outputs exist (`data/wet/*.wav`, `data/splits/{train,val,test}.csv`).
2. Command: `uv run python -m train.train --train-manifest data/splits/train.csv --val-manifest data/splits/val.csv --epochs 200 --batch 64 --hidden 32`.
3. Expected wall-clock on 8-core x86.
4. Target val ESR ≤ 0.02 (~−17 dB).
5. Export: `uv run python -m train.export --checkpoint checkpoints/best.pth --out plugin/weights/ba283_gru32.json`.
6. TensorBoard: `uv run tensorboard --logdir runs/`.

### Step 2.6.2: Commit

```bash
git add docs/training_runbook.md
git commit -m "docs(train): training runbook"
```

---

# Phase 3 — Rust Plugin

## Task 3.1: Plugin scaffold + params

**Files:**
- Modify: `plugin/src/lib.rs`
- Create: `plugin/tests/test_params.rs`

### Step 3.1.1: Write the failing test

```rust
// plugin/tests/test_params.rs
use ba283::Ba283Params;
use nih_plug::prelude::*;

#[test]
fn default_params_have_expected_values() {
    let p = Ba283Params::default();
    assert!((p.drive_db.value() - 0.0).abs() < 1e-6);
    assert!((p.bias.value() - 0.5).abs() < 1e-6);
    assert!((p.output_db.value() - 0.0).abs() < 1e-6);
    assert!((p.mix.value() - 100.0).abs() < 1e-6);
}

#[test]
fn drive_db_range_bounded() {
    let p = Ba283Params::default();
    assert!(p.drive_db.preview_normalized(-12.0) >= 0.0);
    assert!(p.drive_db.preview_normalized(24.0) <= 1.0);
}
```

### Step 3.1.2: Run test to verify it fails

Run: `cd plugin && cargo test --test test_params 2>&1 | tail -20`
Expected: compile error.

### Step 3.1.3: Implement params and plugin shell

Write `plugin/src/lib.rs`:

```rust
use nih_plug::prelude::*;
use std::num::NonZeroU32;
use std::sync::Arc;

pub struct Ba283Plugin {
    params: Arc<Ba283Params>,
}

#[derive(Params)]
pub struct Ba283Params {
    #[id = "drive_db"]
    pub drive_db: FloatParam,
    #[id = "bias"]
    pub bias: FloatParam,
    #[id = "output_db"]
    pub output_db: FloatParam,
    #[id = "mix"]
    pub mix: FloatParam,
}

impl Default for Ba283Params {
    fn default() -> Self {
        Self {
            drive_db: FloatParam::new(
                "Drive",
                0.0,
                FloatRange::Linear { min: -12.0, max: 24.0 },
            )
            .with_unit(" dB")
            .with_smoother(SmoothingStyle::Linear(20.0)),
            bias: FloatParam::new(
                "Bias",
                0.5,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_smoother(SmoothingStyle::Linear(50.0)),
            output_db: FloatParam::new(
                "Output",
                0.0,
                FloatRange::Linear { min: -24.0, max: 12.0 },
            )
            .with_unit(" dB")
            .with_smoother(SmoothingStyle::Linear(20.0)),
            mix: FloatParam::new(
                "Mix",
                100.0,
                FloatRange::Linear { min: 0.0, max: 100.0 },
            )
            .with_unit(" %")
            .with_smoother(SmoothingStyle::Linear(20.0)),
        }
    }
}

impl Default for Ba283Plugin {
    fn default() -> Self {
        Self { params: Arc::new(Ba283Params::default()) }
    }
}

impl Plugin for Ba283Plugin {
    const NAME: &'static str = "BA283";
    const VENDOR: &'static str = "gruber.dev.br";
    const URL: &'static str = "";
    const EMAIL: &'static str = "contact@gruber.dev.br";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(1),
            main_output_channels: NonZeroU32::new(1),
            ..AudioIOLayout::const_default()
        },
    ];

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        _buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        true
    }

    fn reset(&mut self) {}

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        for mut frame in buffer.iter_samples() {
            for sample in frame.iter_mut() {
                *sample = *sample;
            }
        }
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for Ba283Plugin {
    const VST3_CLASS_ID: [u8; 16] = *b"ba283-neve-vst3.";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Dynamics];
}

impl ClapPlugin for Ba283Plugin {
    const CLAP_ID: &'static str = "br.dev.gruber.ba283";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("Neve BA283 neural emulation");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[ClapFeature::AudioEffect];
}

nih_export_vst3!(Ba283Plugin);
```

### Step 3.1.4: Run tests

Run: `cd plugin && cargo test --test test_params --release`
Expected: PASS.

### Step 3.1.5: Build the plugin

Run: `cargo build --release`
Expected: builds successfully.

### Step 3.1.6: Commit

```bash
git add plugin/src/lib.rs plugin/tests/test_params.rs Cargo.lock
git commit -m "feat(plugin): nih_plug scaffold with Drive/Bias/Output/Mix params"
```

## Task 3.2: rtneural model wrapper + round-trip

**Files:**
- Create: `plugin/src/model.rs`
- Modify: `plugin/src/bin/standalone.rs`
- Create: `plugin/weights/ba283_gru32.json` (generate from a small test model)
- Modify: `train/verify_roundtrip.py`
- Modify: `plugin/src/lib.rs` (wire up model)
- Create: `plugin/tests/test_model.rs`

### Step 3.2.1: Generate a placeholder weights JSON

Run:
```bash
uv run python -c "
import torch
from pathlib import Path
from train.model import BA283GRU
from train.export import export_rtneural_json
m = BA283GRU(hidden=32)
torch.manual_seed(42)
Path('plugin/weights').mkdir(parents=True, exist_ok=True)
export_rtneural_json(m, Path('plugin/weights/ba283_gru32.json'), meta={'sample_rate':96000,'input_scale':2.0,'placeholder':True})
print('ok')
"
```

### Step 3.2.2: Write the failing test

```rust
// plugin/tests/test_model.rs
use ba283::model::Ba283Model;

#[test]
fn model_loads_from_embedded_json() {
    let _ = Ba283Model::new_from_embedded_weights();
}

#[test]
fn model_output_is_finite() {
    let mut model = Ba283Model::new_from_embedded_weights();
    let y = model.forward(&[0.123, 0.5]);
    assert!(y[0].is_finite());
}

#[test]
fn reset_zeros_state() {
    let mut model = Ba283Model::new_from_embedded_weights();
    for _ in 0..100 {
        let _ = model.forward(&[1.0, 0.5]);
    }
    model.reset();
    let mut peak = 0.0f32;
    for _ in 0..1000 {
        let v = model.forward(&[0.0, 0.5])[0].abs();
        if v > peak { peak = v; }
    }
    assert!(peak < 1e-3, "peak {peak} too large after reset");
}
```

### Step 3.2.3: Run test to verify it fails

Run: `cd plugin && cargo test --test test_model`
Expected: compile error.

### Step 3.2.4: Implement `plugin/src/model.rs`

Consult rtneural-rs API at impl time. Sketch:

```rust
// plugin/src/model.rs
use rtneural::{ModelT, GRULayerT, DenseT};

pub type Ba283Model = ModelT<f32, 2, 1, (GRULayerT<f32, 2, 32>, DenseT<f32, 32, 1>)>;

const WEIGHTS_JSON: &str = include_str!("../weights/ba283_gru32.json");

pub trait Ba283ModelExt {
    fn new_from_embedded_weights() -> Self;
    fn forward(&mut self, x: &[f32; 2]) -> [f32; 1];
    fn reset(&mut self);
}

impl Ba283ModelExt for Ba283Model {
    fn new_from_embedded_weights() -> Self {
        let mut m = Self::default();
        m.load_from_json(WEIGHTS_JSON).expect("embedded weights must parse");
        m
    }

    fn forward(&mut self, x: &[f32; 2]) -> [f32; 1] {
        let mut out = [0.0f32; 1];
        self.forward_into(x, &mut out);
        out
    }

    fn reset(&mut self) {
        ModelT::reset(self);
    }
}
```

**Note:** rtneural-rs's actual public API differs; adapt method names by reading its `src/lib.rs`. The three tests in 3.2.2 are the contract.

Expose `pub mod model;` in `lib.rs`.

### Step 3.2.5: Run tests

Run: `cd plugin && cargo test --release`
Expected: 5 PASS (params 2 + model 3).

### Step 3.2.6: Wire model into plugin

Update `lib.rs` to hold `models: [Ba283Model; 2]`, initialize them in `initialize`, reset in `reset`, replace identity `process` body with:

```rust
fn process(
    &mut self,
    buffer: &mut Buffer,
    _aux: &mut AuxiliaryBuffers,
    _context: &mut impl ProcessContext<Self>,
) -> ProcessStatus {
    for mut frame in buffer.iter_samples() {
        let bias = self.params.bias.smoothed.next();
        for (ch, sample) in frame.iter_mut().enumerate() {
            let dry = *sample;
            let wet = self.models[ch].forward(&[dry, bias])[0];
            *sample = wet;
        }
    }
    ProcessStatus::Normal
}
```

(Drive / Output / Mix wired in Task 3.4.)

### Step 3.2.7: Build the standalone binary

Update `plugin/src/bin/standalone.rs`:

```rust
use ba283::Ba283Plugin;
use nih_plug::prelude::*;

fn main() {
    nih_export_standalone::<Ba283Plugin>();
}
```

Run: `cargo build --release --bin ba283_standalone`
Expected: builds.

### Step 3.2.8: Extend `train/verify_roundtrip.py`

Modify the script to:
1. Render a 1000-sample sine to `/tmp/rt_in.wav`.
2. `subprocess.check_call(["target/release/ba283_standalone", "--input", "/tmp/rt_in.wav", "--output", "/tmp/rt_out.wav"])`.
3. Read both PyTorch reference and Rust output.
4. Assert `np.max(np.abs(ref - rust)) < 1e-5`.

### Step 3.2.9: Run round-trip

Run: `uv run python -m train.verify_roundtrip`
Expected: prints `max diff = ...`, exits 0 if `< 1e-5`. On larger diff → JSON layout wrong, debug via @superpowers:systematic-debugging.

### Step 3.2.10: Commit

```bash
git add plugin/src/model.rs plugin/src/lib.rs plugin/src/bin/standalone.rs plugin/weights/ba283_gru32.json plugin/tests/test_model.rs train/verify_roundtrip.py Cargo.lock
git commit -m "feat(plugin): rtneural GRU(2,32)+Dense(1) wired with round-trip vs PyTorch"
```

## Task 3.3: 2× polyphase oversampling

**Files:**
- Create: `plugin/src/oversample.rs`
- Create: `plugin/tests/test_oversample.rs`
- Create: `plugin/tests/common/mod.rs`
- Create: `tools/gen_fir_coeffs.py`

### Step 3.3.1: Generate coefficients

Write `tools/gen_fir_coeffs.py`:

```python
import numpy as np
from scipy.signal import firwin

n = 63
beta = 8.6
cutoff = 0.5
h = firwin(n, cutoff, window=("kaiser", beta), pass_zero="lowpass")
h *= 2.0
print("const COEFFS: [f32; 63] = [")
for c in h:
    print(f"    {c:.10e},")
print("];")
```

Run: `uv run python tools/gen_fir_coeffs.py > /tmp/coeffs.rs`
Inspect, paste into `plugin/src/oversample.rs` in step 3.3.4.

### Step 3.3.2: Write the failing test

```rust
// plugin/tests/test_oversample.rs
mod common;

use ba283::oversample::Oversample2x;

#[test]
fn upsample_dc_returns_constant() {
    let mut os = Oversample2x::new();
    let (mut last_a, mut last_b) = (0.0, 0.0);
    for _ in 0..200 {
        let (a, b) = os.upsample(1.0);
        last_a = a;
        last_b = b;
    }
    assert!((last_a - 1.0).abs() < 0.01);
    assert!((last_b - 1.0).abs() < 0.01);
}

#[test]
fn upsample_then_downsample_is_unity_after_latency() {
    let mut os = Oversample2x::new();
    let input: Vec<f32> = (0..1024).map(|i| ((i as f32) * 0.01).sin()).collect();
    let mut output = vec![0.0f32; 1024];
    for i in 0..1024 {
        let (u0, u1) = os.upsample(input[i]);
        output[i] = os.downsample(u0, u1);
    }
    let latency = 32;
    let mut max_err = 0.0f32;
    for i in latency..1024 {
        let e = (output[i] - input[i - latency]).abs();
        if e > max_err { max_err = e; }
    }
    assert!(max_err < 0.05, "max err {max_err}");
}

#[test]
fn no_heap_alloc_in_steady_state() {
    use assert_no_alloc::*;
    let mut os = Oversample2x::new();
    for _ in 0..64 { os.upsample(0.0); }
    assert_no_alloc(|| {
        for _ in 0..1024 {
            let (a, b) = os.upsample(0.123);
            let _ = os.downsample(a, b);
        }
    });
}
```

```rust
// plugin/tests/common/mod.rs
#[global_allocator]
static A: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;
```

### Step 3.3.3: Run test to verify it fails

Run: `cd plugin && cargo test --test test_oversample`
Expected: compile error.

### Step 3.3.4: Implement `plugin/src/oversample.rs`

Paste generated coefficients. Implement polyphase decomposition properly; tests in 3.3.2 are the contract.

```rust
// plugin/src/oversample.rs
const N: usize = 31;
const COEFFS: [f32; N] = [/* paste from tools/gen_fir_coeffs.py */];

pub struct Oversample2x {
    delay: [f32; N],
    idx: usize,
}

impl Oversample2x {
    pub const fn new() -> Self {
        Self { delay: [0.0; N], idx: 0 }
    }

    pub fn upsample(&mut self, x: f32) -> (f32, f32) {
        // Polyphase: even-indexed taps process the real sample; odd-indexed taps
        // process the zero-stuffed sample. Implement two passes through the delay
        // line, writing `x` then `0.0` between them, and accumulate the two
        // filtered outputs.
        unimplemented!()
    }

    pub fn downsample(&mut self, y0: f32, _y1: f32) -> f32 {
        // After upsample()+model() the signal is already band-limited; pick on-grid
        // sample. (Or run a second FIR pass for symmetric anti-imaging — pick whichever
        // strategy passes the unity test in 3.3.2.)
        y0
    }
}

impl Default for Oversample2x {
    fn default() -> Self { Self::new() }
}
```

Expose `pub mod oversample;` in `lib.rs`.

### Step 3.3.5: Run tests

Run: `cd plugin && cargo test --release`
Expected: all green.

### Step 3.3.6: Wire OS into the process block

Update `process` in `lib.rs`:

```rust
let host_sr_ge_96k = self.sample_rate >= 96_000.0;
for mut frame in buffer.iter_samples() {
    let bias = self.params.bias.smoothed.next();
    for (ch, sample) in frame.iter_mut().enumerate() {
        let dry = *sample;
        let wet = if host_sr_ge_96k {
            self.models[ch].forward(&[dry, bias])[0]
        } else {
            let (u0, u1) = self.os[ch].upsample(dry);
            let w0 = self.models[ch].forward(&[u0, bias])[0];
            let w1 = self.models[ch].forward(&[u1, bias])[0];
            self.os[ch].downsample(w0, w1)
        };
        *sample = wet;
    }
}
```

Cache `self.sample_rate` from `BufferConfig.sample_rate` in `initialize`. Override:

```rust
fn latency_samples(&self) -> u32 {
    if self.sample_rate < 96_000.0 { 32 } else { 0 }
}
```

### Step 3.3.7: Commit

```bash
git add plugin/src/oversample.rs plugin/tests/test_oversample.rs plugin/tests/common/mod.rs tools/gen_fir_coeffs.py plugin/src/lib.rs
git commit -m "feat(plugin): 31-tap polyphase 2x oversampling with 8-sample latency report"
```

## Task 3.4: Wire Drive / Output / Mix + dry delay

**Files:**
- Modify: `plugin/src/lib.rs`
- Create: `plugin/src/delay.rs`
- Create: `plugin/tests/test_process.rs`

### Step 3.4.1: Implement `plugin/src/delay.rs`

```rust
// plugin/src/delay.rs
pub struct ShortDelay<const N: usize> {
    buf: [f32; N],
    idx: usize,
}

impl<const N: usize> ShortDelay<N> {
    pub const fn new() -> Self {
        Self { buf: [0.0; N], idx: 0 }
    }

    pub fn push_pop(&mut self, x: f32) -> f32 {
        let out = self.buf[self.idx];
        self.buf[self.idx] = x;
        self.idx = (self.idx + 1) % N;
        out
    }

    pub fn reset(&mut self) {
        self.buf = [0.0; N];
        self.idx = 0;
    }
}
```

### Step 3.4.2: Write the failing test

```rust
// plugin/tests/test_process.rs
mod common;

use ba283::delay::ShortDelay;

#[test]
fn short_delay_returns_n_samples_later() {
    let mut d: ShortDelay<8> = ShortDelay::new();
    for _ in 0..8 { let _ = d.push_pop(0.0); }
    let out = d.push_pop(0.5);
    assert!(out.abs() < 1e-9);
    for _ in 0..7 { let _ = d.push_pop(0.0); }
    let delayed = d.push_pop(0.0);
    assert!((delayed - 0.5).abs() < 1e-6);
}

#[test]
fn db_to_gain_correct() {
    fn db_to_gain(db: f32) -> f32 { 10.0f32.powf(db * 0.05) }
    assert!((db_to_gain(0.0) - 1.0).abs() < 1e-6);
    assert!((db_to_gain(6.02) - 2.0).abs() < 0.01);
    assert!((db_to_gain(-6.02) - 0.5).abs() < 0.01);
}
```

### Step 3.4.3: Implement the full process body

```rust
// inside process()
fn db_to_gain(db: f32) -> f32 { 10.0f32.powf(db * 0.05) }

for mut frame in buffer.iter_samples() {
    let drive = db_to_gain(self.params.drive_db.smoothed.next());
    let bias  = self.params.bias.smoothed.next();
    let out_g = db_to_gain(self.params.output_db.smoothed.next());
    let mix   = self.params.mix.smoothed.next() * 0.01;

    for (ch, sample) in frame.iter_mut().enumerate() {
        let dry = *sample;
        let driven = dry * drive;

        let wet = if self.host_sr_ge_96k {
            self.models[ch].forward(&[driven, bias])[0]
        } else {
            let (u0, u1) = self.os[ch].upsample(driven);
            let w0 = self.models[ch].forward(&[u0, bias])[0];
            let w1 = self.models[ch].forward(&[u1, bias])[0];
            self.os[ch].downsample(w0, w1)
        } * out_g;

        let dry_aligned = self.dry_delay[ch].push_pop(dry);
        *sample = wet * mix + dry_aligned * (1.0 - mix);
    }
}
```

Hook `self.dry_delay: [ShortDelay<16>; 2]`, zeroed in `reset()`.

### Step 3.4.4: Run all plugin tests

Run: `cd plugin && cargo test --release`
Expected: all green.

### Step 3.4.5: Commit

```bash
git add plugin/src/lib.rs plugin/src/delay.rs plugin/tests/test_process.rs
git commit -m "feat(plugin): wire Drive/Output/Mix params + dry-delay phase alignment"
```

## Task 3.5: Phase 3 gate check

> **DONE.** The nih_plug `nih_plug_xtask` bundler is set up: `xtask/` workspace
> member (Cargo.toml + src/main.rs calling `nih_plug_xtask::main()`, pinned to
> the same nih_plug rev `f36931f7af4646065488a9845d8f8c2f95252c23`), root
> `bundler.toml` mapping `[ba283] name = "BA283"`, and a `.cargo/config.toml`
> alias `xtask = "run --package xtask --release --"`. `cargo xtask bundle ba283
> --release` produces `target/bundled/BA283.vst3/Contents/x86_64-linux/BA283.so`
> (no manual fallback needed; the xtask API worked unmodified at this rev — at
> this rev it emits no moduleinfo.json, which is optional for VST3). The render
> smoke uses the offline `ba283_standalone` custom renderer (not the nih_plug
> standalone audio backend), so the gate runs headless without an audio device.
> Gate passed: 13 cargo tests green, clippy clean, fmt clean, standalone renders
> sr=96000 non-silent finite WAV, round-trip max diff 2.6e-08 < 1e-5.

### Step 3.5.1: Build VST3 bundle

Run: `cargo xtask bundle ba283 --release`
(If xtask not configured, fall back to `cargo build --release` and link `target/release/libba283.so` into a `.vst3` directory per nih_plug docs.)

Expected: `target/bundled/BA283.vst3/` exists.

### Step 3.5.2: Smoke standalone

Run:
```bash
uv run python -c "
import numpy as np, soundfile as sf
sr = 96_000
t = np.arange(sr) / sr
x = (0.3 * np.sin(2*np.pi*1000*t)).astype(np.float32)
sf.write('/tmp/in.wav', x, sr, subtype='FLOAT')
"
cargo run --release --bin ba283_standalone -- --input /tmp/in.wav --output /tmp/out.wav --bias 0.5
```
Expected: writes non-empty, non-silent `/tmp/out.wav`. Adapt CLI flags to whatever nih_plug standalone supports.

### Step 3.5.3: Verify with Python

Run:
```bash
uv run python -c "
import soundfile as sf, numpy as np
y, sr = sf.read('/tmp/out.wav')
print('sr', sr, 'len', len(y), 'rms', float(np.sqrt(np.mean(y**2))))
"
```
Expected: sr=96000, non-zero RMS.

### Step 3.5.4: Commit gate

```bash
git commit --allow-empty -m "chore: pass Phase 3 gate (plugin builds, standalone renders)"
```

---

# Phase 4 — Validation

## Task 4.1: Numerical null test

**Files:**
- Create: `tests/verify_numerical.py`

### Step 4.1.1: Write the script

```python
# tests/verify_numerical.py
import argparse
import subprocess
import sys
from pathlib import Path

import numpy as np
import soundfile as sf
from scipy.signal import stft


PLUGIN_BIN = "target/release/ba283_standalone"


def render_plugin(in_wav: Path, out_wav: Path, bias: float) -> None:
    subprocess.check_call([
        PLUGIN_BIN,
        "--input", str(in_wav),
        "--output", str(out_wav),
        "--bias", f"{bias:.4f}",
        "--drive-db", "0",
        "--output-db", "0",
        "--mix", "100",
    ])


def render_spice(in_wav: Path, out_wav: Path, bias: float) -> None:
    subprocess.check_call([
        sys.executable, "-m", "data_gen.run_spice",
        "--single", str(in_wav),
        "--bias", f"{bias:.4f}",
        "--out", str(out_wav),
    ])


def null_depth_db(spice: np.ndarray, plugin: np.ndarray) -> float:
    n = min(len(spice), len(plugin))
    spice = spice[:n]
    plugin = plugin[:n]
    head = min(n, 2 * 96_000)
    corr = np.correlate(spice[:head], plugin[:head], mode="full")
    lag = int(corr.argmax() - (head - 1))
    if lag > 0:
        plugin = plugin[lag:]
        spice = spice[:-lag] if lag <= len(spice) else spice
    elif lag < 0:
        spice = spice[-lag:]
        plugin = plugin[:lag] if -lag <= len(plugin) else plugin
    n = min(len(spice), len(plugin))
    null = spice[:n] - plugin[:n]
    rms_null = np.sqrt(np.mean(null ** 2) + 1e-20)
    rms_ref = np.sqrt(np.mean(spice[:n] ** 2) + 1e-20)
    return 20 * np.log10(rms_null / rms_ref)


def worst_third_octave_db(spice, plugin, sr=96_000):
    n = min(len(spice), len(plugin))
    null = spice[:n] - plugin[:n]
    f, _, Z_null = stft(null, fs=sr, nperseg=8192)
    _, _, Z_ref = stft(spice[:n], fs=sr, nperseg=8192)
    p_null = np.mean(np.abs(Z_null) ** 2, axis=1)
    p_ref = np.mean(np.abs(Z_ref) ** 2, axis=1) + 1e-20
    ratio_db = 10 * np.log10(p_null / p_ref)
    bands = []
    f_c = 31.25
    while f_c < 16_000:
        f_lo = f_c / (2 ** (1 / 6))
        f_hi = f_c * (2 ** (1 / 6))
        mask = (f >= f_lo) & (f < f_hi)
        if mask.any():
            bands.append((f_c, float(ratio_db[mask].max())))
        f_c *= 2 ** (1 / 3)
    return bands


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--stimuli", nargs="+", required=True)
    ap.add_argument("--bias", type=float, default=0.5)
    ap.add_argument("--target-db", type=float, default=-22.0)
    args = ap.parse_args()

    failed = False
    for stim in args.stimuli:
        p = Path(stim)
        spice_out = Path(f"/tmp/spice_{p.stem}.wav")
        plugin_out = Path(f"/tmp/plugin_{p.stem}.wav")
        render_spice(p, spice_out, args.bias)
        render_plugin(p, plugin_out, args.bias)
        spice = sf.read(spice_out, dtype="float32")[0]
        plugin = sf.read(plugin_out, dtype="float32")[0]
        depth = null_depth_db(spice, plugin)
        bands = worst_third_octave_db(spice, plugin)
        worst_band = max(bands, key=lambda b: b[1])
        print(f"{p.name}: broadband {depth:+.2f} dB, worst band {worst_band}")
        if depth > args.target_db:
            failed = True
        if worst_band[1] > -15.0:
            failed = True
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
```

### Step 4.1.2: Commit

```bash
git add tests/verify_numerical.py
git commit -m "test(validation): numerical null vs SPICE with 1/3-octave check"
```

## Task 4.2: Bias-sweep coverage test

**Files:**
- Create: `tests/verify_bias_sweep.py`

### Step 4.2.1: Write the script

Same shape as 4.1, but loop over `bias ∈ [0.0, 0.125, 0.25, 0.375, 0.5, 0.625, 0.75, 0.875, 1.0]`. Print a table. Exit non-zero on any trained-point null > −22 dB or interpolated-point null > −18 dB.

### Step 4.2.2: Commit

```bash
git add tests/verify_bias_sweep.py
git commit -m "test(validation): bias-sweep coverage vs SPICE"
```

## Task 4.3: Realtime + latency tests

**Files:**
- Create: `tests/verify_latency.py`
- Create: `tests/verify_realtime.py`
- Create: `tests/verify_xrun.sh`

### Step 4.3.1: `verify_latency.py`

```python
# tests/verify_latency.py
import subprocess
import numpy as np
import soundfile as sf


def test_impulse_latency():
    sr = 48_000
    n = sr
    x = np.zeros(n, dtype=np.float32)
    x[100] = 1.0
    sf.write("/tmp/imp_in.wav", x, sr, subtype="FLOAT")
    subprocess.check_call([
        "target/release/ba283_standalone",
        "--input", "/tmp/imp_in.wav",
        "--output", "/tmp/imp_out.wav",
        "--bias", "0.5", "--drive-db", "0", "--output-db", "0", "--mix", "100",
    ])
    y, _ = sf.read("/tmp/imp_out.wav", dtype="float32")
    peak = int(np.argmax(np.abs(y)))
    latency = peak - 100
    assert 6 <= latency <= 10, f"unexpected latency {latency}"


if __name__ == "__main__":
    test_impulse_latency()
    print("ok")
```

### Step 4.3.2: `verify_realtime.py`

Requires plugin built with `assert-no-alloc` feature flag wrapping `process` in `assert_no_alloc!`. Add to `plugin/Cargo.toml`:

```toml
[features]
assert-no-alloc = ["dep:assert_no_alloc"]
```

(Move `assert_no_alloc` from dev-dependencies to optional regular dependency.)

Then `cargo build --release --features assert-no-alloc` and run standalone over 60 s of audio. Capture stderr for any "allocation occurred" panic. Exit non-zero on any.

### Step 4.3.3: `verify_xrun.sh`

```bash
#!/usr/bin/env bash
set -euo pipefail
chrt -f 85 ./target/release/ba283_standalone --backend jack --bias 0.5 --duration 600 --report-callback-times /tmp/callback_times.csv
WORST=$(uv run python -c "
import pandas as pd
df = pd.read_csv('/tmp/callback_times.csv')
print(int(df['us'].max()))
")
echo "worst callback us = $WORST"
test "$WORST" -lt 333
```

### Step 4.3.4: Commit

```bash
chmod +x tests/verify_xrun.sh
git add tests/verify_latency.py tests/verify_realtime.py tests/verify_xrun.sh plugin/Cargo.toml
git commit -m "test(validation): latency + realtime allocation + xrun guards"
```

## Task 4.4: Ableton Live null protocol doc

**Files:**
- Create: `docs/null_test_ableton.md`

### Step 4.4.1: Write the doc

Reproduce Section 5.5 of the design doc verbatim in a standalone markdown file.

### Step 4.4.2: Commit

```bash
git add docs/null_test_ableton.md
git commit -m "docs(validation): Ableton null test protocol"
```

## Task 4.5: Regression suite

**Files:**
- Create: `tests/regression/run.sh`
- Create: `.gitattributes`

### Step 4.5.1: Configure Git LFS

```bash
git lfs install
echo "tests/regression/refs/*.wav filter=lfs diff=lfs merge=lfs -text" >> .gitattributes
```

### Step 4.5.2: Write the script

```bash
#!/usr/bin/env bash
# tests/regression/run.sh
set -euo pipefail
REFS=tests/regression/refs
TOL=1e-5
for stim in $REFS/*_dry.wav; do
    base=$(basename "$stim" _dry.wav)
    out=/tmp/${base}_plugin.wav
    target/release/ba283_standalone --input "$stim" --output "$out" --bias 0.5 --drive-db 0 --output-db 0 --mix 100
    uv run python -c "
import numpy as np, soundfile as sf, sys
ref = sf.read('$REFS/${base}_ref.wav', dtype='float32')[0]
cur = sf.read('$out', dtype='float32')[0]
n = min(len(ref), len(cur))
d = np.max(np.abs(ref[:n] - cur[:n]))
print('$base diff =', d)
sys.exit(0 if d < $TOL else 1)
"
done
```

### Step 4.5.3: Commit

```bash
chmod +x tests/regression/run.sh
git add tests/regression/run.sh .gitattributes
git commit -m "test(regression): byte-compare plugin output against committed refs (LFS)"
```

## Task 4.6: Phase 4 gate check

### Step 4.6.1: Run numerical null

Run:
```bash
uv run python tests/verify_numerical.py \
    --stimuli data/splits/test_files/*.wav \
    --bias 0.5 --target-db -22
```
Expected: exit 0.

### Step 4.6.2: Run bias sweep

Run: `uv run python tests/verify_bias_sweep.py --stimuli data/splits/test_files/*.wav`
Expected: exit 0.

### Step 4.6.3: Run latency

Run: `uv run python tests/verify_latency.py`
Expected: `ok`.

### Step 4.6.4: Run realtime

Run: `uv run python tests/verify_realtime.py`
Expected: exit 0, no allocation panic.

### Step 4.6.5: Manual DAW null

Follow `docs/null_test_ableton.md`. Confirm SPAN shows broadband residual ≤ −25 dB and no octave > −15 dB.

### Step 4.6.6: Phase 4 gate verification

@superpowers:verification-before-completion. Confirm:
- Numerical null script exits 0.
- Bias sweep exits 0.
- Latency in expected range.
- Zero allocations in process block.
- DAW null visually ≤ −25 dB.

### Step 4.6.7: Commit + tag

```bash
git commit --allow-empty -m "chore: pass Phase 4 gate (validation green)"
git tag v0.1.0-rc1
```

---

# Phase 5 — Release Polish

## Task 5.1: README

**Files:**
- Create: `README.md`

### Step 5.1.1: Write

Cover: project goal, prerequisites (ngspice, Rust toolchain, uv), data-gen → train → build flow, link to design doc + training runbook + Ableton doc, license placeholder.

### Step 5.1.2: Commit

```bash
git add README.md
git commit -m "docs: top-level README"
```

## Task 5.2: CI

**Files:**
- Create: `.github/workflows/ci.yml`

### Step 5.2.1: Write workflow

Jobs:
1. **python-tests**: `uv sync && uv run pytest tests/data_gen tests/train -v` (guard ngspice-dependent tests with env var if CI runner lacks ngspice).
2. **rust-build**: `cargo test --release` and `cargo clippy --release -- -D warnings`.
3. **regression**: download LFS refs, run `tests/regression/run.sh`.

### Step 5.2.2: Commit

```bash
git add .github/workflows/ci.yml
git commit -m "ci: python + rust + regression jobs"
```

---

## Summary

| Phase | Tasks | Approximate effort |
|-------|-------|--------------------|
| 0 — Bootstrap | 3 | 30 min |
| 1 — Data gen | 5 | 1 day code + several hours wall-clock for full SPICE batch |
| 2 — Training | 6 | 1 day code + 2–4 hours wall-clock for full train |
| 3 — Plugin | 5 | 2 days |
| 4 — Validation | 6 | 1 day + manual DAW session |
| 5 — Release | 2 | half day |

Each task is TDD (failing test → impl → passing test → commit). Phase gates block advancement. Audio numerical assertions are deterministic on small fixtures; full-scale runs are gated by separate "smoke" verifications that don't block CI.

When stuck: @superpowers:systematic-debugging. Before declaring any gate complete: @superpowers:verification-before-completion.
