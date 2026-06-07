import numpy as np

from eval.ablation.gain_eval import held_out_gain_esr, per_gain_esr


def _identity_predict(dry, gain):
    return dry  # perfect on identity targets


def test_per_gain_esr_groups_by_gain():
    rng = np.random.default_rng(1)
    rows = [(rng.standard_normal(2048), g, None) for g in (0.0, 0.5, 1.0)]
    rows = [(d, g, d) for (d, g, _) in rows]  # target == dry
    out = per_gain_esr(_identity_predict, rows)
    assert set(out) == {0.0, 0.5, 1.0}
    assert all(v < 1e-9 for v in out.values())


def test_per_gain_esr_averages_multiple_rows_at_same_gain():
    # two rows at gain 0.5: one scored perfectly, one with known ESR.
    def predict(dry, gain):
        # err on the row whose dry[0] != 0.0 by predicting zeros.
        return dry if dry[0] == 0.0 else np.zeros_like(dry)

    perfect_dry = np.concatenate([[0.0], np.ones(2047)])
    perfect_row = (perfect_dry, 0.5, perfect_dry)  # pred==target -> ESR 0.0
    err_dry = np.concatenate([[1.0], np.ones(2047)])
    err_row = (err_dry, 0.5, err_dry)  # pred=zeros, target=err_dry -> ESR 1.0
    out = per_gain_esr(predict, [perfect_row, err_row])
    assert set(out) == {0.5}
    assert abs(out[0.5] - 0.5) < 1e-9  # mean of {0.0, 1.0}


def test_held_out_gain_esr_filters_to_unseen_gains():
    rng = np.random.default_rng(2)
    rows = [(d, g, d) for d, g in
            [(rng.standard_normal(2048), g) for g in (0.125, 0.375, 0.5)]]
    mean = held_out_gain_esr(_identity_predict, rows, trained_gains={0.0, 0.5, 1.0})
    assert mean < 1e-9  # only .125/.375 counted, both perfect here
