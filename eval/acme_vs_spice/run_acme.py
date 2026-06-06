"""
Drive the ACME.jl BA283 model (``ba283_acme.jl``) on a dry stimulus.

Mirrors ``eval/wdf_vs_spice/run_wdf.py``.  The Julia script takes the gain
resistor and the two RV1 pot legs as explicit args; we compute them here from
``data_gen.netlist`` so the ACME circuit is parameterised **identically** to the
SPICE reference (single source of the bias→pot and gain→R_KJ mappings).

IO is raw little-endian Float64 binary (one mono channel) through a temp dir, so
the Julia side needs no WAV dependency.
"""
from __future__ import annotations

import os
import subprocess
import tempfile
from pathlib import Path

import numpy as np

from data_gen.netlist import RV1_WIPER_MIN

ACME_DIR = Path(__file__).resolve().parent
JULIA_SCRIPT = ACME_DIR / "ba283_acme.jl"
# juliaup drops the launcher in ~/.juliaup/bin; fall back to PATH.
JULIA_BIN = os.environ.get("JULIA_BIN") or str(Path.home() / ".juliaup/bin/julia")


def rv1_legs(bias: float) -> tuple[float, float]:
    """Map ``bias`` ∈ [0,1] to (r_upper, r_lower) ohms — identical to
    ``data_gen.netlist.build_netlist`` (lines 151-154)."""
    wiper = RV1_WIPER_MIN + (1.0 - RV1_WIPER_MIN) * bias
    r_upper = max(1.0, 4700.0 * (1.0 - wiper))
    r_lower = max(1.0, 4700.0 * wiper)
    return r_upper, r_lower


def run_acme(
    dry: np.ndarray, sr: int, rgain: float = 220.0, bias: float = 0.77,
    timeout: float = 900.0,
) -> np.ndarray:
    """Run the ACME model on *dry* samples; return wet volts at SEC_IN (float64).

    Args:
        dry:   mono dry stimulus (volts, peak ≤ 0.5).
        sr:    sample rate (Hz).
        rgain: K↔J gain shunt R_KJ in ohms (lower → more gain).
        bias:  RV1 trim ∈ [0,1] (held at 0.77 for the gain-conditioned set).

    Returns:
        Wet samples (float64 ndarray, volts), same length as *dry*.
    """
    if not JULIA_SCRIPT.exists():
        raise RuntimeError(f"missing Julia script: {JULIA_SCRIPT}")
    julia = JULIA_BIN if Path(JULIA_BIN).exists() else "julia"

    r_upper, r_lower = rv1_legs(bias)
    dry = np.ascontiguousarray(np.asarray(dry, dtype=np.float64))

    with tempfile.TemporaryDirectory(prefix="acme_run_") as _tmp:
        tmp = Path(_tmp)
        dry_bin = tmp / "dry.f64"
        out_bin = tmp / "wet.f64"
        dry_bin.write_bytes(dry.tobytes())

        result = subprocess.run(
            [
                julia, f"--project={ACME_DIR}", str(JULIA_SCRIPT),
                str(dry_bin), str(out_bin), str(int(sr)),
                repr(float(rgain)), repr(r_upper), repr(r_lower),
            ],
            capture_output=True,
            text=True,
            timeout=timeout,
        )
        if result.returncode != 0:
            raise RuntimeError(
                f"ACME (julia) failed (rc={result.returncode}):\n{result.stderr}"
            )
        if not out_bin.exists():
            raise RuntimeError(
                f"ACME ran but wrote no output:\n{result.stdout}\n{result.stderr}"
            )
        wet = np.frombuffer(out_bin.read_bytes(), dtype=np.float64)
        return np.array(wet, dtype=np.float64)  # writable copy
