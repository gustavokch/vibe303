"""Build a small, representative ablation subset from a dataset manifest.

The ablation runs on a reduced manifest so each cell finishes fast (see
``docs/plans/2026-06-06-ablation-matrix-design.md``): keep **all gains** (the
conditioning axis must stay fully sampled) for a handful of representative
``stim_kind`` rows, and trim long stimuli to a wall-clock cap.
"""

from collections.abc import Sequence

import pandas as pd


def build_subset(
    df: pd.DataFrame,
    *,
    kinds: Sequence[str],
    max_trim_s: float,
) -> pd.DataFrame:
    """Return the subset of ``df`` for the given stimulus ``kinds``.

    - Keeps every row whose ``stim_kind`` is in ``kinds`` (all gains retained).
    - Clamps ``duration_s`` to ``max_trim_s`` (a ceiling: shorter rows are left
      as-is). The clamp is on the manifest's declared duration; the actual
      windowing/trim is applied downstream when reading audio.
    - Does not mutate the input frame.

    Raises ``ValueError`` if any requested kind is absent from ``df`` (a typo in
    the kind list would otherwise silently shrink the subset).
    """
    available = set(df["stim_kind"])
    missing = [k for k in kinds if k not in available]
    if missing:
        raise ValueError(
            f"stim_kind(s) not in manifest: {missing}; available: {sorted(available)}"
        )
    sub = df[df["stim_kind"].isin(list(kinds))].copy()
    sub["duration_s"] = sub["duration_s"].clip(upper=max_trim_s)
    return sub.reset_index(drop=True)
