"""Self-describing JSON export of a trained BA283LSTM model.

This module is the weight-transfer contract between PyTorch (Phase 2) and the
Rust inference engine (Phase 3, Task 3.2). The Rust backend is deferred and is
expected to be a hand-coded pure-Rust LSTM(1, 32) + Dense(1) forward pass with no
external NN crate. To keep the contract unambiguous regardless of which Rust
backend is ultimately chosen, the exported JSON is *self-describing*: it carries
explicit shapes, the input/hidden sizes, the LSTM gate ordering and the exact
LSTM equations in-band, alongside every weight tensor.

The round-trip contract (PyTorch output == Rust output, max abs diff < 1e-5) is
verified in Task 3.2; this exporter therefore captures every weight exactly,
serializing Python floats (which round-trip f64) so the Rust side can read f32
with values matching to well under 1e-5.

PyTorch LSTM weight conventions (for ``nn.LSTM(input_size=I, hidden_size=H=32,
num_layers=1, batch_first=True)``):

- ``weight_ih_l0``: shape ``(4H, I)`` = ``(128, I)`` -- input->hidden weights
  for the 4 gates.
- ``weight_hh_l0``: shape ``(4H, H)`` = ``(128, 32)`` -- hidden->hidden weights
  for the 4 gates.
- ``bias_ih_l0``: shape ``(4H,)`` = ``(128,)``.
- ``bias_hh_l0``: shape ``(4H,)`` = ``(128,)``.

Gate ordering in the 4H rows is PyTorch's: **input (i), forget (f), cell (g),
output (o)** -- rows ``[0:H]`` = i, ``[H:2H]`` = f, ``[2H:3H]`` = g,
``[3H:4H]`` = o. This is recorded as ``gate_order: "ifgo"``. The Rust LSTM math
MUST use the same ordering.

Dense layer: ``dense.weight`` shape ``(1, H)`` = ``(1, 32)``, ``dense.bias``
shape ``(1,)``.

PyTorch keeps the input bias (``b_ih``) and hidden bias (``b_hh``) *separate*;
both are exported. That is why there are TWO bias vectors per LSTM layer. The
LSTM carries BOTH a hidden state ``h`` and a cell state ``c`` (each size H); the
Rust side must keep both. The exact LSTM equations (which the Rust side must
reproduce) are::

    i  = sigmoid(W_ii x + b_ii + W_hi h + b_hi)
    f  = sigmoid(W_if x + b_if + W_hf h + b_hf)
    g  = tanh   (W_ig x + b_ig + W_hg h + b_hg)
    o  = sigmoid(W_io x + b_io + W_ho h + b_ho)
    c' = f * c + i * g
    h' = o * tanh(c')
"""

import json
from pathlib import Path

from train.dataset import INPUT_SCALE, OUTPUT_REF_V, OUTPUT_SCALE
from train.model import BA283LSTM

LSTM_EQUATIONS = (
    "i=sig(Wii x+bii+Whi h+bhi); "
    "f=sig(Wif x+bif+Whf h+bhf); "
    "g=tanh(Wig x+big+Whg h+bhg); "
    "o=sig(Wio x+bio+Who h+bho); "
    "c'=f*c+i*g; "
    "h'=o*tanh(c')"
)


def export_rtneural_json(model: BA283LSTM, out_path: Path, meta: dict) -> None:
    """Export a trained BA283LSTM to self-describing JSON plus a meta sidecar.

    Writes ``out_path`` containing the full model description (shapes, gate
    order, equations and all 6 weight tensors as nested float lists) and
    ``out_path.with_suffix(".meta.json")`` containing just the ``meta`` dict.

    The 6 tensors come from ``model.state_dict()``:
    ``lstm.weight_ih_l0``, ``lstm.weight_hh_l0``, ``lstm.bias_ih_l0``,
    ``lstm.bias_hh_l0``, ``dense.weight``, ``dense.bias``.
    """
    out_path = Path(out_path)
    state = model.state_dict()

    def to_list(key: str) -> list:
        return state[key].detach().cpu().numpy().tolist()

    weight_ih = to_list("lstm.weight_ih_l0")
    weight_hh = to_list("lstm.weight_hh_l0")
    bias_ih = to_list("lstm.bias_ih_l0")
    bias_hh = to_list("lstm.bias_hh_l0")
    dense_weight = to_list("dense.weight")
    dense_bias = to_list("dense.bias")

    # FiLM mode: LSTM sees only audio (input_size 1) + a film layer; gain drives
    # the affine. Concat mode: gain is the 2nd LSTM input channel, no film layer.
    conditioning = getattr(model, "conditioning", "film")
    lstm_input_size = state["lstm.weight_ih_l0"].shape[1]
    hidden_size = model.hidden

    lstm_layer = {
        "type": "lstm",
        "shapes": {
            "weight_ih": [4 * hidden_size, int(lstm_input_size)],
            "weight_hh": [4 * hidden_size, hidden_size],
            "bias_ih": [4 * hidden_size],
            "bias_hh": [4 * hidden_size],
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
    layers = [lstm_layer]
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
        # v1 LSTM schema (carries `conditioning` + optional film layer).
        # concat models are also emitted with conditioning="concat".
        "format": "ba283-lstm-v1",
        "input_size": 2,
        "lstm_input_size": int(lstm_input_size),
        "hidden_size": int(hidden_size),
        "conditioning": conditioning,
        "film_equation": "out = dense((1 + gamma) * y + beta); [gamma,beta]=film(gain)",
        "gate_order": "ifgo",
        # Domain scaling contract (see train/dataset.py): the model trained on
        # input col 0 = dry_audio * input_scale, target = wet_volts * output_scale.
        # Plugin feeds model_in = audio * input_scale, recovers volts as
        # model_out * output_ref_v (= model_out / output_scale).
        "input_scale": float(INPUT_SCALE),
        "output_scale": float(OUTPUT_SCALE),
        "output_ref_v": float(OUTPUT_REF_V),
        "bias_convention": (
            "PyTorch keeps b_ih and b_hh separate; both exported per LSTM layer."
        ),
        "lstm_equations": LSTM_EQUATIONS,
        "layers": layers,
        "meta": meta,
    }

    out_path.parent.mkdir(parents=True, exist_ok=True)
    with out_path.open("w") as f:
        json.dump(doc, f)

    meta_path = out_path.with_suffix(".meta.json")
    with meta_path.open("w") as f:
        json.dump(meta, f)
