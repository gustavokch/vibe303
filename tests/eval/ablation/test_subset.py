import pandas as pd
import pytest

from eval.ablation.subset import build_subset

_COLS = ["dry_path", "wet_path", "gain_value", "duration_s", "stim_kind"]


def _full_manifest():
    rows = [
        (f"d{k}", f"w{k}_{g}", g, 60.0, k)
        for k in ("music", "pink_burst", "sweep", "multisine", "two_tone")
        for g in (0.0, 0.25, 0.5, 0.75, 1.0)
    ]
    return pd.DataFrame(rows, columns=_COLS)


def test_subset_keeps_all_gains_for_selected_kinds():
    sub = build_subset(
        _full_manifest(),
        kinds=("music", "pink_burst", "sweep", "multisine"),
        max_trim_s=20.0,
    )
    assert set(sub["stim_kind"]) == {"music", "pink_burst", "sweep", "multisine"}
    assert set(sub["gain_value"]) == {0.0, 0.25, 0.5, 0.75, 1.0}
    assert (sub["duration_s"] <= 20.0).all()
    assert "two_tone" not in set(sub["stim_kind"])


def test_subset_does_not_mutate_input():
    df = _full_manifest()
    before = df["duration_s"].tolist()
    build_subset(df, kinds=("music",), max_trim_s=5.0)
    assert df["duration_s"].tolist() == before  # original untouched


def test_subset_raises_on_missing_kind():
    with pytest.raises(ValueError):
        build_subset(_full_manifest(), kinds=("nonexistent",), max_trim_s=20.0)


def test_subset_preserves_durations_already_below_cap():
    df = _full_manifest()
    df.loc[df["stim_kind"] == "music", "duration_s"] = 3.0
    sub = build_subset(df, kinds=("music",), max_trim_s=20.0)
    assert (sub["duration_s"] == 3.0).all()  # clamp is a min, not a set
