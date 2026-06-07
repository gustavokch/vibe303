# Generates the placeholder LSTM weights embedded by the Rust plugin.
# Seed-deterministic so the JSON is reproducible. The round-trip test loads
# THIS json back into PyTorch (see verify_roundtrip.py), so PyTorch and Rust
# provably use identical weights regardless of torch-init determinism.
from pathlib import Path

import torch

from train.export import export_rtneural_json
from train.model import BA283LSTM


def build_placeholder() -> BA283LSTM:
    torch.manual_seed(42)
    return BA283LSTM(hidden=32)


def main() -> int:
    m = build_placeholder()
    out = Path("plugin/weights/ba283_lstm32.json")
    out.parent.mkdir(parents=True, exist_ok=True)
    export_rtneural_json(
        m, out, meta={"sample_rate": 96000, "input_scale": 2.0, "placeholder": True}
    )
    print(f"wrote {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
