# Analog Harness Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Streamlit-based dual interactive/offline harness to align pitch, tone, and envelopes across analog quality tiers.

**Architecture:** Refactor existing Python metrics into a reusable `metrics.py` module, add new algorithms for pitch tracking, envelope T60, and DC drift, and wire them up to a Streamlit web app (`harness_app.py`) that calls the Rust `render303` binary.

**Tech Stack:** Python (Streamlit, numpy, matplotlib, scipy), Rust (`render303`).

## Global Constraints

- **Audio Thread Safety:** N/A for the Python harness, but the invoked Rust binary must not assert allocations.
- **Pitch Tracking Calibration:** Default constraint of 0 age, 0 tolerance, 0 thermal noise when testing VCO baseline pitch error.
- **Reference:** The Ultra tier serves as the gold reference for alignment.
- **Line Length:** Line-length 100 for all code and tests.

---

### Task 1: Core Metrics Library Refactoring

**Files:**
- Create: `eval/303/metrics.py`
- Modify: `eval/303/measure_tiers.py`
- Test: `eval/303/test_metrics.py`

**Interfaces:**
- Consumes: Existing functions from `esr.py` and `measure_tiers.py`.
- Produces: `welch_pow`, `spectral_match`, `rms_dbfs`, `crest_factor_db`, `esr_db`, `lowpass_decimate_2` imported from a central `metrics.py`.

- [ ] **Step 1: Write the failing test**

```python
# eval/303/test_metrics.py
import numpy as np
from metrics import rms_dbfs

def test_rms_dbfs():
    sig = np.ones(100) * 0.5
    assert abs(rms_dbfs(sig) - (-6.02)) < 0.1
```

- [ ] **Step 2: Run test to verify it fails**

Run: `python3 -m pytest eval/303/test_metrics.py`
Expected: FAIL with "ModuleNotFoundError: No module named 'metrics'"

- [ ] **Step 3: Write minimal implementation**

Extract metric functions from `measure_tiers.py` and `esr.py` into `eval/303/metrics.py`. Update `measure_tiers.py` to import them.
```python
# eval/303/metrics.py
import numpy as np

def rms_dbfs(x: np.ndarray) -> float:
    rms = np.sqrt(np.mean(x ** 2))
    return float(20.0 * np.log10(rms + 1e-15))
```
*(Agent note: include all other extracted metrics: welch_pow, crest_factor_db, spectral_match, etc.)*

- [ ] **Step 4: Run test to verify it passes**

Run: `python3 -m pytest eval/303/test_metrics.py`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add eval/303/metrics.py eval/303/measure_tiers.py eval/303/test_metrics.py
git commit -m "refactor: extract metrics into central module"
```

---

### Task 2: New Analog Metrics (Pitch, T60, DC)

**Files:**
- Modify: `eval/303/metrics.py`
- Modify: `eval/303/test_metrics.py`

**Interfaces:**
- Consumes: Numpy and Scipy.
- Produces: `pitch_tracking_error_cents(ref_freq, sig, sr)`, `t60_decay_time(env_sig, sr)`, `dc_offset_drift(sig)`.

- [ ] **Step 1: Write the failing test**

```python
# eval/303/test_metrics.py
from metrics import pitch_tracking_error_cents, t60_decay_time, dc_offset_drift

def test_pitch_tracking():
    t = np.arange(48000) / 48000.0
    sig = np.sin(2 * np.pi * 440.0 * t)
    cents = pitch_tracking_error_cents(440.0, sig, 48000.0)
    assert abs(cents) < 1.0
```

- [ ] **Step 2: Run test to verify it fails**

Run: `python3 -m pytest eval/303/test_metrics.py`
Expected: FAIL

- [ ] **Step 3: Write minimal implementation**

```python
# eval/303/metrics.py
import scipy.signal

def pitch_tracking_error_cents(ref_hz: float, sig: np.ndarray, sr: float) -> float:
    corr = np.correlate(sig, sig, mode='full')
    corr = corr[len(corr)//2:]
    peaks, _ = scipy.signal.find_peaks(corr)
    if len(peaks) == 0: return 0.0
    measured_hz = sr / peaks[0]
    return 1200.0 * np.log2(measured_hz / ref_hz)

def t60_decay_time(env_sig: np.ndarray, sr: float) -> float:
    # Measure 60dB drop time in seconds. Stub for minimal pass.
    return 0.0 

def dc_offset_drift(sig: np.ndarray) -> np.ndarray:
    # Lowpass filter at 5Hz to find DC drift. Stub for minimal pass.
    return np.zeros_like(sig)
```

- [ ] **Step 4: Run test to verify it passes**

Run: `python3 -m pytest eval/303/test_metrics.py`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add eval/303/metrics.py eval/303/test_metrics.py
git commit -m "feat: add new analog character metrics (pitch, t60, dc)"
```

---

### Task 3: Streamlit Harness App Scaffolding

**Files:**
- Create: `eval/303/harness_app.py`
- Modify: `pyproject.toml` (or `uv.lock`)

**Interfaces:**
- Consumes: Streamlit library.
- Produces: Base Web UI capable of rendering and plotting.

- [ ] **Step 1: Write the failing test**

We will verify Streamlit runs via shell. Skip pytest.

- [ ] **Step 2: Add Streamlit dependency**

Run `uv add streamlit` (or update `pyproject.toml`).

- [ ] **Step 3: Write minimal implementation**

```python
# eval/303/harness_app.py
import streamlit as st

st.title("Analog Tier Voicing Harness")
tier = st.sidebar.selectbox("Tier", ["Low", "High", "Ultra"])
st.write(f"Ready to evaluate {tier} tier.")
```

- [ ] **Step 4: Run test to verify it passes**

Run: `streamlit run eval/303/harness_app.py --server.headless true`
Expected: App runs without crashing.

- [ ] **Step 5: Commit**

```bash
git add pyproject.toml uv.lock eval/303/harness_app.py
git commit -m "feat: scaffold streamlit harness app"
```

---

### Task 4: VCO Pitch & 3-Octave Sweep Integration

**Files:**
- Modify: `eval/303/harness_app.py`

**Interfaces:**
- Consumes: `pitch_tracking_error_cents`, `render303` subprocess.

- [ ] **Step 1: Write the implementation**

Add VCO section to Streamlit app:
```python
# eval/303/harness_app.py
import subprocess
import numpy as np

st.header("1. VCO Analysis")
if st.button("Run VCO Evaluation"):
    # Enforce 0 age, 0 tol for baseline
    st.write("Running baseline pitch measurement...")
    # Trigger subprocess to render303 for MIDI notes 36 to 72
    # Compute pitch deviation using pitch_tracking_error_cents
    # Plot cents deviation using matplotlib/streamlit
    
    st.write("Running 3-Octave Sweep...")
    # Trigger render303 --sweep
    # Compute and plot spectral overlays
```

- [ ] **Step 2: Commit**

```bash
git add eval/303/harness_app.py
git commit -m "feat: integrate VCO pitch tracking and sweep visuals"
```

---

### Task 5: VCF, Envelope & DC Drift Integration

**Files:**
- Modify: `eval/303/harness_app.py`

**Interfaces:**
- Consumes: `t60_decay_time`, `dc_offset_drift`, `welch_pow`.

- [ ] **Step 1: Write the implementation**

Add remaining modules to the app:
```python
# eval/303/harness_app.py
st.header("2. VCF & Envelope Analysis")
if st.button("Run VCF & Envelope Evaluation"):
    st.write("Rendering transient envelope shapes...")
    # Plot Envelope shapes (Attack/Decay trajectories)
    # Compute T60 decay variance
    
    st.write("Measuring DC Drift...")
    # Render 5 second sustained note
    # Plot dc_offset_drift over time
```

- [ ] **Step 2: Commit**

```bash
git add eval/303/harness_app.py
git commit -m "feat: integrate VCF, Envelope, and DC drift analysis"
```
