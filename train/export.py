"""Self-describing JSON export of a trained BA283GRU model.

This module is the weight-transfer contract between PyTorch (Phase 2) and the
Rust inference engine (Phase 3, Task 3.2). The Rust backend is deferred and is
expected to be a hand-coded pure-Rust GRU(2, 32) + Dense(1) forward pass with no
external NN crate. To keep the contract unambiguous regardless of which Rust
backend is ultimately chosen, the exported JSON is *self-describing*: it carries
explicit shapes, the input/hidden sizes, the GRU gate ordering and the exact GRU
equations in-band, alongside every weight tensor.

The round-trip contract (PyTorch output == Rust output, max abs diff < 1e-5) is
verified in Task 3.2; this exporter therefore captures every weight exactly,
serializing Python floats (which round-trip f64) so the Rust side can read f32
with values matching to well under 1e-5.

PyTorch GRU weight conventions (for ``nn.GRU(input_size=2, hidden_size=H=32,
num_layers=1, batch_first=True)``):

- ``weight_ih_l0``: shape ``(3H, input_size)`` = ``(96, 2)`` -- input->hidden
  weights for the 3 gates.
- ``weight_hh_l0``: shape ``(3H, H)`` = ``(96, 32)`` -- hidden->hidden weights
  for the 3 gates.
- ``bias_ih_l0``: shape ``(3H,)`` = ``(96,)``.
- ``bias_hh_l0``: shape ``(3H,)`` = ``(96,)``.

Gate ordering in the 3H rows is PyTorch's: **reset (r), update (z), new (n)** --
rows ``[0:H]`` = r, ``[H:2H]`` = z, ``[2H:3H]`` = n. This is recorded as
``gate_order: "rzn"``. The Rust GRU math MUST use the same ordering.

Dense layer: ``dense.weight`` shape ``(1, H)`` = ``(1, 32)``, ``dense.bias``
shape ``(1,)``.

PyTorch keeps the input bias (``b_ih``) and hidden bias (``b_hh``) *separate*;
both are exported. That is why there are TWO bias vectors per GRU layer. The
exact GRU equations (which the Rust side must reproduce) are::

    r  = sigmoid(W_ir x + b_ir + W_hr h + b_hr)
    z  = sigmoid(W_iz x + b_iz + W_hz h + b_hz)
    n  = tanh(W_in x + b_in + r * (W_hn h + b_hn))
    h' = (1 - z) * n + z * h
"""

import json
from pathlib import Path

from train.dataset import INPUT_SCALE, OUTPUT_REF_V, OUTPUT_SCALE
from train.model import BA283GRU

GRU_EQUATIONS = (
    "r=sig(Wir x+bir+Whr h+bhr); "
    "z=sig(Wiz x+biz+Whz h+bhz); "
    "n=tanh(Win x+bin+r*(Whn h+bhn)); "
    "h'=(1-z)*n+z*h"
)


def export_rtneural_json(model: BA283GRU, out_path: Path, meta: dict) -> None:
    """Export a trained BA283GRU to self-describing JSON plus a meta sidecar.

    Writes ``out_path`` containing the full model description (shapes, gate
    order, equations and all 6 weight tensors as nested float lists) and
    ``out_path.with_suffix(".meta.json")`` containing just the ``meta`` dict.

    The 6 tensors come from ``model.state_dict()``:
    ``gru.weight_ih_l0``, ``gru.weight_hh_l0``, ``gru.bias_ih_l0``,
    ``gru.bias_hh_l0``, ``dense.weight``, ``dense.bias``.
    """
    out_path = Path(out_path)
    state = model.state_dict()

    def to_list(key: str) -> list:
        return state[key].detach().cpu().numpy().tolist()

    weight_ih = to_list("gru.weight_ih_l0")
    weight_hh = to_list("gru.weight_hh_l0")
    bias_ih = to_list("gru.bias_ih_l0")
    bias_hh = to_list("gru.bias_hh_l0")
    dense_weight = to_list("dense.weight")
    dense_bias = to_list("dense.bias")

    # FiLM mode: GRU sees only audio (input_size 1) + a film layer; bias drives
    # the affine. Concat mode: bias is the 2nd GRU input channel, no film layer.
    conditioning = getattr(model, "conditioning", "film")
    gru_input_size = state["gru.weight_ih_l0"].shape[1]
    hidden_size = model.hidden

    gru_layer = {
        "type": "gru",
        "shapes": {
            "weight_ih": [3 * hidden_size, int(gru_input_size)],
            "weight_hh": [3 * hidden_size, hidden_size],
            "bias_ih": [3 * hidden_size],
            "bias_hh": [3 * hidden_size],
        },
        "weight_ih": weight_ih,
        "weight_hh": weight_hh,
        "bias_ih": bias_ih,
        "bias_hh": bias_hh,
    }
    dense_layer = {
        "type": "dense",
        "shapes": {"weight": [1, hidden_size], "bias": [1]},
        "weight": dense_weight,
        "bias": dense_bias,
    }
    layers = [gru_layer]
    if conditioning == "film":
        layers.append(
            {
                "type": "film",
                "shapes": {"weight": [2 * hidden_size, 1], "bias": [2 * hidden_size]},
                "weight": to_list("film.weight"),
                "bias": to_list("film.bias"),
            }
        )
    layers.append(dense_layer)

    doc = {
        # v2 = FiLM-capable schema (carries `conditioning` + optional film layer).
        # concat models are also emitted as v2 with conditioning="concat".
        "format": "ba283-gru-v2",
        "input_size": 2,
        "gru_input_size": int(gru_input_size),
        "hidden_size": int(hidden_size),
        "conditioning": conditioning,
        "film_equation": "out = dense((1 + gamma) * y + beta); [gamma,beta]=film(bias)",
        "gate_order": "rzn",
        # Domain scaling contract (see train/dataset.py): the model trained on
        # input col 0 = dry_audio * input_scale, target = wet_volts * output_scale.
        # Plugin feeds model_in = audio * input_scale, recovers volts as
        # model_out * output_ref_v (= model_out / output_scale).
        "input_scale": float(INPUT_SCALE),
        "output_scale": float(OUTPUT_SCALE),
        "output_ref_v": float(OUTPUT_REF_V),
        "bias_convention": (
            "PyTorch keeps b_ih and b_hh separate; both exported per GRU layer."
        ),
        "gru_equations": GRU_EQUATIONS,
        "layers": layers,
        "meta": meta,
    }

    out_path.parent.mkdir(parents=True, exist_ok=True)
    with out_path.open("w") as f:
        json.dump(doc, f)

    meta_path = out_path.with_suffix(".meta.json")
    with meta_path.open("w") as f:
        json.dump(meta, f)
