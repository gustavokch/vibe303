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
