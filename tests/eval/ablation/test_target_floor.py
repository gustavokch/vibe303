import pytest

from eval.ablation.target_floor import summarize_floor


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
