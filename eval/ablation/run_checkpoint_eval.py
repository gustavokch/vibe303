"""Checkpoint -> ablation-metrics bridge.

Loads a trained ``best.pth`` into a :class:`train.model.BA283LSTM`, builds a
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

import argparse
import json
from collections.abc import Sequence
from pathlib import Path

import numpy as np
import pandas as pd
import soundfile as sf
import torch

from eval.ablation.gain_eval import Predict, Row, held_out_gain_esr, per_gain_esr
from eval.ablation.metrics import high_band_error, resample_to
from train.dataset import INPUT_SCALE, OUTPUT_SCALE, _resolve_path
from train.model import BA283LSTM


def _read_mono(path: str) -> np.ndarray:
    """Read an audio file as float32, mono-mixing stereo via mean.

    Mirrors :meth:`train.dataset.BA283Dataset._read_full` (cwd-relative path
    resolution + stereo mean) so eval reads exactly what training read.
    """
    resolved = _resolve_path(path)
    with sf.SoundFile(str(resolved)) as f:
        data = f.read(dtype="float32")
    if data.ndim == 2:
        data = data.mean(axis=1)
    return np.ascontiguousarray(data, dtype=np.float32)


def load_model(
    checkpoint: str | Path,
    *,
    hidden: int,
    conditioning: str = "film",
) -> BA283LSTM:
    """Construct a :class:`BA283LSTM` and load ``checkpoint`` into it (eval mode).

    ``weights_only=True`` + ``map_location="cpu"`` so a GPU-trained checkpoint
    loads on a CPU-only eval host without unpickling arbitrary objects.
    """
    model = BA283LSTM(hidden=hidden, conditioning=conditioning)
    state = torch.load(checkpoint, map_location="cpu", weights_only=True)
    model.load_state_dict(state)
    model.train(False)
    return model


def make_predict(model, *, input_scale: float, warmup: int) -> Predict:
    """Build a normalized-domain ``Predict`` closure around ``model``.

    The returned callable takes a RAW dry signal and a gain, scales the dry by
    ``input_scale`` (column 0), broadcasts gain as column 1, runs a single
    forward pass with a fresh hidden state, and returns a 1-D numpy array with
    the first ``warmup`` samples discarded (the LSTM hidden state has not settled
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


def build_rows(
    manifest_df: pd.DataFrame,
    *,
    output_scale: float,
    warmup: int,
) -> list[Row]:
    """Load manifest rows into ``(dry_raw, gain, target_normalized)`` tuples.

    Each row reads its dry and wet wavs (mono-mixed, cwd-relative resolution
    mirroring the dataset). The target is the wet signal scaled by
    ``output_scale`` with the first ``warmup`` samples trimmed -- so it aligns
    in length with a ``make_predict`` output (which trims the same warmup from
    its own result). The dry stays RAW and full-length; ``make_predict`` applies
    ``INPUT_SCALE`` and trims warmup internally.
    """
    rows: list[Row] = []
    for _, row in manifest_df.iterrows():
        dry = _read_mono(row["dry_path"])
        wet = _read_mono(row["wet_path"])
        # head-aligned; mirror dataset.py min(dry,wet)
        n = min(len(dry), len(wet))
        dry = dry[:n]
        wet = wet[:n]
        target = (wet.astype(np.float64) * output_scale)[warmup:]
        rows.append((dry, float(row["gain_value"]), target))
    return rows


def run_cell(
    checkpoint: str | Path,
    manifest: str | Path,
    *,
    hidden: int,
    conditioning: str = "film",
    cell: str,
    trained_gains: set[float],
    sr_train: int | None = None,
    sr_eval: int | None = None,
    warmup: int = 1024,
) -> dict:
    """Score one ablation cell and emit a ``render_table``-shaped dict.

    Loads the checkpoint, reads the manifest, builds normalized-domain rows and
    a predict closure, then computes per-gain ESR, the val_esr (mean over gains),
    and the held-out-gain ESR (``None`` when every gain was trained).

    When ``sr_train`` and ``sr_eval`` differ, the predict output is resampled
    ``sr_train -> sr_eval`` through the SAME polyphase path as
    :func:`metrics.esr_resampled` (via :func:`metrics.resample_to`) before
    scoring, and an ``aliasing_high_band`` field is populated with the mean
    high-band error over rows at ``sr_eval``.
    """
    model = load_model(checkpoint, hidden=hidden, conditioning=conditioning)
    manifest_df = pd.read_csv(manifest)
    rows = build_rows(manifest_df, output_scale=OUTPUT_SCALE, warmup=warmup)
    predict = make_predict(model, input_scale=INPUT_SCALE, warmup=warmup)

    rescaled = (
        sr_train is not None and sr_eval is not None and sr_train != sr_eval
    )
    scored_predict: Predict = predict
    if rescaled:

        def _resample_wrap(dry: np.ndarray, gain: float) -> np.ndarray:
            return resample_to(predict(dry, gain), sr_from=sr_train, sr_to=sr_eval)

        scored_predict = _resample_wrap

    per_gain = per_gain_esr(scored_predict, rows)
    # equal-weight per gain (ablation fairness): each gain setting counts
    # equally regardless of row count
    val_esr = float(np.mean(list(per_gain.values())))

    try:
        hog = held_out_gain_esr(scored_predict, rows, trained_gains=trained_gains)
    except ValueError as exc:
        if "no held-out rows" not in str(exc):
            raise
        hog = None

    result: dict = {
        "cell": cell,
        "val_esr": val_esr,
        "per_gain": per_gain,
        "held_out_gain_esr": hog,
        "notes": "",
    }

    if rescaled:
        hb = [
            high_band_error(scored_predict(dry, gain), target, sr=sr_eval)
            for dry, gain, target in rows
        ]
        result["aliasing_high_band"] = float(np.mean(hb))

    return result


def _main(argv: Sequence[str] | None = None) -> None:
    """CLI: score a checkpoint cell and write the result dict to ``--out`` JSON."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--checkpoint", required=True)
    parser.add_argument("--manifest", required=True)
    parser.add_argument("--hidden", type=int, required=True)
    parser.add_argument("--conditioning", default="film")
    parser.add_argument("--cell", required=True)
    parser.add_argument(
        "--trained-gains",
        required=True,
        help="comma-separated gains the model was trained on, e.g. 0.0,1.0",
    )
    parser.add_argument("--sr-train", type=int, default=None)
    parser.add_argument("--sr-eval", type=int, default=None)
    parser.add_argument("--warmup", type=int, default=1024)
    parser.add_argument("--out", required=True)
    args = parser.parse_args(argv)

    trained_gains = {float(g) for g in args.trained_gains.split(",") if g.strip()}

    cell = run_cell(
        args.checkpoint,
        args.manifest,
        hidden=args.hidden,
        conditioning=args.conditioning,
        cell=args.cell,
        trained_gains=trained_gains,
        sr_train=args.sr_train,
        sr_eval=args.sr_eval,
        warmup=args.warmup,
    )

    Path(args.out).write_text(json.dumps(cell, indent=2))


if __name__ == "__main__":  # pragma: no cover
    _main()
