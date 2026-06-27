import numpy as np
from esr import metrics, esr_db


def test_identical_signals_have_minus_inf_esr():
    x = np.sin(np.linspace(0, 100, 4096)).astype(np.float64)
    m = metrics(x, x.copy())
    assert m["esr_db"] < -120.0          # numerically ~ -inf
    assert abs(m["nrmse"]) < 1e-9


def test_known_error_ratio():
    x = np.sin(np.linspace(0, 100, 4096))
    e = 0.1 * np.sin(np.linspace(0, 137, 4096))   # uncorrelated-ish error
    m = metrics(x, x + e)
    # esr = sum(e^2)/sum(x^2); both unit-ish sines => ~ (0.1^2)/1 = 0.01 => -20 dB
    assert -23.0 < m["esr_db"] < -17.0


def test_lag_is_removed():
    x = np.sin(np.linspace(0, 200, 8192))
    shifted = np.roll(x, 17)             # pure delay must not count as error
    m = metrics(x, shifted)
    assert m["lag"] == 17 or m["lag"] == -17
    assert m["esr_db"] < -40.0
