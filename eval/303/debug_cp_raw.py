"""Debug: inspect raw v(cp) from SPICE to verify it's oscillating properly.

    uv run --no-project --with PySpice --with numpy python eval/303/debug_cp_raw.py
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

# Use a known-good CV from run_vco303_spice.py diagnostics (cv=5.0 ≈ 477 Hz, runs fine)
# and a smaller duration so it's fast
CV = 5.0
SETTLE = 0.03
SECS = 0.05  # ~24 cycles at 477 Hz
STEP = 1.25e-6

from PySpice.Spice.Netlist import Circuit  # noqa: E402

c = Circuit("vco303dbg")
c.raw_spice += gen.TRANSISTOR_MODELS
c.raw_spice += "Vvp vp 0 PWL(0 0 1m 12)\n"
c.raw_spice += "Vvh vh 0 PWL(0 0 1m 5.333)\n"
c.raw_spice += f"Vcvb cvb 0 PWL(0 0 1m {-4.7062 + CV})\n"
c.raw_spice += "Vcv cv 0 0\n"
gen.add_elements(c)

with tempfile.TemporaryDirectory() as d:
    out_or = Path(d) / "vco_or.dat"
    out_cp = Path(d) / "vco_cp.dat"
    cir = Path(d) / "vco.cir"
    deck = (
        str(c)
        + "\n.options gmin=1e-10 reltol=1e-3 abstol=1e-9 vntol=1e-6 method=gear\n"
        + ".control\n"
        + f"tran {STEP:.8e} {SETTLE + SECS:.8e}\n"
        + f"wrdata {out_or} v(o_r)\n"
        + f"wrdata {out_cp} v(cp)\n"
        + ".endc\n.end\n"
    )
    cir.write_text(deck)
    result = subprocess.run(["ngspice", "-b", str(cir)], capture_output=True, timeout=120)
    stderr = result.stderr.decode()

    if not out_or.exists():
        print("o_r output missing. stderr:\n", stderr[:3000])
        sys.exit(1)

    data_or = np.loadtxt(out_or)
    data_cp = np.loadtxt(out_cp)

print(f"o_r shape: {data_or.shape}, cp shape: {data_cp.shape}")
print(f"o_r columns: {data_or.shape[1] if data_or.ndim > 1 else 1}")
print()

# Show what's in each file
if data_or.ndim == 1:
    print("WARNING: o_r has only 1 column — interleaved t,v?")
    t_or = data_or[0::2]
    v_or = data_or[1::2]
else:
    t_or, v_or = data_or[:, 0], data_or[:, 1]

if data_cp.ndim == 1:
    print("WARNING: cp has only 1 column — interleaved t,v?")
    t_cp = data_cp[0::2]
    v_cp = data_cp[1::2]
else:
    t_cp, v_cp = data_cp[:, 0], data_cp[:, 1]

mask = t_cp > SETTLE
t_ss = t_cp[mask]
cp_ss = v_cp[mask]
print(f"cp settled: {len(cp_ss)} points  range: {cp_ss.min():.4f}..{cp_ss.max():.4f} V")
print(f"cp p-t-p: {np.ptp(cp_ss):.4f} V")

mask_or = t_or > SETTLE
v_or_ss = v_or[mask_or]
print(f"o_r settled: range {v_or_ss.min():.4f}..{v_or_ss.max():.4f} V  p-t-p: {np.ptp(v_or_ss):.4f}")

# Show first 20 lines of each raw file
print("\n--- first 10 rows of o_r.dat ---")
for row in data_or[:10]:
    print(row)
print("\n--- first 10 rows of cp.dat ---")
for row in data_cp[:10]:
    print(row)
