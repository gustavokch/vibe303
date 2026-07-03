"""Quick diagnostic to look at the raw dv/dt histogram of v(cp).

    uv run --no-project --with PySpice --with numpy python eval/303/debug_dvdt.py
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

from PySpice.Spice.Netlist import Circuit  # noqa: E402

CV = 5.0
SETTLE = 0.03
SECS = 0.05
STEP = 1.25e-6

c = Circuit("vco303dbg")
c.raw_spice += gen.TRANSISTOR_MODELS
c.raw_spice += "Vvp vp 0 PWL(0 0 1m 12)\n"
c.raw_spice += "Vvh vh 0 PWL(0 0 1m 5.333)\n"
c.raw_spice += f"Vcvb cvb 0 PWL(0 0 1m {-4.7062 + CV})\n"
c.raw_spice += "Vcv cv 0 0\n"
gen.add_elements(c)

with tempfile.TemporaryDirectory() as d:
    out_cp = Path(d) / "vco_cp.dat"
    cir = Path(d) / "vco.cir"
    cir.write_text(
        str(c)
        + "\n.options gmin=1e-10 reltol=1e-3 abstol=1e-9 vntol=1e-6 method=gear\n"
        + ".control\n"
        + f"tran {STEP:.8e} {SETTLE + SECS:.8e}\n"
        + f"wrdata {out_cp} v(cp)\n"
        + ".endc\n.end\n"
    )
    subprocess.run(["ngspice", "-b", str(cir)], capture_output=True, timeout=120)
    data = np.loadtxt(out_cp)

t, v = data[:, 0], data[:, 1]
mask = t > SETTLE
t_ss, v_ss = t[mask], v[mask]

dv = np.diff(v_ss)
dt = np.diff(t_ss)
with np.errstate(divide="ignore", invalid="ignore"):
    dvdt = np.where(dt > 0, dv / dt, 0.0)

print(f"CP settled: {len(v_ss)} points  range {v_ss.min():.4f}..{v_ss.max():.4f} V")
print(f"dv (per timestep) min/max: {dv.min():.6f}  {dv.max():.6f}")
print(f"dt (per timestep) min/max: {dt.min():.2e}  {dt.max():.2e}")
print(f"dv/dt min/max: {dvdt.min():.2e}  {dvdt.max():.2e}")

# Print the most-negative dv/dt events (top 10)
idx_neg = np.argsort(dvdt)[:20]
print("\nTop 20 most-negative dv/dt samples:")
for i in idx_neg:
    print(f"  t={t_ss[i]:.6f}s  cp={v_ss[i]:.4f} V  dcp={dv[i]:.6f}  dt={dt[i]:.2e}  dv/dt={dvdt[i]:.3e}")
