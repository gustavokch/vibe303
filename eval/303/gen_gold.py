"""Generate the committed SPICE gold waveform for the Rust ESR gate (tests/voice_esr.rs).

Renders the whole-voice SPICE reference at one fixed operating point, RMS-normalises it,
and writes `<f4` PCM + a JSON sidecar (note, cutoff_hz, reso, sr, secs). Re-run only when
the reference pipeline changes.

  uv run --no-project --with PySpice --with numpy python eval/303/gen_gold.py
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import voice_ref_spice as vr  # noqa: E402

NOTE = 57
ITAIL = 32e-6
RRESO = 47e3
SR = 48_000.0
SECS = 0.2


def main() -> int:
    y = vr.voice_reference(NOTE, ITAIL, RRESO, sr=SR, secs=SECS)
    rms = float(np.sqrt(np.mean(y**2)))
    y = (y / rms).astype("<f4")  # RMS-normalised, little-endian f32

    out_dir = HERE / "gold"
    out_dir.mkdir(exist_ok=True)
    (out_dir / "voice_A2_mid.f32").write_bytes(y.tobytes())
    sidecar = {
        "note": NOTE,
        "cutoff_hz": vr.itail_to_hz(ITAIL),
        "reso": vr.rreso_to_reso(RRESO),
        "sr": SR,
        "secs": SECS,
        "samples": int(y.size),
        "_source": "eval/303/gen_gold.py — whole-voice SPICE ref, RMS-normalised",
    }
    (out_dir / "voice_A2_mid.json").write_text(json.dumps(sidecar, indent=2) + "\n")
    print(f"wrote {y.size} samples; cutoff {sidecar['cutoff_hz']:.1f} Hz reso {sidecar['reso']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
