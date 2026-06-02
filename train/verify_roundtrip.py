# train/verify_roundtrip.py
# Round-trip gate: PyTorch GRU output == Rust hand-coded GRU output (<1e-5).
# Loads weights from the SAME committed JSON the Rust plugin embeds, so both
# sides provably use identical weights (no torch-init seed fragility).
import argparse
import json
import subprocess
from pathlib import Path

import numpy as np
import soundfile as sf
import torch

from train.model import BA283GRU


def load_model_from_json(json_path: Path) -> BA283GRU:
    doc = json.loads(json_path.read_text())
    assert doc["hidden_size"] == 32 and doc["input_size"] == 2
    assert doc["format"] == "ba283-gru-v1"
    assert doc["gate_order"] == "rzn"
    gru = next(layer for layer in doc["layers"] if layer["type"] == "gru")
    dense = next(layer for layer in doc["layers"] if layer["type"] == "dense")
    m = BA283GRU(hidden=32)
    sd = {
        "gru.weight_ih_l0": torch.tensor(gru["weight_ih"], dtype=torch.float32),
        "gru.weight_hh_l0": torch.tensor(gru["weight_hh"], dtype=torch.float32),
        "gru.bias_ih_l0": torch.tensor(gru["bias_ih"], dtype=torch.float32),
        "gru.bias_hh_l0": torch.tensor(gru["bias_hh"], dtype=torch.float32),
        "dense.weight": torch.tensor(dense["weight"], dtype=torch.float32),
        "dense.bias": torch.tensor(dense["bias"], dtype=torch.float32),
    }
    m.load_state_dict(sd)
    m.train(False)
    return m


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument(
        "--weights-json",
        type=Path,
        default=Path("plugin/weights/ba283_gru32.json"),
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
    sf.write(in_wav, x0, 96_000, subtype="FLOAT")
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
    print(f"max diff = {max_diff:.3e} over {n} samples (tol {args.tol:.1e})")
    if max_diff < args.tol and n == args.n:
        print("ROUND-TRIP PASS")
        return 0
    print("ROUND-TRIP FAIL")
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
