import pandas as pd
import pytest

from eval.ablation.target_floor import (
    esrs_from_summary,
    subset_stims_gains,
    summarize_floor,
)


def test_summarize_floor_flags_target_bound():
    out = summarize_floor({"acme": [0.09, 0.10], "wdf": [0.11, 0.12]}, threshold=0.05)
    assert out["acme_median"] == pytest.approx(0.095)
    assert out["wdf_median"] == pytest.approx(0.115)
    assert out["target_bound"] is True


def test_summarize_floor_clears_when_below_threshold():
    out = summarize_floor({"acme": [0.001, 0.002], "wdf": [0.003]}, threshold=0.05)
    assert out["target_bound"] is False


def test_summarize_floor_target_bound_if_any_model_exceeds():
    # acme clean, wdf over threshold -> still target-bound (worst model gates)
    out = summarize_floor({"acme": [0.001], "wdf": [0.20]}, threshold=0.05)
    assert out["target_bound"] is True


def test_summarize_floor_median_is_true_median_not_mean():
    out = summarize_floor({"acme": [0.01, 0.02, 0.30]}, threshold=1.0)
    assert out["acme_median"] == pytest.approx(0.02)  # median of 3, not the 0.11 mean


def test_summarize_floor_raises_on_empty_model_list():
    with pytest.raises(ValueError):
        summarize_floor({"acme": []}, threshold=0.05)


def test_summarize_floor_raises_on_nan_median_not_silently_clear():
    # a NaN median must NOT silently report target_bound=False (would falsely
    # steer the redesign toward the model instead of data-gen).
    nan = float("nan")
    with pytest.raises(ValueError):
        summarize_floor({"acme": [nan, 0.2]}, threshold=0.05)


# --- esrs_from_summary: shapes mirror the real wdf/acme compare.py summary.json ---

def test_esrs_from_summary_wdf_gain_ladder_db_to_linear():
    # WDF stores rows under 'gain_ladder'; esr_db -30 -> 1e-3, -20 -> 1e-2.
    summary = {
        "gain_ladder": {
            "sweep": [{"esr_db": -30.0}, {"esr_db": None, "spice_failed": True}],
            "pink": [{"esr_db": -20.0}],
        }
    }
    out = esrs_from_summary(summary)
    assert out == pytest.approx([1e-3, 1e-2])  # None row skipped


def test_esrs_from_summary_acme_points_shape():
    summary = {"points": {"b0.77_sweep": [{"esr_db": -10.0}]}}
    assert esrs_from_summary(summary) == pytest.approx([0.1])


def test_esrs_from_summary_raises_without_container_key():
    with pytest.raises(ValueError):
        esrs_from_summary({"esr_summary_db": {"median": -25.0}})


def test_esrs_from_summary_raises_when_all_rows_failed():
    with pytest.raises(ValueError):
        esrs_from_summary({"gain_ladder": {"sweep": [{"esr_db": None}]}})


# --- subset_stims_gains: derive harness --stims/--gains from a manifest ---

def test_subset_stims_gains_strips_dry_suffix_and_sorts_gains():
    df = pd.DataFrame(
        {
            "dry_path": [
                "data/dry/sweep_dry.wav",
                "data/dry/sweep_dry.wav",
                "data/dry/pink_dry.wav",
            ],
            "gain_value": [1.0, 0.0, 0.5],
        }
    )
    stims, gains = subset_stims_gains(df)
    assert stims == ["sweep", "pink"]  # de-duped, first-seen order
    assert gains == [0.0, 0.5, 1.0]  # sorted ascending
