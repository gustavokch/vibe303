"""Pure-numpy fidelity metrics for the BA283 model ablation study.

No torch, no GPU. scipy is used only for polyphase anti-aliased resampling.
"""

from math import gcd

import numpy as np
from scipy.signal import resample_poly


def resample_to(sig: np.ndarray, *, sr_from: int, sr_to: int) -> np.ndarray:
    """Polyphase-resample ``sig`` from ``sr_from`` to ``sr_to`` (no-op if equal).

    The single source of truth for the anti-aliased rate conversion shared by
    :func:`esr_resampled` and the run-cell resample wrap, so both use the exact
    same scipy Kaiser-windowed polyphase path (gcd-reduced up/down factors).
    """
    sig = np.asarray(sig, dtype=np.float64)
    if sr_from == sr_to:
        return sig
    g = gcd(sr_from, sr_to)
    return resample_poly(sig, sr_to // g, sr_from // g)


def esr(pred: np.ndarray, target: np.ndarray, eps: float = 1e-12) -> float:
    """Error-to-signal ratio: sum((pred - target)**2) / sum(target**2).

    Inputs are truncated to their common length, so a few-sample length
    mismatch (e.g. from resampling edge effects) is handled gracefully.
    Returns 0.0 when ``pred == target``.

    ``eps`` is solely a 0/0 guard for the degenerate all-zero-target case; it
    is NOT a meaningful floor on the raw-volt domain (target energy is
    ~1e3-1e6). Callers pass energetic full-length val stems; near-silent inputs
    are not expected here (the dataset drops them upstream).
    """
    pred = np.asarray(pred, dtype=np.float64)
    target = np.asarray(target, dtype=np.float64)
    n = min(len(pred), len(target))
    if n == 0:
        raise ValueError("esr: empty signal")
    pred, target = pred[:n], target[:n]
    if not (np.isfinite(pred).all() and np.isfinite(target).all()):
        raise ValueError("esr: non-finite values in pred/target")
    return float(((pred - target) ** 2).sum() / ((target**2).sum() + eps))


def esr_resampled(
    pred: np.ndarray,
    target: np.ndarray,
    *,
    sr_from: int,
    sr_to: int,
    eps: float = 1e-12,
) -> float:
    """ESR after resampling ``pred`` from ``sr_from`` to ``sr_to``.

    Uses a polyphase anti-aliasing filter so 88.2k and 44.1k models are scored
    at a common rate (``sr_to``) and compare apples-to-apples. When the two rates
    are already equal this reduces to a plain :func:`esr` call.
    """
    # scipy's default Kaiser-windowed polyphase filter provides the anti-alias;
    # adequate as a fairness/comparison metric. Shared with run_cell's wrap.
    pred = resample_to(pred, sr_from=sr_from, sr_to=sr_to)
    return esr(pred, target, eps=eps)


def high_band_error(
    pred: np.ndarray,
    target: np.ndarray,
    *,
    sr: int,
    split_frac: float = 0.5,
    eps: float = 1e-12,
) -> float:
    """Relative error energy ABOVE split_frac * Nyquist (where folded aliases land).

    rFFT both signals, sum squared-magnitude error over bins >= split_frac*Nyquist,
    normalize by the target's high-band energy. Used to compare a 44.1k-trained model
    against an alias-minimized gold reference: lower = less aliasing.
    """
    if not 0.0 <= split_frac < 1.0:
        raise ValueError(f"high_band_error: split_frac must be in [0, 1), got {split_frac}")
    pred = np.asarray(pred, dtype=np.float64)
    target = np.asarray(target, dtype=np.float64)
    n = min(len(pred), len(target))
    if n == 0:
        raise ValueError("high_band_error: empty signal")
    pred, target = pred[:n], target[:n]
    if not (np.isfinite(pred).all() and np.isfinite(target).all()):
        raise ValueError("high_band_error: non-finite values in pred/target")
    # no window: spectral leakage is common-mode across pred/target and cancels in the C4 diff
    P = np.fft.rfft(pred)
    T = np.fft.rfft(target)
    freqs = np.fft.rfftfreq(n, 1.0 / sr)
    hi = freqs >= split_frac * (sr / 2)
    return float((np.abs(P[hi] - T[hi]) ** 2).sum() / ((np.abs(T[hi]) ** 2).sum() + eps))
