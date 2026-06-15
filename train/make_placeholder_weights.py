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
    # NAM-style 3 x 18 default. Randomise the learned initial state too (it
    # defaults to zeros) so the round-trip test actually exercises init-state
    # loading on both the PyTorch and Rust sides.
    m = BA283LSTM(hidden=18, num_layers=3)
    with torch.no_grad():
        m.h0.copy_(0.1 * torch.randn_like(m.h0))
        m.c0.copy_(0.1 * torch.randn_like(m.c0))
    return m


def main() -> int:
    m = build_placeholder()
    out = Path("plugin/weights/ba283_lstm_3x18.json")
    out.parent.mkdir(parents=True, exist_ok=True)
    export_rtneural_json(
        m,
        out,
        meta={"sample_rate": 48000, "input_scale": 2.0, "placeholder": True},
        sample_rate=48000,
    )
    print(f"wrote {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
