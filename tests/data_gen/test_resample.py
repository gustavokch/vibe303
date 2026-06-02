import numpy as np
import pandas as pd
import soundfile as sf

from data_gen.resample import resample_to_grid, split_manifest, write_manifest


def test_resample_lands_on_96k_grid(tmp_path):
    t = np.cumsum(np.random.default_rng(0).uniform(5e-7, 1.5e-6, size=20_000))
    v = np.sin(2 * np.pi * 1000 * t).astype(np.float32)
    npz = tmp_path / "raw.npz"
    np.savez(npz, t=t, v=v)
    out = tmp_path / "wet.wav"
    resample_to_grid(npz, out, target_sr=96_000, head_trim_ms=50)
    data, sr = sf.read(out, dtype="float32")
    assert sr == 96_000
    assert data.shape[0] > 0
    assert 0.9 <= np.max(np.abs(data)) <= 1.1


def test_write_manifest_columns(tmp_path):
    rows = [
        {
            "dry_path": "a.wav",
            "wet_path": "a_bias0.50.wav",
            "bias_value": 0.5,
            "duration_s": 1.0,
            "stim_kind": "sweep",
        },
    ]
    out = tmp_path / "manifest.csv"
    write_manifest(rows, out)
    text = out.read_text()
    assert "dry_path,wet_path,bias_value,duration_s,stim_kind" in text.splitlines()[0]
    assert "a.wav" in text


def test_split_by_stimulus_file_no_leakage(tmp_path):
    rows = []
    for stim in ["sweepA", "sweepB", "sweepC", "noiseA", "noiseB"]:
        for bias in [0.0, 0.5, 1.0]:
            rows.append({
                "dry_path": f"dry/{stim}.wav",
                "wet_path": f"wet/{stim}_bias{bias:.2f}.wav",
                "bias_value": bias,
                "duration_s": 60.0,
                "stim_kind": "sweep" if "sweep" in stim else "noise",
            })
    manifest = tmp_path / "manifest.csv"
    pd.DataFrame(rows).to_csv(manifest, index=False)
    split_manifest(manifest, tmp_path, ratios=(0.6, 0.2, 0.2), seed=0)
    train = pd.read_csv(tmp_path / "train.csv")
    val = pd.read_csv(tmp_path / "val.csv")
    test = pd.read_csv(tmp_path / "test.csv")
    train_stems = set(train["dry_path"])
    val_stems = set(val["dry_path"])
    test_stems = set(test["dry_path"])
    assert train_stems.isdisjoint(val_stems)
    assert train_stems.isdisjoint(test_stems)
    assert val_stems.isdisjoint(test_stems)
