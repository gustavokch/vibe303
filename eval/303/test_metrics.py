import numpy as np
from metrics import (
    rms_dbfs,
    pitch_tracking_error_cents,
    t60_decay_time,
    dc_offset_drift
)

def test_rms_dbfs():
    sig = np.ones(100) * 0.5
    assert abs(rms_dbfs(sig) - (-6.02)) < 0.1

def test_pitch_tracking():
    t = np.arange(48000) / 48000.0
    sig = np.sin(2 * np.pi * 440.0 * t)
    cents = pitch_tracking_error_cents(440.0, sig, 48000.0)
    assert abs(cents) < 1.0

def test_t60_decay_time():
    # 60dB decay corresponds to a drop by a factor of 1000
    # Let tau be the time constant such that exp(-t/tau) = 0.001 at t=1.0s
    # tau = 1.0 / ln(1000) = 1.0 / 6.907
    sr = 48000.0
    t = np.arange(48000) / sr
    tau = 1.0 / np.log(1000.0)
    env = np.exp(-t / tau)
    t60 = t60_decay_time(env, sr)
    assert abs(t60 - 1.0) < 0.05

def test_dc_offset_drift():
    # Drift from 0 to 0.1 over 1s, with high frequency noise
    sr = 48000.0
    t = np.arange(48000) / sr
    drift = np.linspace(0, 0.1, len(t))
    noise = np.sin(2 * np.pi * 1000.0 * t) * 0.5
    sig = drift + noise
    max_drift = dc_offset_drift(sig)
    # The max drift should be around 0.1
    assert abs(np.max(max_drift) - 0.1) < 0.01
