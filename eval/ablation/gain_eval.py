"""Gain-resolved ESR aggregation for the BA283 model ablation study.

Reports ESR per gain value and a held-out-gain mean: evaluation at gains the
model was NOT trained on, which probes the FiLM-interpolation blind spot.

Pure numpy/python; no torch. ESR itself is DRY-reused from ``metrics.esr``.
"""

from collections import defaultdict
from collections.abc import Callable, Sequence

import numpy as np

from eval.ablation.metrics import esr

Row = tuple[np.ndarray, float, np.ndarray]
Predict = Callable[[np.ndarray, float], np.ndarray]


def per_gain_esr(predict: Predict, rows: Sequence[Row]) -> dict[float, float]:
    """Mean ESR per gain value.

    Groups ``rows`` by their ``gain`` field; for each gain runs ``predict`` on
    each row's ``dry`` input, scores ESR vs ``target``, and returns the mean ESR
    across rows sharing that gain.
    """
    grouped: dict[float, list[float]] = defaultdict(list)
    for dry, gain, target in rows:
        pred = predict(dry, gain)
        grouped[gain].append(esr(pred, target))
    result = {gain: float(np.mean(scores)) for gain, scores in grouped.items()}
    return dict(sorted(result.items()))


def held_out_gain_esr(
    predict: Predict,
    rows: Sequence[Row],
    *,
    trained_gains: set[float],
) -> float:
    """Mean ESR over rows whose gain was NOT in ``trained_gains``.

    Probes generalization to unseen gains. Raises ``ValueError`` if no held-out
    rows remain (guards a silent, meaningless 0). Gain membership is tested with
    a small float tolerance to avoid float-equality surprises.
    """
    scores: list[float] = []
    for dry, gain, target in rows:
        if any(abs(gain - t) < 1e-9 for t in trained_gains):
            continue
        scores.append(esr(predict(dry, gain), target))
    if not scores:
        raise ValueError("held_out_gain_esr: no held-out rows (all gains trained)")
    return float(np.mean(scores))
