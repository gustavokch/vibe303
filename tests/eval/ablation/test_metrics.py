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
