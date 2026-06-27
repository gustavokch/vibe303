"""ESR / NRMSE metrics (BA283 methodology). Lag-aligned error-to-signal ratio."""
from __future__ import annotations
import numpy as np


def best_lag(ref: np.ndarray, mod: np.ndarray, max_lag: int = 256) -> int:
    n = min(len(ref), len(mod))
    r, m = ref[:n] - ref[:n].mean(), mod[:n] - mod[:n].mean()
    corr = np.correlate(r, m, mode="full")
    lags = np.arange(-(n - 1), n)
    w = np.abs(lags) <= max_lag
    return int(lags[w][np.argmax(corr[w])])


def _align(ref, mod):
    lag = best_lag(ref, mod)
    if lag > 0:
        ref, mod = ref[lag:], mod[: len(ref) - lag]
    elif lag < 0:
        mod, ref = mod[-lag:], ref[: len(mod) + lag]
    n = min(len(ref), len(mod))
    return ref[:n], mod[:n]


def esr_db(esr: float) -> float:
    return float(10.0 * np.log10(esr + 1e-30))


def metrics(ref: np.ndarray, mod: np.ndarray) -> dict:
    ref = np.asarray(ref, float)
    mod = np.asarray(mod, float)
    lag = best_lag(ref, mod)
    r, m = _align(ref, mod)
    err = r - m
    esr = float(np.sum(err**2) / max(np.sum(r**2), 1e-20))
    nrmse = float(np.sqrt(np.mean(err**2)) / max(r.max() - r.min(), 1e-20))
    return {"lag": lag, "esr": esr, "esr_db": esr_db(esr), "nrmse": nrmse}


def pre_emphasis(x: np.ndarray, alpha: float = 0.8613) -> np.ndarray:
    """Perceptual HF weighting (matched-z of the paper's 0.85 @ 48 kHz)."""
    y = np.empty_like(x)
    y[0] = x[0]
    y[1:] = x[1:] - alpha * x[:-1]
    return y
