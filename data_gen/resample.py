"""Resample raw variable-timestep SPICE outputs to a uniform 96 kHz grid,
build a dataset manifest, and split it leakage-free into train/val/test.

Task 1.3 emits ``data/wet_raw/<stem>_bias<value>.npz`` holding the raw
``(t, v)`` transient on ngspice's adaptive (variable) timestep. This module:

1. ``resample_to_grid`` — PCHIP-interpolates each ``(t, v)`` onto a uniform
   ``1 / target_sr`` grid and writes a float WAV. PCHIP (monotonic Hermite)
   is used over a cubic spline because it does not overshoot at the clipped
   transformer-saturation edges, where a plain cubic would ring.
2. ``write_manifest`` — writes a CSV with an exact, training-consumable
   column order.
3. ``split_manifest`` — partitions the manifest by *unique dry stimulus*, so
   all bias variants of one stimulus land in the same split. The model is
   therefore validated/tested on genuinely unseen stimuli (no leakage).
"""

from __future__ import annotations

import argparse
import re
import warnings
from pathlib import Path

from math import gcd

import numpy as np
import pandas as pd
import soundfile as sf
from scipy.interpolate import PchipInterpolator
from scipy.signal import resample_poly

TARGET_SR = 96_000
MANIFEST_COLUMNS = ["dry_path", "wet_path", "gain_value", "duration_s", "stim_kind"]
# WS4 stepped/dual-conditioned manifests add the input-gain conditioning column.
STEPPED_MANIFEST_COLUMNS = MANIFEST_COLUMNS + ["input_gain_db"]

# Substring -> stim_kind, checked in order against the dry stem. Best-effort:
# unmatched stems fall back to "synth".
_STIM_KINDS = ("sweep", "noise", "imd", "impulse", "multisine", "music")

# Matches "<stem>_gain<value>" filename stems, e.g. "sweepA_gain0.500".
_GAIN_RE = re.compile(r"^(?P<stem>.+)_gain(?P<gain>[-+0-9.eE]+)$")
# WS4 stepped stems optionally carry an input-gain tag BEFORE the gain token,
# e.g. "ampsweep_ig+06_gain220" (ig = input gain in dB). The base stem, the
# input-gain dB, and the R_KJ ohms are all recovered here.
_STEPPED_RE = re.compile(
    r"^(?P<stem>.+)_ig(?P<ig>[-+0-9.eE]+)_gain(?P<gain>[-+0-9.eE]+)$"
)
# The amplitude-sweep dry tags (data_gen.stimulus.build_amplitude_sweep) encode
# the input-gain dB as "ampsweep_p06" / "ampsweep_m24" (p/m = +/-). When a wet
# stem carries that tag but no explicit "_ig" token, the input-gain dB is read
# from it so existing run_gain_batch naming (<tag>_gain<rkj>) needs no rename.
_AMP_TAG_RE = re.compile(r"(?:^|_)(?P<sign>[pm])(?P<mag>\d+)$")


def _input_gain_db_from_stem(stem: str) -> float:
    """Recover input-gain dB from an amplitude-sweep stem (0.0 if untagged)."""
    m = _AMP_TAG_RE.search(stem)
    if m is None:
        return 0.0
    mag = float(m.group("mag"))
    return mag if m.group("sign") == "p" else -mag


def load_gi_map(gi_json: Path | str) -> dict[float, float]:
    """Load ``g_i.json`` (written by the stepped renderer) into ``{rkj: G_i}``.

    ``G_i`` is the per-position small-signal secondary gain the wet target must be
    divided by so each detent trains at unity (the level-norm domain the plugin's
    ``1/G_i`` makeup table mirrors). Keyed by R_KJ ohms (float)."""
    import json
    rows = json.loads(Path(gi_json).read_text())
    out: dict[float, float] = {}
    for r in rows:
        g_i = r.get("g_i")
        if g_i:  # skip null/zero (a failed probe) — divisor stays 1.0
            out[float(r["rkj_ohms"])] = float(g_i)
    return out


def level_norm_divisor(rkj: float, gi_map: dict[float, float]) -> float:
    """Return the G_i to divide a wet target by for position *rkj* (nearest key).

    Defaults to ``1.0`` (no normalization) if the map lacks the position, so a
    missing ``g_i.json`` entry leaves the target untouched rather than guessing."""
    if not gi_map:
        return 1.0
    if rkj in gi_map:
        return gi_map[rkj]
    # Tolerate float drift in the filename vs the json (e.g. 1e9 vs 1.0e9).
    key = min(gi_map, key=lambda k: abs(k - rkj))
    return gi_map[key] if abs(key - rkj) <= 1e-3 * max(1.0, abs(rkj)) else 1.0


def resample_to_grid(
    npz_path: Path | str,
    out_wav: Path | str,
    target_sr: int = TARGET_SR,
    head_trim_ms: int = 50,
    gi_divisor: float = 1.0,
) -> Path:
    """Resample a raw ``(t, v)`` ``.npz`` onto a uniform ``target_sr`` grid.

    Parameters
    ----------
    npz_path:
        Path to a ``.npz`` holding arrays ``t`` (variable timestep) and ``v``.
    out_wav:
        Destination WAV path (written as 32-bit float).
    target_sr:
        Uniform sample rate of the output grid (Hz).
    head_trim_ms:
        Milliseconds of the head to drop as a DC-settle margin.

    Returns
    -------
    ``Path`` to the written WAV (equal to ``out_wav``).
    """
    out_wav = Path(out_wav)
    d = np.load(npz_path)
    if "t" not in d or "v" not in d:
        raise ValueError(
            f"{npz_path}: missing required arrays 't' and/or 'v' "
            f"(found {list(d.keys())})"
        )
    t = d["t"]
    v = d["v"]
    if not np.all(np.diff(t) > 0):
        raise ValueError(f"{npz_path}: time array 't' must be strictly increasing")
    # WS4 stepped: divide the wet target by the position's small-signal gain G_i
    # so each detent lands at unity (the level-norm domain the plugin's 1/G_i
    # makeup table mirrors). gi_divisor == 1.0 (default) leaves the target raw.
    if gi_divisor not in (1.0, 0.0):
        v = v / gi_divisor

    # Uniform grid spanning the simulated interval, one sample per 1/target_sr.
    tg = np.arange(t[0], t[-1], 1.0 / target_sr)
    # PCHIP: monotonic Hermite interpolation, no overshoot at clipped edges.
    v_uniform = PchipInterpolator(t, v)(tg)

    trim = int(head_trim_ms * target_sr / 1000)
    # Guard: a short stimulus (or a large head_trim) could trim every sample.
    # If trimming would leave nothing, keep the full signal — the WAV must be
    # non-empty — but warn loudly: a clip shorter than the settle margin is
    # entirely unsettled SPICE transient and silently keeping it leaks bad data.
    if trim >= len(v_uniform):
        warnings.warn(
            f"{npz_path}: head_trim {trim} samples >= signal length "
            f"{len(v_uniform)}; keeping full (likely-unsettled) signal. "
            "Input shorter than settle margin.",
            stacklevel=2,
        )
    else:
        v_uniform = v_uniform[trim:]

    out_wav.parent.mkdir(parents=True, exist_ok=True)
    sf.write(out_wav, v_uniform.astype(np.float32), target_sr, subtype="FLOAT")
    return out_wav


def write_aligned_dry(
    dry_src: Path | str,
    out_wav: Path | str,
    target_sr: int = TARGET_SR,
    head_trim_ms: int = 50,
) -> Path:
    """Write a head-trimmed copy of a dry stimulus, aligned to the wet target.

    ``resample_to_grid`` drops the first ``head_trim_ms`` of the WET signal as a
    settle margin, so ``wet[i]`` is the response to ``dry[i + trim]``. Pairing the
    raw (untrimmed) dry with the trimmed wet therefore misaligns them by ``trim``
    samples (~50 ms), which makes the dry->wet map unlearnable. This drops the
    SAME head from the dry so the written pair satisfies ``dry[i] <-> wet[i]``.
    The trim is mirrored exactly (including the short-signal keep-full fallback).
    """
    out_wav = Path(out_wav)
    data, sr = sf.read(str(dry_src), dtype="float32")
    if data.ndim == 2:
        data = data.mean(axis=1)
    # Resample the dry to the wet's grid rate. ``resample_to_grid`` renders the
    # wet at ``target_sr``; if the dry source is at a different rate (e.g. the
    # 96 kHz SPICE inputs vs a 44.1 kHz wet target) and is left at its native
    # rate, the training loader's frame-indexed (dry[i], wet[i]) windows span
    # DIFFERENT durations -> the pair is decorrelated and the dry->wet map is
    # unlearnable (the model collapses to zero output, ESR -> 1.0). Polyphase
    # resampling (linear phase, near-zero group delay) keeps dry[i] time-aligned
    # with the PCHIP wet grid.
    if sr != target_sr:
        g = gcd(int(target_sr), int(sr))
        data = resample_poly(data, int(target_sr) // g, int(sr) // g).astype(
            np.float32
        )
        sr = target_sr
    # Mirror resample_to_grid's head trim EXACTLY (same ms at the SAME, now
    # target, rate) so the kept dry head lines up with the kept wet head.
    trim = int(head_trim_ms * target_sr / 1000)
    if trim < len(data):
        data = data[trim:]
    out_wav.parent.mkdir(parents=True, exist_ok=True)
    sf.write(
        out_wav, np.ascontiguousarray(data, dtype=np.float32), target_sr, subtype="FLOAT"
    )
    return out_wav


def write_manifest(
    rows: list[dict], out_csv: Path | str,
    columns: list[str] | None = None,
) -> Path:
    """Write manifest ``rows`` to ``out_csv`` with an exact column order.

    Columns default to ``dry_path,wet_path,gain_value,duration_s,stim_kind``;
    WS4 stepped sets pass ``STEPPED_MANIFEST_COLUMNS`` (adds ``input_gain_db``).
    """
    out_csv = Path(out_csv)
    out_csv.parent.mkdir(parents=True, exist_ok=True)
    pd.DataFrame(rows, columns=columns or MANIFEST_COLUMNS).to_csv(out_csv, index=False)
    return out_csv


def split_manifest(
    manifest_csv: Path | str,
    out_dir: Path | str,
    ratios: tuple[float, float, float] = (0.8, 0.1, 0.1),
    seed: int = 42,
) -> dict[str, Path]:
    """Split a manifest into train/val/test by *unique dry stimulus*.

    All bias variants of one ``dry_path`` go to the same split, so the val/test
    sets contain only genuinely unseen stimuli (no leakage). Writes
    ``train.csv``/``val.csv``/``test.csv`` (all manifest columns preserved) to
    ``out_dir``.

    Returns a mapping ``{"train": path, "val": path, "test": path}``.
    """
    out_dir = Path(out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    df = pd.read_csv(manifest_csv)

    stems = np.asarray(df["dry_path"].unique(), dtype=object)
    rng = np.random.default_rng(seed)
    rng.shuffle(stems)

    n = len(stems)
    r_train, r_val, _ = ratios
    n_train = round(n * r_train)
    n_val = round(n * r_val)
    n_test = n - n_train - n_val

    if n < 3:
        raise ValueError(
            f"need >=3 unique dry stimuli to make non-empty train/val/test; "
            f"got {n}. "
            "Increase the stimulus library size or pass explicit ratios."
        )
    counts = {"train": n_train, "val": n_val, "test": n_test}
    empty = [name for name, c in counts.items() if c <= 0]
    if empty:
        raise ValueError(
            f"split ratios {ratios} on n={n} unique stimuli yield empty "
            f"splits: {empty}. "
            "Adjust ratios or add more stimuli."
        )

    train_stems = set(stems[:n_train])
    val_stems = set(stems[n_train : n_train + n_val])
    test_stems = set(stems[n_train + n_val :])

    splits = {
        "train": df[df["dry_path"].isin(train_stems)],
        "val": df[df["dry_path"].isin(val_stems)],
        "test": df[df["dry_path"].isin(test_stems)],
    }
    out_paths: dict[str, Path] = {}
    for name, part in splits.items():
        path = out_dir / f"{name}.csv"
        part.to_csv(path, index=False)
        out_paths[name] = path

    return out_paths


def temporal_split(
    manifest_csv: Path | str,
    out_dir: Path | str,
    ratios: tuple[float, float, float] = (0.8, 0.1, 0.1),
) -> dict[str, Path]:
    """Split EACH file by TIME into train/val/test (frame_start/frame_end columns).

    Unlike :func:`split_manifest` (which partitions whole stimuli), this keeps
    every stimulus AND every drive level in all three splits but on disjoint
    time slices: train = first ``ratios[0]`` of each file, val/test the next
    slices. Used as a diagnostic re-eval when the by-stimulus split is too small
    to give representative val/test (e.g. it exiles the loudest levels). Val/test
    are unseen time segments of seen conditions, so this measures whether the
    model can learn the transfer at all -- it is NOT leakage-free across stimuli.
    """
    out_dir = Path(out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    df = pd.read_csv(manifest_csv)
    r_train, r_val, _ = ratios
    parts: dict[str, list[dict]] = {"train": [], "val": [], "test": []}
    for _, row in df.iterrows():
        dry_f = sf.info(str(_resolve(row["dry_path"]))).frames
        wet_f = sf.info(str(_resolve(row["wet_path"]))).frames
        frames = min(dry_f, wet_f)
        a = int(frames * r_train)
        b = int(frames * (r_train + r_val))
        bounds = {"train": (0, a), "val": (a, b), "test": (b, frames)}
        for name, (lo, hi) in bounds.items():
            parts[name].append({**row.to_dict(), "frame_start": lo, "frame_end": hi})
    out_paths: dict[str, Path] = {}
    for name, rows in parts.items():
        path = out_dir / f"{name}.csv"
        pd.DataFrame(rows).to_csv(path, index=False)
        out_paths[name] = path
    return out_paths


def _resolve(raw: str) -> Path:
    p = Path(raw)
    return p if p.is_absolute() else Path.cwd() / p


def _infer_stim_kind(stem: str) -> str:
    """Best-effort substring match of a dry stem to a stim_kind."""
    low = stem.lower()
    for kind in _STIM_KINDS:
        if kind in low:
            return kind
    return "synth"


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Resample raw SPICE .npz outputs to 96k WAVs + build manifest.",
    )
    parser.add_argument(
        "--raw-dir",
        required=True,
        help="Directory of raw <stem>_bias<value>.npz files.",
    )
    parser.add_argument(
        "--out-dir",
        required=True,
        help="Directory to write resampled wet WAVs.",
    )
    parser.add_argument(
        "--manifest",
        required=True,
        help="Output manifest CSV path (train/val/test written alongside).",
    )
    parser.add_argument(
        "--target-sr",
        type=int,
        default=TARGET_SR,
        help="Uniform output sample rate (Hz).",
    )
    parser.add_argument(
        "--head-trim-ms",
        type=int,
        default=50,
        help="Milliseconds trimmed from each head (DC-settle margin).",
    )
    parser.add_argument(
        "--dry-dir",
        default="data/dry",
        help="Directory of dry stimulus WAVs (<stem>.wav); used for dry_path.",
    )
    parser.add_argument(
        "--no-split",
        action="store_true",
        help="Skip the by-stimulus train/val/test split (split_manifest needs "
        ">=3 unique dry stimuli). Use for single-stimulus gain-sweep datasets "
        "that are split temporally by the caller instead (see temporal_split).",
    )
    parser.add_argument(
        "--stepped",
        action="store_true",
        help="WS4: divide each wet target by its position G_i (from --gi-json) "
        "and emit the input_gain_db conditioning column. Filenames may carry an "
        "'_ig<db>' tag (see _STEPPED_RE); R_KJ is the '_gain<ohms>' token.",
    )
    parser.add_argument(
        "--gi-json",
        default=None,
        help="Path to g_i.json (default: <raw-dir>/g_i.json) for --stepped "
        "level normalization. Missing entries leave a target un-normalized.",
    )
    args = parser.parse_args(argv)

    raw_dir = Path(args.raw_dir)
    out_dir = Path(args.out_dir)
    dry_dir = Path(args.dry_dir)
    out_dir.mkdir(parents=True, exist_ok=True)

    gi_map: dict[float, float] = {}
    if args.stepped:
        gi_path = Path(args.gi_json) if args.gi_json else raw_dir / "g_i.json"
        if gi_path.exists():
            gi_map = load_gi_map(gi_path)
        else:
            warnings.warn(
                f"--stepped but no g_i.json at {gi_path}; wet targets will NOT be "
                "level-normalized (gi_divisor=1.0 for every position).",
                stacklevel=2,
            )

    rows: list[dict] = []
    aligned_dry: dict[str, Path] = {}  # stem -> aligned dry path (written once)
    for npz_path in sorted(raw_dir.glob("*.npz")):
        input_gain_db = 0.0
        if args.stepped:
            ms = _STEPPED_RE.match(npz_path.stem)
            if ms is not None:
                stem = ms.group("stem")
                gain = float(ms.group("gain"))
                input_gain_db = float(ms.group("ig"))
            else:
                m = _GAIN_RE.match(npz_path.stem)
                if m is None:
                    continue
                stem = m.group("stem")
                gain = float(m.group("gain"))
                # No explicit _ig token: recover input-gain dB from the
                # amplitude-sweep tag in the stem (e.g. "ampsweep_p06").
                input_gain_db = _input_gain_db_from_stem(stem)
        else:
            m = _GAIN_RE.match(npz_path.stem)
            if m is None:
                # Skip files that don't follow <stem>_gain<value>.npz.
                continue
            stem = m.group("stem")
            gain = float(m.group("gain"))

        gi_divisor = level_norm_divisor(gain, gi_map) if args.stepped else 1.0
        wet_path = out_dir / f"{npz_path.stem}.wav"
        resample_to_grid(
            npz_path,
            wet_path,
            target_sr=args.target_sr,
            head_trim_ms=args.head_trim_ms,
            gi_divisor=gi_divisor,
        )
        # Pair against a head-trimmed dry so dry[i] <-> wet[i] (see
        # write_aligned_dry). Written once per unique stem, reused across gains.
        dry_src = dry_dir / f"{stem}.wav"
        if not dry_src.exists():
            warnings.warn(
                f"dry stimulus not found: {dry_src}; manifest row will "
                "reference a missing dry WAV.",
                stacklevel=2,
            )
            dry_path = dry_src
        else:
            if stem not in aligned_dry:
                aligned_dry[stem] = write_aligned_dry(
                    dry_src,
                    out_dir / f"{stem}_dry.wav",
                    target_sr=args.target_sr,
                    head_trim_ms=args.head_trim_ms,
                )
            dry_path = aligned_dry[stem]
        info = sf.info(str(wet_path))
        row = {
            "dry_path": str(dry_path),
            "wet_path": str(wet_path),
            "gain_value": gain,
            "duration_s": info.duration,
            "stim_kind": _infer_stim_kind(stem),
        }
        if args.stepped:
            row["input_gain_db"] = input_gain_db
        rows.append(row)

    manifest = Path(args.manifest)
    write_manifest(
        rows, manifest,
        columns=STEPPED_MANIFEST_COLUMNS if args.stepped else MANIFEST_COLUMNS,
    )
    if not args.no_split:
        split_manifest(manifest, manifest.parent)
    print(f"wrote manifest: {manifest} ({len(rows)} rows)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
