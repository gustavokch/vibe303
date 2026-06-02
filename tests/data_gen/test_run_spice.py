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


def test_bias_changes_output(tmp_path):
    out_lo = simulate_one(dry_wav=FIXTURE, bias=0.0, out_npz=tmp_path / "lo.npz")
    out_hi = simulate_one(dry_wav=FIXTURE, bias=1.0, out_npz=tmp_path / "hi.npz")
    v_lo = np.load(out_lo)["v"]
    v_hi = np.load(out_hi)["v"]
    # different bias -> materially different wet output (catches a bias-ignored regression)
    n = min(len(v_lo), len(v_hi))
    assert not np.allclose(v_lo[:n], v_hi[:n], atol=1e-3)
