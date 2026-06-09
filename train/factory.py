# train/factory.py
"""Model factory: map a ``--model`` string to a constructed module.

Keeps the LSTM path bit-for-bit unchanged (``model_type="lstm"`` builds the
exact :class:`BA283LSTM` the training loop always built) while routing ``"s4d"``
/ ``"s6"`` to :class:`train.ssm.BA283SSM`. The SSM-only knobs (``d_state``,
``d_conv``, ``expand``) are accepted for every type but consumed only by the SSM
path, so a single call site in ``train.train`` and the eval harness can pass the
full set without branching.
"""

from __future__ import annotations

from torch import nn

from train.model import BA283LSTM
from train.ssm import BA283SSM

MODEL_TYPES = ("lstm", "s4d", "s6")


def make_model(
    model_type: str,
    *,
    hidden: int,
    conditioning: str = "film",
    num_layers: int = 1,
    dropout: float = 0.0,
    head: str = "linear",
    film_kind: str = "linear",
    d_state: int = 64,
    d_conv: int = 4,
    expand: int = 2,
) -> nn.Module:
    if model_type == "lstm":
        return BA283LSTM(
            hidden=hidden,
            conditioning=conditioning,
            num_layers=num_layers,
            dropout=dropout,
            head=head,
            film_kind=film_kind,
        )
    if model_type in ("s4d", "s6"):
        return BA283SSM(
            model_type,
            hidden=hidden,
            conditioning=conditioning,
            num_layers=num_layers,
            dropout=dropout,
            head=head,
            film_kind=film_kind,
            d_state=d_state,
            d_conv=d_conv,
            expand=expand,
        )
    raise ValueError(f"model_type must be one of {MODEL_TYPES}, got {model_type!r}")
