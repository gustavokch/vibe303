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
from train.factory import ARCH_FILENAME, arch_spec, make_model, resolve_d_state


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
    head: str = "linear",
    film_kind: str = "linear",
    num_layers: int = 1,
    model_type: str = "lstm",
    d_state: int = 64,
    d_conv: int = 4,
    expand: int = 2,
    ignore_arch_meta: bool = False,
):
    """Construct the model via :func:`train.factory.make_model` and load
    ``checkpoint`` into it (eval mode).

    ``model_type`` / ``head`` / ``film_kind`` / ``num_layers`` (and the SSM dims
    ``d_state`` / ``d_conv`` / ``expand`` when ``model_type`` is ``s4d``/``s6``)
    must match the architecture the checkpoint was trained with, or
    ``load_state_dict`` raises on a key/shape mismatch. They default to the v1
    LSTM graph; ``dropout`` is irrelevant in eval mode and so is not exposed.

    If an ``arch.json`` sidecar (written by :mod:`train.train`) sits next to the
    checkpoint, the passed knobs are cross-checked against it FIRST and a
    mismatch raises a clear :class:`ValueError` -- catching a wrong-config score
    that ``load_state_dict`` would miss whenever the shapes happen to line up.
    Pass ``ignore_arch_meta=True`` to skip the check (e.g. a deliberately
    re-pointed graph). Checkpoints without a sidecar (legacy LSTM runs) load
    unchanged.

    ``weights_only=True`` + ``map_location="cpu"`` so a GPU-trained checkpoint
    loads on a CPU-only eval host without unpickling arbitrary objects.
    """
    want = arch_spec(
        model_type,
        hidden=hidden,
        conditioning=conditioning,
        num_layers=num_layers,
        head=head,
        film_kind=film_kind,
        d_state=d_state,
        d_conv=d_conv,
        expand=expand,
    )
    arch_path = Path(checkpoint).parent / ARCH_FILENAME
    if arch_path.exists() and not ignore_arch_meta:
        saved = json.loads(arch_path.read_text())
        diffs = {
            k: (saved.get(k), want[k]) for k in want if saved.get(k) != want[k]
        }
        if diffs:
            detail = ", ".join(
                f"{k}: checkpoint={s!r} requested={r!r}" for k, (s, r) in diffs.items()
            )
            raise ValueError(
                f"architecture mismatch vs {arch_path} ({detail}); pass "
                f"ignore_arch_meta=True to override"
            )
    model = make_model(
        model_type,
        hidden=hidden,
        conditioning=conditioning,
        head=head,
        film_kind=film_kind,
        num_layers=num_layers,
        d_state=d_state,
        d_conv=d_conv,
        expand=expand,
    )
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
    head: str = "linear",
    film_kind: str = "linear",
    num_layers: int = 1,
    model_type: str = "lstm",
    d_state: int = 64,
    d_conv: int = 4,
    expand: int = 2,
    ignore_arch_meta: bool = False,
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
    model = load_model(
        checkpoint,
        hidden=hidden,
        conditioning=conditioning,
        head=head,
        film_kind=film_kind,
        num_layers=num_layers,
        model_type=model_type,
        d_state=d_state,
        d_conv=d_conv,
        expand=expand,
        ignore_arch_meta=ignore_arch_meta,
    )
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
        # Score at sr_eval: the target must be downsampled through the SAME
        # polyphase path as the pred, otherwise ESR compares half-duration,
        # half-rate slices (pred at sr_eval vs target still at sr_train).
        rows = [
            (dry, gain, resample_to(target, sr_from=sr_train, sr_to=sr_eval))
            for dry, gain, target in rows
        ]

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
    parser.add_argument("--model", default="lstm", choices=("lstm", "s4d", "s6"))
    parser.add_argument(
        "--d-state",
        type=int,
        default=None,
        help="SSM state size N; default 64 for s4d, 16 for s6 (must match training)",
    )
    parser.add_argument("--d-conv", type=int, default=4)
    parser.add_argument("--expand", type=int, default=2)
    parser.add_argument(
        "--ignore-arch-meta",
        action="store_true",
        help="skip the arch.json cross-check next to the checkpoint",
    )
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
        model_type=args.model,
        d_state=resolve_d_state(args.model, args.d_state),
        d_conv=args.d_conv,
        expand=args.expand,
        ignore_arch_meta=args.ignore_arch_meta,
        cell=args.cell,
        trained_gains=trained_gains,
        sr_train=args.sr_train,
        sr_eval=args.sr_eval,
        warmup=args.warmup,
    )

    Path(args.out).write_text(json.dumps(cell, indent=2))


if __name__ == "__main__":  # pragma: no cover
    _main()
