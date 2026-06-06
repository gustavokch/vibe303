import numpy as np
import pytest

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


def test_esr_resampled_equal_rate_matches_esr():
    rng = np.random.default_rng(0)
    x = rng.standard_normal(4096)
    assert esr_resampled(x, x, sr_from=44100, sr_to=44100) == esr(x, x)


def test_esr_resampled_antialias_removes_supernyquist():
    # target: 5 kHz sine at 44.1k. pred: same 5 kHz sine PLUS a 40 kHz tone
    # at 88.2k (valid below the 44.1k Nyquist of 88.2k, but above 44.1k's
    # 22.05 kHz Nyquist). The AA filter must reject the 40 kHz content; naive
    # decimation would alias it to ~4.1 kHz and blow up the ESR.
    sr_lo, sr_hi = 44100, 88200
    n_lo, n_hi = sr_lo, sr_hi  # 1 second each
    t_lo = np.arange(n_lo) / sr_lo
    t_hi = np.arange(n_hi) / sr_hi
    target = np.sin(2 * np.pi * 5_000 * t_lo)
    pred = np.sin(2 * np.pi * 5_000 * t_hi) + np.sin(2 * np.pi * 40_000 * t_hi)
    assert esr_resampled(pred, target, sr_from=sr_hi, sr_to=sr_lo) < 1e-2


def test_esr_raises_on_empty():
    with pytest.raises(ValueError):
        esr(np.array([]), np.array([]))


def test_esr_raises_on_nonfinite():
    target = np.array([1.0, 2.0, 3.0])
    with pytest.raises(ValueError):
        esr(np.array([np.nan, 2.0, 3.0]), target)
    with pytest.raises(ValueError):
        esr(np.array([np.inf, 2.0, 3.0]), target)
