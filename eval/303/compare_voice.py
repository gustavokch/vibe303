"""Whole-voice ESR compare: SPICE reference vs the Rust 303 voice, per operating point.

For each op-point: render the chained-deck SPICE reference (`voice_ref_spice`) and the
Rust voice (`render303` raw f32), RMS-normalise both, and compute lag-aligned ESR/NRMSE
(`esr.metrics`). Prints a table and dumps `voice_esr_baseline.json` — the F1 deliverable
(the current Low/TPT gap).

  uv run --no-project --with PySpice --with numpy python eval/303/compare_voice.py

Notes:
  * `metrics()` does not normalise gain, so both signals are scaled to unit RMS first
    (the absolute levels of a TPT voice and a SPICE deck are unrelated; we measure the
    SHAPE gap, not a level mismatch).
  * cutoff/reso/accent are NOT yet wired into the Rust voice (lands in F2), so the Rust
    render is identical across corners at a given note — the corner axis shows how far
    the fixed-cutoff TPT voice sits from each SPICE corner. Accent is omitted entirely
    until it exists on both sides.
"""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
sys.path.insert(0, str(HERE))
import esr  # noqa: E402
import voice_ref_spice as vr  # noqa: E402

RENDER303 = ROOT / "target" / "debug" / "render303"
SR = 48_000.0
SECS = 0.2

# (label, note, cutoff_itail [A], rreso [Ohm]). Notes 45/57/69 = A1/A2/A3 (110/220/440 Hz).
OP_POINTS = [
    ("A1_mid",         45, 32e-6, 47e3),
    ("A1_bright_open", 45, 128e-6, 1e12),
    ("A2_mid",         57, 32e-6, 47e3),
    ("A2_bright_open", 57, 128e-6, 1e12),
    ("A3_mid",         69, 32e-6, 47e3),
    ("A3_bright_open", 69, 128e-6, 1e12),
]


def _norm(x: np.ndarray) -> np.ndarray:
    x = np.asarray(x, float)
    r = np.sqrt(np.mean(x**2))
    return x / r if r > 1e-20 else x


def render_rust(note: int, cutoff: float, reso: float) -> np.ndarray:
    with tempfile.TemporaryDirectory() as d:
        out = Path(d) / "r.f32"
        subprocess.run(
            [str(RENDER303), "--note", str(note), "--sr", str(SR), "--secs", str(SECS),
             "--cutoff", str(cutoff), "--reso", str(reso), "--out", str(out)],
            check=True, capture_output=True,
        )
        return np.fromfile(out, dtype="<f4").astype(np.float64)


def main() -> int:
    if not RENDER303.exists():
        print(f"missing {RENDER303}; run: cargo build --bin render303 -p plugin303", file=sys.stderr)
        return 1

    cv_cache: dict[int, float] = {}
    results: dict[str, dict] = {}
    print(f"{'op-point':>16} {'note':>4} {'cutoff_uA':>10} {'rreso':>8} "
          f"{'esr_db':>8} {'nrmse':>8} {'lag':>5}")
    for label, note, cutoff_itail, rreso in OP_POINTS:
        if note not in cv_cache:
            cv_cache[note] = vr.solve_cv(vr.note_to_freq(note), SR)
        ref = vr.voice_reference(note, cutoff_itail, rreso, sr=SR, secs=SECS, cv=cv_cache[note])
        # F2.4: drive the Rust voice at the SPICE corner's equivalent cutoff/reso.
        cutoff_hz = vr.itail_to_hz(cutoff_itail)
        reso01 = vr.rreso_to_reso(rreso)
        rust = render_rust(note, cutoff_hz, reso01 * 100.0)  # render303 --reso is a %
        m = esr.metrics(_norm(ref), _norm(rust))
        results[label] = {
            "note": note, "cutoff_itail": cutoff_itail, "rreso": rreso,
            "esr_db": m["esr_db"], "nrmse": m["nrmse"], "lag": m["lag"],
        }
        rr = "open" if rreso >= 1e11 else f"{rreso/1e3:.0f}k"
        print(f"{label:>16} {note:>4} {cutoff_itail*1e6:>9.0f}u {rr:>8} "
              f"{m['esr_db']:>8.2f} {m['nrmse']:>8.4f} {m['lag']:>5}")

    esr_vals = [r["esr_db"] for r in results.values()]
    summary = {
        "_source": "eval/303/compare_voice.py (F1.4); SPICE chained decks vs Rust Low/TPT voice",
        "_method": "RMS-normalised, lag-aligned ESR (BA283); cutoff/reso unwired in voice (F1)",
        "sr": SR, "secs": SECS,
        "esr_db_mean": float(np.mean(esr_vals)),
        "esr_db_worst": float(np.max(esr_vals)),
        "op_points": results,
    }
    out = HERE / "voice_esr_baseline.json"
    out.write_text(json.dumps(summary, indent=2) + "\n")
    print(f"\nmean ESR {summary['esr_db_mean']:.2f} dB  worst {summary['esr_db_worst']:.2f} dB"
          f"  -> {out.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
