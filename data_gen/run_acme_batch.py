"""WS4 stepped/dual-conditioned reamp via ACME (4x oversampled), SPICE fallback.

The stepped retrain trains the shipped WaveNet on the BA283 saturation across a
2-D operating grid: the 5 hardware gain detents (``STEPPED_RKJ_OHMS``) crossed
with an input-gain sweep (``data_gen.stimulus.build_amplitude_sweep`` — the dry
clips pre-scaled to ``STEPPED_AMP_DB``, whose dB IS the input-gain conditioning
label). Each (input-gain dB, R_KJ) point is rendered to a wet ``.npz`` that
``data_gen.resample --stepped`` then grids + level-normalizes (``wet /= G_i``).

Renderer policy (WS4 requirement): **ACME @ 4x oversampling is primary** — it
tracks SPICE to ~ -30 dB ESR (eval/acme_vs_spice), is far faster, and has no
ngspice 2.1 GB buffer wall, so it carries the large matrix. Any point ACME fails
to render (Julia error / non-finite) **falls back to SPICE at os=2**
(``OVERSAMPLE`` = 2) and is LOGGED — never silently dropped or substituted.

G_i (per-position small-signal secondary gain) is measured with the SAME ACME
renderer and written to ``g_i.json`` (with ``makeup_ref`` = the embedded WDF
``STEPPED_MAKEUP`` for a drift gate). If the measured ``1/G_i`` diverges from the
WDF reference beyond ``--gi-tol`` the batch STOPS (the netlist drifted from the
WDF; targets must not be silently re-scaled).

This module only RENDERS (npz + g_i.json). Run ``data_gen.resample --stepped``
afterwards to grid, level-normalize, pair the aligned dry, and build the manifest.
"""
from __future__ import annotations

import argparse
import json
import math
from pathlib import Path

import numpy as np
import soundfile as sf
import soxr

from data_gen.netlist import STEPPED_MAKEUP, STEPPED_RKJ_OHMS
from data_gen.run_spice import BIAS_NORM_FIXED, SAMPLE_RATE, simulate_one
from data_gen.stimulus import STEPPED_AMP_DB, build_amplitude_sweep

# Small-signal ACME probe (mirrors data_gen.run_spice.measure_gi but via ACME).
_GI_PROBE_HZ = 1_000.0
_GI_PROBE_PEAK_V = 0.005   # << 0.5 V ceiling -> linear region
_GI_PROBE_S = 0.05


def _amp_tag(db: float) -> str:
    """Match build_amplitude_sweep's tag: '+6' -> 'p06', '-24' -> 'm24'."""
    sign = "p" if db >= 0 else "m"
    return f"ampsweep_{sign}{abs(int(round(db))):02d}"


def measure_gi_acme(rkj: float, bias: float, sr: int) -> float:
    """Linear secondary gain G_i at *rkj* via a low-level 1 kHz ACME probe."""
    from eval.acme_vs_spice.run_acme import run_acme
    n = int(_GI_PROBE_S * sr)
    t = np.arange(n) / sr
    probe = (_GI_PROBE_PEAK_V * np.sin(2 * np.pi * _GI_PROBE_HZ * t)).astype(np.float64)
    wet = run_acme(probe, sr, rgain=rkj, bias=bias)
    wet = np.asarray(wet, dtype=np.float64)[: len(probe)]
    # Drop the first few ms of transient, measure RMS ratio.
    skip = min(len(wet) // 4, int(0.01 * sr))
    out_rms = float(np.sqrt(np.mean(wet[skip:] ** 2)))
    in_rms = _GI_PROBE_PEAK_V / math.sqrt(2.0)
    return out_rms / in_rms if in_rms else 0.0


def _render_acme_4x(dry: np.ndarray, rkj: float, bias: float,
                    sr: int, os_factor: int) -> np.ndarray:
    """ACME render at *os_factor* x *sr*, antialias-decimate back to *sr*.

    Upsamples the dry to ``os_factor*sr`` (soxr VHQ), runs ACME there so the hot
    nonlinearity is computed oversampled, then polyphase-decimates the wet — the
    band-limited downsample the os=1 ACME alone would skip."""
    from eval.acme_vs_spice.run_acme import run_acme
    hi = os_factor * sr
    dry_hi = soxr.resample(np.asarray(dry, dtype=np.float64), sr, hi, quality="VHQ")
    wet_hi = run_acme(dry_hi, hi, rgain=rkj, bias=bias)
    wet_hi = np.asarray(wet_hi, dtype=np.float64)
    if not np.all(np.isfinite(wet_hi)) or wet_hi.size == 0:
        raise RuntimeError("ACME returned non-finite / empty output")
    wet = soxr.resample(wet_hi, hi, sr, quality="VHQ")
    return np.asarray(wet, dtype=np.float64)


def _write_npz(out: Path, wet: np.ndarray, sr: int) -> None:
    """Store a uniform wet render as the (t, v) npz resample_to_grid consumes."""
    t = np.arange(len(wet), dtype=np.float64) / sr
    out.parent.mkdir(parents=True, exist_ok=True)
    np.savez(out, t=t, v=np.asarray(wet, dtype=np.float64))


def run_stepped_acme_batch(
    out_dir: Path, dry_dir: Path, bias: float = BIAS_NORM_FIXED,
    sr: int = SAMPLE_RATE, os_factor: int = 4, gi_tol: float = 0.15,
    amps_db: tuple[float, ...] = STEPPED_AMP_DB,
) -> dict:
    """Render the (input-gain x R_KJ) matrix to wet npz + g_i.json.

    Returns a summary dict (rendered / fallback / failed counts + g_i rows).
    """
    out_dir = Path(out_dir)
    dry_dir = Path(dry_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    dry_dir.mkdir(parents=True, exist_ok=True)

    # 1. Dry amplitude sweep (write once so resample can pair the aligned dry).
    dry_set = build_amplitude_sweep(amps_db=amps_db)
    for tag, arr in dry_set.items():
        sf.write(dry_dir / f"{tag}.wav", arr.astype(np.float32), sr, subtype="FLOAT")

    # 2. Per-position G_i (ACME) + drift gate vs the embedded WDF makeup.
    g_rows = []
    for idx, rkj in enumerate(STEPPED_RKJ_OHMS):
        g_i = measure_gi_acme(rkj, bias, sr)
        makeup = (1.0 / g_i) if g_i else None
        ref = float(STEPPED_MAKEUP[idx])
        if makeup is not None:
            rel = abs(makeup - ref) / max(ref, 1e-12)
            if rel > gi_tol:
                raise SystemExit(
                    f"G_i drift gate FAILED at idx {idx} (R_KJ={rkj:g}): measured "
                    f"makeup {makeup:.6f} vs WDF ref {ref:.6f} (rel {rel:.1%} > "
                    f"{gi_tol:.0%}). Netlist drifted from the WDF; reconcile before "
                    "rescaling targets."
                )
        g_rows.append({"idx": idx, "rkj_ohms": float(rkj), "g_i": float(g_i),
                       "makeup": makeup, "makeup_ref": ref})
    (out_dir / "g_i.json").write_text(json.dumps(g_rows, indent=2))

    # 3. Render the matrix: ACME 4x primary, SPICE os=2 fallback (logged).
    rendered = fallback = failed = 0
    log: list[dict] = []
    for db in amps_db:
        tag = _amp_tag(db)
        dry = dry_set[tag].astype(np.float64)
        dry_wav = dry_dir / f"{tag}.wav"
        for rkj in STEPPED_RKJ_OHMS:
            stem = f"{tag}_gain{rkj:g}"
            out_npz = out_dir / f"{stem}.npz"
            try:
                wet = _render_acme_4x(dry, rkj, bias, sr, os_factor)
                _write_npz(out_npz, wet, sr)
                rendered += 1
            except Exception as e:  # noqa: BLE001 — fall back, never drop
                print(f"[acme-fail] {stem}: {e} -> SPICE os=2 fallback")
                try:
                    # SPICE writes its own (t, v) npz directly at OVERSAMPLE=2.
                    simulate_one(dry_wav, bias, out_npz, gain_rkj=rkj)
                    fallback += 1
                    log.append({"stem": stem, "renderer": "spice_os2", "reason": str(e)})
                except Exception as e2:  # noqa: BLE001
                    failed += 1
                    log.append({"stem": stem, "renderer": "FAILED", "reason": str(e2)})
                    print(f"[FAILED] {stem}: SPICE fallback also failed: {e2}")

    summary = {
        "out_dir": str(out_dir), "sr": sr, "os_factor": os_factor,
        "positions_rkj": list(STEPPED_RKJ_OHMS), "amps_db": list(amps_db),
        "rendered_acme": rendered, "fallback_spice": fallback, "failed": failed,
        "g_i": g_rows, "fallback_log": log,
    }
    (out_dir / "render_summary.json").write_text(json.dumps(summary, indent=2))
    print(f"\nACME 4x batch: {rendered} acme + {fallback} spice-fallback, "
          f"{failed} failed. g_i.json + render_summary.json in {out_dir}")
    return summary


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("--out-dir", default="data/wet_raw_stepped",
                    help="Directory for wet <tag>_gain<rkj>.npz + g_i.json.")
    ap.add_argument("--dry-dir", default="data/dry_stepped",
                    help="Directory for the dry amplitude-sweep WAVs.")
    ap.add_argument("--bias", type=float, default=BIAS_NORM_FIXED)
    ap.add_argument("--sr", type=int, default=SAMPLE_RATE)
    ap.add_argument("--os", dest="os_factor", type=int, default=4,
                    help="ACME oversampling factor (default 4).")
    ap.add_argument("--gi-tol", type=float, default=0.15,
                    help="Max relative drift of measured 1/G_i vs WDF makeup_ref "
                         "before the batch hard-fails (open item 3).")
    args = ap.parse_args(argv)
    run_stepped_acme_batch(
        Path(args.out_dir), Path(args.dry_dir), bias=args.bias, sr=args.sr,
        os_factor=args.os_factor, gi_tol=args.gi_tol,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
