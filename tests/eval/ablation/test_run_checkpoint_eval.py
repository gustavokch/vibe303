import numpy as np
import pandas as pd
import pytest
import soundfile as sf
import torch

from eval.ablation.run_checkpoint_eval import (
    build_rows,
    load_model,
    make_predict,
    run_cell,
)
from train.dataset import INPUT_SCALE, OUTPUT_SCALE
from train.model import BA283GRU


def test_load_model_roundtrips_state(tmp_path):
    src = BA283GRU(hidden=8)
    ckpt = tmp_path / "best.pth"
    torch.save(src.state_dict(), ckpt)

    loaded = load_model(ckpt, hidden=8)

    assert loaded.hidden == 8
    assert loaded.conditioning == "film"
    assert not loaded.training  # eval mode
    src_state = src.state_dict()
    loaded_state = loaded.state_dict()
    assert set(src_state) == set(loaded_state)
    for k in src_state:
        assert torch.equal(src_state[k], loaded_state[k])


class _FakeModel:
    """Records the forward input tensor and echoes its audio column."""

    def __init__(self):
        self.seen = None

    def __call__(self, x, h=None):
        self.seen = x
        return x[..., :1], None


def test_make_predict_scales_input_and_trims_warmup():
    warmup = 5
    fake = _FakeModel()
    predict = make_predict(fake, input_scale=INPUT_SCALE, warmup=warmup)

    dry = np.linspace(-0.3, 0.3, 20, dtype=np.float64)
    gain = 0.5
    out = predict(dry, gain)

    x = fake.seen
    assert x.shape == (1, 20, 2)
    np.testing.assert_allclose(x[0, :, 0].numpy(), dry * INPUT_SCALE, rtol=1e-6)
    np.testing.assert_allclose(x[0, :, 1].numpy(), gain, rtol=1e-6)
    assert out.ndim == 1
    assert len(out) == len(dry) - warmup
    # echo of scaled audio, post-warmup
    np.testing.assert_allclose(out, (dry * INPUT_SCALE)[warmup:], rtol=1e-6)


def _write_wav(path, data, sr=44100):
    sf.write(str(path), data.astype(np.float32), sr)


def test_build_rows_applies_output_scale_and_trims_target(tmp_path):
    warmup = 3
    rng = np.random.default_rng(7)
    dry = rng.standard_normal(32).astype(np.float64)
    wet = rng.standard_normal(32).astype(np.float64) * 40.0  # volt-domain
    dry_p = tmp_path / "dry.wav"
    wet_p = tmp_path / "wet.wav"
    _write_wav(dry_p, dry)
    _write_wav(wet_p, wet)

    manifest = pd.DataFrame(
        [
            {
                "dry_path": str(dry_p),
                "wet_path": str(wet_p),
                "gain_value": 0.5,
                "duration_s": 0.001,
                "stim_kind": "noise",
            }
        ]
    )

    rows = build_rows(manifest, output_scale=OUTPUT_SCALE, warmup=warmup)

    assert len(rows) == 1
    row_dry, gain, target = rows[0]
    # dry stays raw (unscaled) and full-length
    assert len(row_dry) == 32
    np.testing.assert_allclose(row_dry, sf.read(str(dry_p), dtype="float32")[0], rtol=1e-5)
    assert gain == 0.5
    # target is wet * output_scale with first `warmup` samples trimmed
    expected = (sf.read(str(wet_p), dtype="float32")[0] * OUTPUT_SCALE)[warmup:]
    np.testing.assert_allclose(target, expected, rtol=1e-5)
    assert len(target) == 32 - warmup


def _make_fixture(tmp_path, gains, *, n=4096, sr=44100):
    """Save a tiny BA283GRU checkpoint + a manifest of dry/wet wavs.

    Returns ``(checkpoint_path, manifest_csv_path, hidden)``.
    """
    hidden = 4
    model = BA283GRU(hidden=hidden)
    ckpt = tmp_path / "best.pth"
    torch.save(model.state_dict(), ckpt)

    rng = np.random.default_rng(11)
    records = []
    for i, g in enumerate(gains):
        dry = (rng.standard_normal(n) * 0.3).astype(np.float64)
        wet = (rng.standard_normal(n) * 10.0).astype(np.float64)
        dry_p = tmp_path / f"dry_{i}.wav"
        wet_p = tmp_path / f"wet_{i}.wav"
        _write_wav(dry_p, dry, sr=sr)
        _write_wav(wet_p, wet, sr=sr)
        records.append(
            {
                "dry_path": str(dry_p),
                "wet_path": str(wet_p),
                "gain_value": g,
                "duration_s": n / sr,
                "stim_kind": "noise",
            }
        )
    manifest = tmp_path / "manifest.csv"
    pd.DataFrame(records).to_csv(manifest, index=False)
    return ckpt, manifest, hidden


def test_run_cell_emits_verdict_dict(tmp_path):
    gains = [0.0, 0.5, 1.0]
    ckpt, manifest, hidden = _make_fixture(tmp_path, gains)

    cell = run_cell(
        ckpt,
        manifest,
        hidden=hidden,
        cell="baseline",
        trained_gains={0.0, 1.0},
        warmup=128,
    )

    assert set(cell) >= {"cell", "val_esr", "per_gain", "held_out_gain_esr", "notes"}
    assert cell["cell"] == "baseline"
    assert set(cell["per_gain"]) == set(gains)
    assert cell["val_esr"] == pytest.approx(
        float(np.mean(list(cell["per_gain"].values())))
    )
    # 0.5 is the only held-out gain -> a finite float, not None
    assert isinstance(cell["held_out_gain_esr"], float)


def test_run_cell_resampled_path_populates_aliasing(tmp_path):
    gains = [0.0, 0.5, 1.0]
    ckpt, manifest, hidden = _make_fixture(tmp_path, gains, sr=88200)

    cell = run_cell(
        ckpt,
        manifest,
        hidden=hidden,
        cell="hi_sr",
        trained_gains={0.0, 1.0},
        sr_train=88200,
        sr_eval=44100,
        warmup=128,
    )

    assert isinstance(cell["aliasing_high_band"], float)
    assert np.isfinite(cell["aliasing_high_band"])
    assert isinstance(cell["val_esr"], float)
