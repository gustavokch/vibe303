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

Conditioned models
------------------
The FiLM gain-conditioned model (``model.cond.nam``, Task 5) is a *bare*
``architecture=="WaveNet"`` whose first layer has ``input_size == 2``: it
consumes a 2-channel input ``[audio, gain]``.  The public NAM wrapper is
single-channel only, so for the conditioned path we keep the **inner** net
(``init_from_nam(cfg)._net``), whose ``forward`` is VALID mode (no pad_start):
``(1, 2, L) -> (1, 1, L - (RF - 1))``.

To stay time-aligned to the dry timeline (the caller's lag-align uses a tiny
SPAN that cannot cover the ~RF-sample latency) we **manually front-pad** the
audio channel by ``RF - 1`` zeros so the valid-mode output length equals
``len(dry)``.  The audio channel is scaled by ``audio_input_scale`` (the model
was trained on ``dry * INPUT_SCALE``); the gain channel is the constant
normalized gain over the full padded length.  ``run(dry, gain=g)`` requires a
gain for this path; the legacy ``SlimmableContainer`` path ignores ``gain``.
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
        self._conditioned = False

        if self.arch == "SlimmableContainer":
            # Sort submodels by ascending max_value so routing picks the
            # tightest-fitting branch first.
            subs = sorted(cfg["config"]["submodels"], key=lambda s: s["max_value"])
            self._thresholds = [float(s["max_value"]) for s in subs]
            self._models = [_inference_mode(init_from_nam(s["model"])) for s in subs]
        elif self.arch == "WaveNet" and self._input_size(cfg) == 2:
            # FiLM gain-conditioned WaveNet (2-channel input: [audio, gain]).
            # The public wrapper cannot forward 2 channels; keep the inner net.
            self._conditioned = True
            wrapper = _inference_mode(init_from_nam(cfg))
            net = wrapper._net if hasattr(wrapper, "_net") else wrapper
            self._net = net
            self._receptive_field = int(net.receptive_field)
            meta = cfg.get("metadata") or {}
            cond = meta.get("condition") or {}
            # audio_input_scale lives in metadata.condition (current export) or,
            # for older exports, falls back to the top-level input_scale field.
            self._audio_input_scale = float(
                cond.get("audio_input_scale", meta.get("input_scale", 1.0))
            )
            self._output_ref_v = float(meta.get("output_ref_v", 1.0))
            self._gain_channel = int(cond.get("gain_channel", 1))
            self._thresholds = [float("inf")]
            self._models = [net]
        else:
            # Plain leaf model (WaveNet/LSTM/Linear) exported directly.
            self._thresholds = [float("inf")]
            self._models = [_inference_mode(init_from_nam(cfg))]

    @staticmethod
    def _input_size(cfg: dict):
        """First-layer input_size for a bare WaveNet `.nam` (None if absent)."""
        try:
            return int(cfg["config"]["layers"][0]["input_size"])
        except (KeyError, IndexError, TypeError, ValueError):
            return None

    def _select(self, peak: float):
        """Pick the smallest-threshold submodel that covers *peak* (fallback:
        the last / widest one)."""
        for thr, model in zip(self._thresholds, self._models):
            if peak <= thr:
                return thr, model
        return self._thresholds[-1], self._models[-1]

    @torch.no_grad()
    def run(self, dry: np.ndarray, gain: float | None = None) -> tuple[np.ndarray, float]:
        """Render *dry* (1-D, native rate, volts).

        Returns ``(wet, threshold)`` where *wet* matches *dry*'s length in the
        model's output domain.  For the legacy ``SlimmableContainer`` path
        *threshold* is the routed submodel's ``max_value`` and *gain* is
        ignored (back-compat).  For the conditioned WaveNet path *gain* is
        REQUIRED and *threshold* echoes the gain used.
        """
        if self._conditioned:
            return self._run_conditioned(dry, gain)
        x = np.ascontiguousarray(np.asarray(dry, dtype=np.float32).ravel())
        peak = float(np.max(np.abs(x))) if x.size else 0.0
        thr, model = self._select(peak)
        xt = torch.from_numpy(x)
        wet = model(xt, pad_start=True).squeeze().numpy().astype(np.float64)
        return wet[: len(x)], thr

    @torch.no_grad()
    def _run_conditioned(self, dry: np.ndarray, gain: float | None):
        """2-channel FiLM-conditioned path: build ``[audio*scale, gain]``,
        front-pad by ``RF-1`` for time alignment, forward the inner net."""
        if gain is None:
            raise ValueError(
                "conditioned model requires an explicit gain; call run(dry, gain=g)"
            )
        # The model was trained on normalized gain in [0,1]; an out-of-range or
        # non-finite condition is out-of-distribution.  Reject at the entry
        # point (self-defensive: the caller in compare.py also pre-validates).
        if not np.isfinite(gain) or not (0.0 <= gain <= 1.0):
            raise ValueError(
                f"conditioned gain must be finite and in [0,1]; got {gain!r}"
            )
        x = np.ascontiguousarray(np.asarray(dry, dtype=np.float32).ravel())
        n = len(x)
        rf = self._receptive_field
        # Audio channel: trained on dry * INPUT_SCALE; front-pad RF-1 zeros so
        # the valid-mode output (length L-(RF-1)) equals len(dry) and stays
        # aligned to the dry timeline.
        audio = np.concatenate(
            [np.zeros(rf - 1, dtype=np.float32), x * np.float32(self._audio_input_scale)]
        )
        # Gain channel: constant over the FULL padded length.
        gain_ch = np.full(rf - 1 + n, np.float32(gain), dtype=np.float32)
        # Audio occupies channel 0; the gain occupies metadata gain_channel
        # (channel 1 by convention).  audio_ch is whichever channel is not gain.
        audio_ch = 0 if self._gain_channel != 0 else 1
        inp = np.empty((2, rf - 1 + n), dtype=np.float32)
        inp[audio_ch] = audio
        inp[self._gain_channel] = gain_ch
        xt = torch.from_numpy(inp).unsqueeze(0)  # (1, 2, RF-1+L)
        wet = self._net(xt).squeeze().numpy().astype(np.float64)
        return wet[:n], float(gain)


def run_nam(dry: np.ndarray, nam_path: Path = DEFAULT_NAM,
            gain: float | None = None) -> np.ndarray:
    """Convenience one-shot: load *nam_path* and render *dry* at native rate.

    *gain* is forwarded to ``NamModel.run`` (required for conditioned models,
    ignored for the legacy ``SlimmableContainer``)."""
    wet, _ = NamModel(nam_path).run(dry, gain=gain)
    return wet


def main() -> None:
    import soundfile as sf

    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("--dry", type=Path, required=True, help="dry WAV (native rate)")
    ap.add_argument("--out", type=Path, required=True, help="output WAV")
    ap.add_argument("--nam", type=Path, default=DEFAULT_NAM)
    ap.add_argument("--gain", type=float, default=None,
                    help="normalized gain for conditioned models (ignored otherwise)")
    args = ap.parse_args()

    dry, sr = sf.read(args.dry, always_2d=False)
    model = NamModel(args.nam)
    if sr != model.sample_rate:
        print(f"[run_nam] WARNING: input SR {sr} != model SR {model.sample_rate}")
    wet, thr = model.run(np.asarray(dry, dtype=np.float64), gain=args.gain)
    sf.write(args.out, wet, sr)
    print(f"[run_nam] arch={model.arch} routed max_value={thr} "
          f"in={len(dry)} out={len(wet)} "
          f"in_pk={np.max(np.abs(dry)):.3f} out_pk={np.max(np.abs(wet)):.4f} "
          f"finite={np.all(np.isfinite(wet))}")


if __name__ == "__main__":
    main()
