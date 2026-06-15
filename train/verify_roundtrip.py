# train/verify_roundtrip.py
# Round-trip gate: PyTorch LSTM output == Rust hand-coded LSTM output (<1e-5).
# Loads weights from the SAME committed JSON the Rust plugin embeds, so both
# sides provably use identical weights (no torch-init seed fragility).
# NOTE: the Rust standalone is the pending #30 port (was GRU); this gate runs
# once that binary implements the LSTM forward described in train/export.py.
import argparse
import json
import subprocess
from pathlib import Path

import numpy as np
import soundfile as sf
import torch

from train.model import BA283LSTM


def load_model_from_json(json_path: Path) -> BA283LSTM:
    doc = json.loads(json_path.read_text())
    assert doc["format"] == "ba283-lstm-v2", doc.get("format")
    assert doc["input_size"] == 2
    assert doc["gate_order"] == "ifgo"
    hidden = int(doc["hidden_size"])
    num_layers = int(doc["num_layers"])
    conditioning = doc.get("conditioning", "concat")
    dense = next(layer for layer in doc["layers"] if layer["type"] == "dense")

    m = BA283LSTM(hidden=hidden, num_layers=num_layers, conditioning=conditioning)

    def t(x):
        return torch.tensor(x, dtype=torch.float32)

    # Per-layer LSTM cells + learned initial state (stacked into h0/c0).
    lstm_layers = sorted(
        (layer for layer in doc["layers"] if layer["type"] == "lstm"),
        key=lambda layer: layer["layer"],
    )
    assert len(lstm_layers) == num_layers
    sd = {
        "dense.weight": t(dense["weight"]),
        "dense.bias": t(dense["bias"]),
        # (num_layers, 1, hidden): each layer's init row inserted below.
        "h0": torch.stack([t(layer["init_hidden"]) for layer in lstm_layers]).unsqueeze(1),
        "c0": torch.stack([t(layer["init_cell"]) for layer in lstm_layers]).unsqueeze(1),
    }
    for layer in lstm_layers:
        i = layer["layer"]
        sd[f"lstm.weight_ih_l{i}"] = t(layer["weight_ih"])
        sd[f"lstm.weight_hh_l{i}"] = t(layer["weight_hh"])
        sd[f"lstm.bias_ih_l{i}"] = t(layer["bias_ih"])
        sd[f"lstm.bias_hh_l{i}"] = t(layer["bias_hh"])
    if conditioning == "film":
        film = next(layer for layer in doc["layers"] if layer["type"] == "film")
        sd["film.weight"] = t(film["weight"])
        sd["film.bias"] = t(film["bias"])
    m.load_state_dict(sd)
    m.train(False)
    return m


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument(
        "--weights-json",
        type=Path,
        default=Path("plugin/weights/ba283_lstm_3x18.json"),
    )
    ap.add_argument(
        "--standalone",
        type=Path,
        default=Path("target/release/ba283_standalone"),
    )
    ap.add_argument("--bias", type=float, default=0.5)
    ap.add_argument("--n", type=int, default=1000)
    ap.add_argument("--tol", type=float, default=1e-5)
    args = ap.parse_args()

    model = load_model_from_json(args.weights_json)
    rng = np.random.default_rng(0)
    x0 = rng.standard_normal(args.n).astype(np.float32)
    x = np.stack([x0, np.full_like(x0, args.bias)], axis=-1)[None, ...]  # (1, n, 2)
    with torch.no_grad():
        y_ref, _ = model(torch.from_numpy(x))
    y_ref = y_ref.numpy().reshape(-1)

    in_wav = Path("/tmp/rt_in.wav")
    out_wav = Path("/tmp/rt_out.wav")
    sf.write(in_wav, x0, 48_000, subtype="FLOAT")
    subprocess.check_call(
        [
            str(args.standalone),
            "--raw",
            "--bias",
            f"{args.bias}",
            "--input",
            str(in_wav),
            "--output",
            str(out_wav),
        ]
    )
    y_rust, _ = sf.read(out_wav, dtype="float32")
    y_rust = np.asarray(y_rust).reshape(-1)

    n = min(len(y_ref), len(y_rust))
    max_diff = float(np.max(np.abs(y_ref[:n] - y_rust[:n])))
    # Relative tolerance: the trained model's raw output can be O(10-100), so an
    # absolute 1e-5 tol is mis-scaled. Normalize the max abs error by the signal
    # scale (max |ref|) so the gate measures Rust-vs-PyTorch agreement in
    # relative terms (~f32 precision), independent of the output magnitude.
    scale = max(float(np.max(np.abs(y_ref[:n]))), 1e-9)
    rel_diff = max_diff / scale
    print(
        f"max diff = {max_diff:.3e} ({rel_diff:.3e} relative) over {n} samples "
        f"(rel tol {args.tol:.1e})"
    )
    if rel_diff < args.tol and n == args.n:
        print("ROUND-TRIP PASS")
        return 0
    print("ROUND-TRIP FAIL")
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
