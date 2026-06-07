"""Aggregate ablation-cell results into a verdict table.

Each cell is compared against its venue-matched baseline. A cell "explains" the
fidelity gap when it improves ESR by >= 3 dB (a halving) OR closes >= 50% of the
``baseline -> target`` distance. See
``docs/plans/2026-06-06-ablation-matrix-design.md`` (decision rule).
"""

import math
from collections.abc import Mapping, Sequence

_IMPROVEMENT_DB = 3.0
_GAP_CLOSURE_FRAC = 0.5


def delta_db(cell_esr: float, baseline_esr: float) -> float:
    """ESR change in dB: ``10*log10(cell/baseline)``. Negative = improvement.

    Raises ``ValueError`` for a non-positive baseline (ratio undefined). A
    ``cell_esr`` of 0 yields ``-inf`` (an infinite improvement), which is left as
    a valid math result.
    """
    if baseline_esr <= 0:
        raise ValueError(f"delta_db: baseline must be > 0, got {baseline_esr}")
    if cell_esr < 0:
        raise ValueError(f"delta_db: cell_esr must be >= 0, got {cell_esr}")
    if cell_esr == 0:
        return -math.inf
    return 10.0 * math.log10(cell_esr / baseline_esr)


def explains_gap(*, cell: float, baseline: float, target: float) -> bool:
    """True if ``cell`` meaningfully closes the ``baseline -> target`` fidelity gap.

    Criterion: improvement >= 3 dB OR >= 50% of the baseline-to-target distance
    closed. Either is sufficient (a big dB win on a small absolute gap, or a large
    absolute-gap closure that happens to be < 3 dB, both count).
    """
    improvement_db = -delta_db(cell, baseline)
    if improvement_db >= _IMPROVEMENT_DB:
        return True
    span = baseline - target
    if span <= 0:
        return False  # baseline already at/under target; nothing to explain
    closure = (baseline - cell) / span
    return closure >= _GAP_CLOSURE_FRAC


def render_table(cells: Sequence[Mapping[str, object]], *, target: float) -> str:
    """Render ablation cells as a markdown verdict table.

    Each cell mapping needs ``cell``, ``val_esr``, ``baseline_esr`` and may carry
    ``held_out_gain_esr`` and ``notes``. A trailing ✅/—  marks whether the cell
    explains the gap.
    """
    header = (
        "| cell | val_esr | baseline | ΔdB | held-out-gain | explains | notes |\n"
        "|---|---|---|---|---|---|---|"
    )
    rows = [header]
    for c in cells:
        val = float(c["val_esr"])
        base = float(c["baseline_esr"])
        d = delta_db(val, base)
        d_str = "-inf" if math.isinf(d) else f"{d:+.1f}"
        hog = c.get("held_out_gain_esr")
        hog_str = "—" if hog is None else f"{float(hog):.4f}"
        verdict = "✅" if explains_gap(cell=val, baseline=base, target=target) else "—"
        rows.append(
            f"| {c['cell']} | {val:.4f} | {base:.4f} | {d_str} | "
            f"{hog_str} | {verdict} | {c.get('notes', '')} |"
        )
    return "\n".join(rows)
