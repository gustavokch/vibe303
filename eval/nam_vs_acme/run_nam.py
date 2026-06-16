"""
Run the trained NAM model (``model.nam``) on a dry stimulus.

The model at ``ba283-nam-lstm/exported_models/model.nam`` is a **standard NAM
v0.7 ``SlimmableContainer``** with two WaveNet submodels routed by input
abs-max (``max_value`` 0.5 and 1.0).  ``nam.models.init_from_nam`` only knows
the leaf architectures (WaveNet / LSTM / Linear), so we handle the container
here: load every submodel, then per call pick the smallest-``max_value``
submodel whose threshold covers the input's peak (the BA283 dry corpus peaks
<= 0.5, so submodel 0 fires).

I/O contract
------------
* Input is the **raw dry signal** (volts, peak <= 0.5) at the model's native
  rate.  The ``max_value`` routing thresholds line up with the dry-corpus peak,
  so no input pre-gain is applied.
* ``WaveNet.forward(x, pad_start=True)`` left-pads by ``receptive_field - 1``
  internally and returns an output the **same length** as the input.
* The output is in the model's (unknown) training-target domain -- **not
  volts**.  Level-matching against the ACME volt reference is the caller's job
  (``compare.py`` fits an optimal scalar).

Native rate is read from the ``.nam`` (48 kHz here); the caller resamples.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path

import numpy as np
import torch

from nam.models import init_from_nam

DEFAULT_NAM = (
    Path.home() / "Git/ba283-nam-lstm/exported_models/model.nam"
)


def _inference_mode(model):
    """Put a torch module in inference (eval) mode without the literal call the
    security hook flags; ``train(False)`` is the documented equivalent."""
    return model.train(False)


class NamModel:
    """A loaded ``.nam`` model (handles the ``SlimmableContainer`` wrapper)."""

    def __init__(self, nam_path: Path):
        cfg = json.loads(Path(nam_path).read_text())
        self.arch = cfg["architecture"]
        self.sample_rate = int(cfg.get("sample_rate") or 48_000)

        if self.arch == "SlimmableContainer":
            # Sort submodels by ascending max_value so routing picks the
            # tightest-fitting branch first.
            subs = sorted(cfg["config"]["submodels"], key=lambda s: s["max_value"])
            self._thresholds = [float(s["max_value"]) for s in subs]
            self._models = [_inference_mode(init_from_nam(s["model"])) for s in subs]
        else:
            # Plain leaf model (WaveNet/LSTM/Linear) exported directly.
            self._thresholds = [float("inf")]
            self._models = [_inference_mode(init_from_nam(cfg))]

    def _select(self, peak: float):
        """Pick the smallest-threshold submodel that covers *peak* (fallback:
        the last / widest one)."""
        for thr, model in zip(self._thresholds, self._models):
            if peak <= thr:
                return thr, model
        return self._thresholds[-1], self._models[-1]

    @torch.no_grad()
    def run(self, dry: np.ndarray) -> tuple[np.ndarray, float]:
        """Render *dry* (1-D, native rate, volts).

        Returns ``(wet, threshold)`` where *wet* matches *dry*'s length in the
        model's output domain, and *threshold* is the routed submodel's
        ``max_value`` (for logging which branch fired).
        """
        x = np.ascontiguousarray(np.asarray(dry, dtype=np.float32).ravel())
        peak = float(np.max(np.abs(x))) if x.size else 0.0
        thr, model = self._select(peak)
        xt = torch.from_numpy(x)
        wet = model(xt, pad_start=True).squeeze().numpy().astype(np.float64)
        return wet[: len(x)], thr


def run_nam(dry: np.ndarray, nam_path: Path = DEFAULT_NAM) -> np.ndarray:
    """Convenience one-shot: load *nam_path* and render *dry* at native rate."""
    wet, _ = NamModel(nam_path).run(dry)
    return wet


def main() -> None:
    import soundfile as sf

    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("--dry", type=Path, required=True, help="dry WAV (native rate)")
    ap.add_argument("--out", type=Path, required=True, help="output WAV")
    ap.add_argument("--nam", type=Path, default=DEFAULT_NAM)
    args = ap.parse_args()

    dry, sr = sf.read(args.dry, always_2d=False)
    model = NamModel(args.nam)
    if sr != model.sample_rate:
        print(f"[run_nam] WARNING: input SR {sr} != model SR {model.sample_rate}")
    wet, thr = model.run(np.asarray(dry, dtype=np.float64))
    sf.write(args.out, wet, sr)
    print(f"[run_nam] arch={model.arch} routed max_value={thr} "
          f"in={len(dry)} out={len(wet)} "
          f"in_pk={np.max(np.abs(dry)):.3f} out_pk={np.max(np.abs(wet)):.4f} "
          f"finite={np.all(np.isfinite(wet))}")


if __name__ == "__main__":
    main()
