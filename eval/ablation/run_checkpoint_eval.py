"""Checkpoint -> ablation-metrics bridge.

Loads a trained ``best.pth`` into a :class:`train.model.BA283GRU`, builds a
``Predict`` closure that operates in the dataset's NORMALIZED domain (dry scaled
by ``INPUT_SCALE`` on input, wet scaled by ``OUTPUT_SCALE`` on target), runs the
existing ablation metrics, and emits a results dict that
:func:`eval.ablation.report.render_table` consumes.

The normalized-domain contract is the correctness crux: the model learns
``input = dry * INPUT_SCALE`` -> ``target = wet * OUTPUT_SCALE``. ``make_predict``
applies ``INPUT_SCALE`` before the model; ``build_rows`` applies ``OUTPUT_SCALE``
to the wet target. ESR is invariant only to a SHARED scale on pred+target, so a
mismatch here corrupts the score even for a perfect model.
"""

from __future__ import annotations

from pathlib import Path

import numpy as np
import torch

from eval.ablation.gain_eval import Predict
from train.model import BA283GRU


def load_model(
    checkpoint: str | Path,
    *,
    hidden: int,
    conditioning: str = "film",
) -> BA283GRU:
    """Construct a :class:`BA283GRU` and load ``checkpoint`` into it (eval mode).

    ``weights_only=True`` + ``map_location="cpu"`` so a GPU-trained checkpoint
    loads on a CPU-only eval host without unpickling arbitrary objects.
    """
    model = BA283GRU(hidden=hidden, conditioning=conditioning)
    state = torch.load(checkpoint, map_location="cpu", weights_only=True)
    model.load_state_dict(state)
    model.train(False)
    return model


def make_predict(model, *, input_scale: float, warmup: int) -> Predict:
    """Build a normalized-domain ``Predict`` closure around ``model``.

    The returned callable takes a RAW dry signal and a gain, scales the dry by
    ``input_scale`` (column 0), broadcasts gain as column 1, runs a single
    forward pass with a fresh hidden state, and returns a 1-D numpy array with
    the first ``warmup`` samples discarded (the GRU hidden state has not settled
    there, so those samples would pollute the ESR score).
    """

    def predict(dry: np.ndarray, gain: float) -> np.ndarray:
        dry = np.asarray(dry, dtype=np.float32)
        n = len(dry)
        x = torch.empty(1, n, 2, dtype=torch.float32)
        x[0, :, 0] = torch.from_numpy(np.ascontiguousarray(dry)) * input_scale
        x[0, :, 1] = float(gain)
        with torch.no_grad():
            out, _ = model(x)
        out = out.reshape(-1).cpu().numpy()
        return out[warmup:]

    return predict
