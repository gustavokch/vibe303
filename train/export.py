"""Self-describing JSON export of a trained BA283LSTM model.

This module is the weight-transfer contract between PyTorch and the Rust
inference engine. The Rust backend is a hand-coded pure-Rust stacked
LSTM(num_layers x hidden) + FiLM + Dense(1) forward pass with no external NN
crate. To keep the contract unambiguous, the exported JSON is *self-describing*:
it carries explicit shapes, the input/hidden sizes, the layer count, the LSTM
gate ordering and the exact LSTM equations in-band, alongside every weight tensor
and the learned per-layer initial hidden/cell state (schema ``ba283-lstm-v2``).

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

from train.dataset import INPUT_SCALE, OUTPUT_REF_V, OUTPUT_SCALE, STEPPED_RKJ_OHMS
from train.model import BA283LSTM

LSTM_EQUATIONS = (
    "i=sig(Wii x+bii+Whi h+bhi); "
    "f=sig(Wif x+bif+Whf h+bhf); "
    "g=tanh(Wig x+big+Whg h+bhg); "
    "o=sig(Wio x+bio+Who h+bho); "
    "c'=f*c+i*g; "
    "h'=o*tanh(c')"
)


def export_rtneural_json(
    model: BA283LSTM, out_path: Path, meta: dict, sample_rate: int = 48_000
) -> None:
    """Export a trained BA283LSTM to self-describing JSON plus a meta sidecar.

    Writes ``out_path`` containing the full model description (shapes, gate
    order, equations and every weight tensor as nested float lists) and
    ``out_path.with_suffix(".meta.json")`` containing just the ``meta`` dict.

    Schema ``ba283-lstm-v2``: the ``layers`` array carries one ``lstm`` entry
    *per stacked layer* (``num_layers`` of them, in order), each with its
    ``weight_ih`` / ``weight_hh`` / ``bias_ih`` / ``bias_hh`` and the learned
    initial state ``init_hidden`` / ``init_cell`` (the per-layer rows of the
    model's ``h0`` / ``c0`` parameters), followed by the optional ``film`` layer
    and the ``dense`` readout. Layer 0's ``weight_ih`` is ``(4H, lstm_input)``;
    layers >= 1 take the previous layer's hidden output, so their ``weight_ih``
    is ``(4H, H)``.
    """
    out_path = Path(out_path)
    state = model.state_dict()

    def to_list(key: str) -> list:
        return state[key].detach().cpu().numpy().tolist()

    dense_weight = to_list("dense.weight")
    dense_bias = to_list("dense.bias")

    # FiLM mode: LSTM sees only audio (input_size 1) + a film layer; gain drives
    # the affine. Concat mode: gain is the 2nd LSTM input channel, no film layer.
    conditioning = getattr(model, "conditioning", "film")
    num_layers = int(getattr(model, "num_layers", 1))
    lstm_input_size = int(state["lstm.weight_ih_l0"].shape[1])
    hidden_size = int(model.hidden)

    # Learned initial state: h0/c0 are (num_layers, 1, hidden); export each
    # layer's row as a flat length-`hidden` vector.
    h0 = to_list("h0")  # nested (num_layers, 1, hidden)
    c0 = to_list("c0")

    layers = []
    for layer_idx in range(num_layers):
        in_cols = lstm_input_size if layer_idx == 0 else hidden_size
        layers.append(
            {
                "type": "lstm",
                "layer": layer_idx,
                "shapes": {
                    "weight_ih": [4 * hidden_size, in_cols],
                    "weight_hh": [4 * hidden_size, hidden_size],
                    "bias_ih": [4 * hidden_size],
                    "bias_hh": [4 * hidden_size],
                    "init_hidden": [hidden_size],
                    "init_cell": [hidden_size],
                },
                "weight_ih": to_list(f"lstm.weight_ih_l{layer_idx}"),
                "weight_hh": to_list(f"lstm.weight_hh_l{layer_idx}"),
                "bias_ih": to_list(f"lstm.bias_ih_l{layer_idx}"),
                "bias_hh": to_list(f"lstm.bias_hh_l{layer_idx}"),
                "init_hidden": h0[layer_idx][0],
                "init_cell": c0[layer_idx][0],
            }
        )
    if conditioning == "film":
        layers.append(
            {
                "type": "film",
                "shapes": {"weight": [2 * hidden_size, 1], "bias": [2 * hidden_size]},
                "weight": to_list("film.weight"),
                "bias": to_list("film.bias"),
            }
        )
    # STEPPED (WS4): export the learned 5-entry FiLM codebook (one [gamma|beta]
    # row of length 2H per switch position). The codebook IS the per-position
    # affine — there is no scalar gain input — so the plugin indexes this table
    # by the discrete switch position (0..4) and applies
    # out = dense((1 + gamma) * y + beta). The R_KJ table is recorded so the
    # index<->resistor mapping is unambiguous and matches the WDF/plugin table.
    if conditioning == "stepped":
        positions = int(model.film_codebook.num_embeddings)
        layers.append(
            {
                "type": "film_codebook",
                "positions": positions,
                "shapes": {"weight": [positions, 2 * hidden_size]},
                # (positions, 2H): each row is [gamma(0:H) | beta(H:2H)].
                "weight": to_list("film_codebook.weight"),
                "rkj_ohms": list(STEPPED_RKJ_OHMS),
            }
        )
    layers.append(
        {
            "type": "dense",
            "shapes": {"weight": [1, hidden_size], "bias": [1]},
            "weight": dense_weight,
            "bias": dense_bias,
        }
    )

    doc = {
        # v2 LSTM schema: stacked per-layer cells + learned initial state, plus
        # `sample_rate`. Carries `conditioning` + optional film layer; concat
        # models are emitted with conditioning="concat". STEPPED (WS4): a
        # conditioning="stepped" export adds a `film_codebook` layer (5 rows)
        # plus `stepped_positions`/`stepped_rkj_ohms` and carries NO film layer.
        "format": "ba283-lstm-v2",
        "input_size": 2,
        "lstm_input_size": int(lstm_input_size),
        "hidden_size": int(hidden_size),
        "num_layers": num_layers,
        "sample_rate": int(sample_rate),
        "conditioning": conditioning,
        "film_equation": "out = dense((1 + gamma) * y + beta); [gamma,beta]=film(gain)",
        # STEPPED (WS4): in stepped mode [gamma,beta] is the codebook ROW for the
        # selected switch position (0..4), not film(gain). The codebook is the
        # `film_codebook` layer above; positions map to STEPPED_RKJ_OHMS.
        "stepped_equation": (
            "out = dense((1 + gamma) * y + beta); "
            "[gamma|beta] = film_codebook[switch_position]"
        ),
        "stepped_positions": (
            int(model.film_codebook.num_embeddings)
            if conditioning == "stepped"
            else None
        ),
        "stepped_rkj_ohms": (
            list(STEPPED_RKJ_OHMS) if conditioning == "stepped" else None
        ),
        # NOTE: the single fixed output reference (volts) is `output_ref_v`,
        # exported below in the domain-scaling block. The model output is
        # normalized; the plugin recovers volts as model_out * output_ref_v, then
        # level-norm holds unity. Shared with the WDF makeup table (handoff doc).
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
        "init_state_convention": (
            "Each lstm layer carries init_hidden/init_cell (length hidden); "
            "reset() loads them instead of zeros."
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
