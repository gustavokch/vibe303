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


def test_held_out_gain_esr_filters_to_unseen_gains():
    rng = np.random.default_rng(2)
    rows = [(d, g, d) for d, g in
            [(rng.standard_normal(2048), g) for g in (0.125, 0.375, 0.5)]]
    mean = held_out_gain_esr(_identity_predict, rows, trained_gains={0.0, 0.5, 1.0})
    assert mean < 1e-9  # only .125/.375 counted, both perfect here
