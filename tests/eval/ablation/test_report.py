import pytest

from eval.ablation.report import delta_db, explains_gap, render_table


def test_delta_db_halving_is_minus_3():
    assert delta_db(0.05, 0.10) == pytest.approx(-3.01, abs=0.05)


def test_delta_db_zero_when_equal():
    assert delta_db(0.097, 0.097) == pytest.approx(0.0)


def test_delta_db_raises_on_nonpositive_baseline():
    with pytest.raises(ValueError):
        delta_db(0.05, 0.0)


def test_explains_gap_on_3db_improvement():
    # 0.10 -> 0.05 is exactly -3.01 dB (a halving) -> explains
    assert explains_gap(cell=0.05, baseline=0.10, target=0.01) is True


def test_explains_gap_on_half_gap_closure_even_below_3db():
    # 0.10 -> 0.055: only ~2.6 dB, but closes (0.10-0.055)/(0.10-0.01)=0.5 of the gap
    assert explains_gap(cell=0.055, baseline=0.10, target=0.01) is True


def test_does_not_explain_small_improvement():
    assert explains_gap(cell=0.092, baseline=0.097, target=0.01) is False


def test_render_table_has_headers_and_row_with_delta_and_verdict():
    md = render_table(
        [
            {
                "cell": "C1",
                "val_esr": 0.05,
                "baseline_esr": 0.10,
                "held_out_gain_esr": 0.07,
                "notes": "hidden 64",
            }
        ],
        target=0.01,
    )
    assert "| cell |" in md
    assert "C1" in md
    assert "hidden 64" in md
    assert "-3.0" in md  # ΔdB column rendered
    assert "✅" in md  # explains-gap verdict marker for this row


def test_render_table_marks_non_explaining_row():
    md = render_table(
        [{"cell": "C3", "val_esr": 0.092, "baseline_esr": 0.097, "notes": "alpha 0.95"}],
        target=0.01,
    )
    assert "C3" in md
    assert "—" in md  # the non-explain verdict branch
    assert "✅" not in md


def test_render_table_handles_perfect_cell_minus_inf_db():
    md = render_table(
        [{"cell": "C9", "val_esr": 0.0, "baseline_esr": 0.10, "notes": "perfect"}],
        target=0.01,
    )
    assert "-inf" in md  # -inf dB rendered cleanly without breaking the column
    assert "✅" in md  # a perfect cell explains the gap
