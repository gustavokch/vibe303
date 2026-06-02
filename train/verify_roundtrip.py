# train/verify_roundtrip.py
import argparse
from pathlib import Path

import numpy as np
import torch

from train.model import BA283GRU


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--checkpoint", type=Path, default=Path("checkpoints/best.pth"))
    ap.add_argument("--out-ref", type=Path, default=Path("/tmp/pytorch_ref.npy"))
    args = ap.parse_args()

    model = BA283GRU(hidden=32)
    state = torch.load(args.checkpoint, map_location="cpu", weights_only=True)
    model.load_state_dict(state)
    model.train(False)

    rng = np.random.default_rng(0)
    x = rng.standard_normal((1, 1000, 2)).astype(np.float32)
    x[..., 1] = 0.5
    with torch.no_grad():
        y, _ = model(torch.from_numpy(x))
    np.save(args.out_ref, y.numpy())
    print(f"saved {args.out_ref}, shape={y.shape}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
