"""Measure the JFET reset thresholds from the SPICE VCO using subprocess ngspice.

Runs a vco303 .tran at 4× finer step (1.25 µs) for a known-oscillating CV
and reads back v(cp) and v(o_r) to extract:
  - CP_HIGH: integrator cap voltage just before JFET fires
  - CP_LOW : integrator cap voltage just after reset
  - OR_SWING: peak-to-peak amplitude of v(o_r) in steady state

    uv run --no-project --with PySpice --with numpy python eval/303/measure_cp_thresholds.py
"""
from __future__ import annotations

import subprocess
import sys
import tempfile
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import _vco303_netlist_gen as gen  # noqa: E402

# Use cv=5.0 (≈477 Hz, known to oscillate well per run_vco303_spice.py)
# We measure the STRUCTURAL thresholds (CP_HIGH/CP_LOW) which are circuit properties,
# not note-dependent. Only OR_SWING is note-dependent; we also check at 3.4 (~154 Hz).
MEASUREMENTS = [
    ("cv=5.0 (~477 Hz)", 5.0),
    ("cv=3.4 (~154 Hz)", 3.4),
]
SETTLE = 0.03
SECS = 0.08
STEP = 1.25e-6  # 800 kHz grid — 4× finer than the standard 200 kHz

from PySpice.Spice.Netlist import Circuit  # noqa: E402


def run_vco_subprocess(cv: float) -> tuple[np.ndarray, np.ndarray, np.ndarray, np.ndarray]:
    """Run vco303 transient; return (t_or, v_or, t_cp, v_cp) raw vectors."""
    c = Circuit("vco303meas")
    c.raw_spice += gen.TRANSISTOR_MODELS
    c.raw_spice += "Vvp vp 0 PWL(0 0 1m 12)\n"
    c.raw_spice += "Vvh vh 0 PWL(0 0 1m 5.333)\n"
    c.raw_spice += f"Vcvb cvb 0 PWL(0 0 1m {-4.7062 + cv})\n"
    c.raw_spice += "Vcv cv 0 0\n"
    gen.add_elements(c)

    with tempfile.TemporaryDirectory() as d:
        out_or = Path(d) / "vco_or.dat"
        out_cp = Path(d) / "vco_cp.dat"
        cir = Path(d) / "vco.cir"
        cir.write_text(
            str(c)
            + "\n.options gmin=1e-10 reltol=1e-3 abstol=1e-9 vntol=1e-6 method=gear\n"
            + ".control\n"
            + f"tran {STEP:.8e} {SETTLE + SECS:.8e}\n"
            + f"wrdata {out_or} v(o_r)\n"
            + f"wrdata {out_cp} v(cp)\n"
            + ".endc\n.end\n"
        )
        subprocess.run(["ngspice", "-b", str(cir)], capture_output=True, timeout=300)
        data_or = np.loadtxt(out_or)
        data_cp = np.loadtxt(out_cp)

    t_or, v_or = data_or[:, 0], data_or[:, 1]
    t_cp, v_cp = data_cp[:, 0], data_cp[:, 1]
    return t_or, v_or, t_cp, v_cp


def analyse(cv_label: str, cv: float) -> None:
    print(f"\n=== {cv_label} ===")
    t_or, v_or, t_cp, v_cp = run_vco_subprocess(cv)

    extra_ms = 0.010
    mask = t_cp > (SETTLE + extra_ms)
    t_ss = t_cp[mask]
    cp_ss = v_cp[mask]
    print(f"  cp settled range: {cp_ss.min():.4f}..{cp_ss.max():.4f} V  (p-t-p {np.ptp(cp_ss):.4f})")

    # Detect resets: cp drops sharply (large negative dv/dt)
    dcp = np.diff(cp_ss)
    dts = np.diff(t_ss)
    with np.errstate(divide="ignore", invalid="ignore"):
        dcp_dt = np.where(dts > 0, dcp / dts, 0.0)

    DRESET_THRESH = -1e5  # V/s (well above noise, well below the ~1e6 V/s reset)
    reset_idx = np.where(dcp_dt < DRESET_THRESH)[0]
    if len(reset_idx) == 0:
        print("  WARNING: no reset events detected")
        return

    # Cluster consecutive reset-rate samples
    clusters: list[list[int]] = []
    cur: list[int] = [reset_idx[0]]
    for idx in reset_idx[1:]:
        if idx == cur[-1] + 1:
            cur.append(idx)
        else:
            clusters.append(cur)
            cur = [idx]
    clusters.append(cur)

    cp_highs, cp_lows = [], []
    for cl in clusters:
        pre = cl[0]
        post = cl[-1] + 1
        if post < len(cp_ss):
            cp_highs.append(float(cp_ss[pre]))
            cp_lows.append(float(cp_ss[post]))

    if not cp_highs:
        print("  WARNING: could not extract threshold pairs")
        return

    h = np.array(cp_highs)
    l = np.array(cp_lows)
    print(f"  Resets detected: {len(cp_highs)}")
    print(f"  CP_HIGH (before): mean={h.mean():.4f}  min={h.min():.4f}  max={h.max():.4f}")
    print(f"  CP_LOW  (after ): mean={l.mean():.4f}  min={l.min():.4f}  max={l.max():.4f}")

    # or_swing
    mask_or = t_or > (SETTLE + extra_ms)
    or_ss = v_or[mask_or]
    or_swing = float(np.ptp(or_ss))
    print(f"  OR_SWING (o_r p-t-p): {or_swing:.4f} V  → RESET_AMPLITUDE = {or_swing * 2.0:.4f} (× VCO_GAIN=2)")

    print(f"\n  // Rust constants for this CV:")
    print(f"  const CP_HIGH_THRESH: f64 = {h.mean():.4};")
    print(f"  const CP_LOW_THRESH:  f64 = {l.mean():.4};")
    print(f"  const RESET_AMPLITUDE: f32 = {or_swing * 2.0:.4};")


for label, cv in MEASUREMENTS:
    analyse(label, cv)
