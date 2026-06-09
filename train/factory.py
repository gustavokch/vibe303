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

# Per-model default for the SSM state size N when the CLI leaves it unset. S4D
# spreads ``N`` complex modes over the spectrum (64 is typical); S6's selective
# scan is Mamba-shaped where ``N=16`` is the canonical/cheaper choice. A single
# numeric default would silently build a 4x-heavier S6 than intended.
_DEFAULT_D_STATE = {"lstm": 64, "s4d": 64, "s6": 16}


def resolve_d_state(model_type: str, d_state: int | None) -> int:
    """Resolve an unset (``None``) ``--d-state`` to the per-model default.

    An explicit value is returned unchanged; ``None`` maps to 64 for ``s4d``
    (and ``lstm``, where it is ignored) and 16 for ``s6``.
    """
    if d_state is not None:
        return d_state
    return _DEFAULT_D_STATE.get(model_type, 64)


# Sidecar written next to a checkpoint capturing the architecture knobs needed
# to reconstruct the model. The eval harness cross-checks it so a checkpoint
# scored with the wrong knobs fails loudly instead of silently mis-loading (or
# only tripping on a lucky shape mismatch in ``load_state_dict``).
ARCH_FILENAME = "arch.json"


def arch_spec(
    model_type: str,
    *,
    hidden: int,
    conditioning: str = "film",
    num_layers: int = 1,
    head: str = "linear",
    film_kind: str = "linear",
    d_state: int = 64,
    d_conv: int = 4,
    expand: int = 2,
) -> dict:
    """Architecture-defining knobs for a checkpoint, per model type.

    Only the fields that change the parameter graph are emitted (``dropout`` is
    eval-irrelevant; the SSM dims appear only for the models that consume them),
    so a comparison never trips on a knob the model ignores.
    """
    spec: dict = {
        "model_type": model_type,
        "hidden": hidden,
        "conditioning": conditioning,
        "num_layers": num_layers,
        "head": head,
        "film_kind": film_kind,
    }
    if model_type in ("s4d", "s6"):
        spec["d_state"] = d_state
    if model_type == "s6":
        spec["d_conv"] = d_conv
        spec["expand"] = expand
    return spec


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
