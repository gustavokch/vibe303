"""ngspice smoke/render for the vco303 VCO core.

Builds the PySpice circuit from the generated ``_vco303_netlist_gen.add_elements``,
drives the rails + CV inputs, seeds the ramp cap (.ic), runs a transient, and
reports the pulse/ramp output swing (and optionally writes WAVs). Confirms the
imported IR actually oscillates through ngspice.

    uv run --no-project --with PySpice --with numpy python eval/303/run_vco303_spice.py [CV]
"""

from __future__ import annotations

import sys
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
import _vco303_netlist_gen as gen  # noqa: E402

from PySpice.Spice.Netlist import Circuit  # noqa: E402

CV = float(sys.argv[1]) if len(sys.argv) > 1 else 5.0  # 5V ≈ middle C per the deck

c = Circuit("vco303")
c.raw_spice += gen.TRANSISTOR_MODELS
# Driven nets (the IR's known nodes). Ramp the rails 0->V over 1 ms (PWL) so the
# relaxation core powers up smoothly and self-starts — avoids the t=0 inconsistency
# that uic + the JFET produce. Values from vibe303/netlist.txt.
c.raw_spice += "Vvp vp 0 PWL(0 0 1m 12)\n"
c.raw_spice += "Vvh vh 0 PWL(0 0 1m 5.333)\n"
c.raw_spice += f"Vcvb cvb 0 PWL(0 0 1m {-4.7062 + CV})\n"
c.raw_spice += "Vcv cv 0 0\n"
gen.add_elements(c)

sim = c.simulator(temperature=25, nominal_temperature=25)
# ngspice is stiffer than LTspice on this relaxation core: rshunt tames the
# singular-matrix warnings on high-Z nodes, gear + relaxed tols help convergence.
sim.options(method="gear", rshunt=1e12, gmin=1e-10,
            reltol=1e-3, abstol=1e-9, vntol=1e-6, itl4=1000, trtol=7)
an = sim.transient(step_time=20e-6, end_time=150e-3)

t = np.array(an.time)
op = np.array(an["o_p"])
orr = np.array(an["o_r"])
# settle: ignore the first 20 ms
mask = t > 20e-3
op_s, or_s = op[mask], orr[mask]


def _zero_crossings(x: np.ndarray) -> int:
    x = x - x.mean()
    return int(np.sum((x[:-1] < 0) & (x[1:] >= 0)))


dur = t[mask][-1] - t[mask][0]
f_p = _zero_crossings(op_s) / dur
op_ptp, or_ptp = float(np.ptp(op_s)), float(np.ptp(or_s))
print(f"CV={CV}  o_p: ptp {op_ptp:.3f} V  ~{f_p:.1f} Hz")
print(f"         o_r: ptp {or_ptp:.3f} V")
osc = op_ptp > 0.05 and or_ptp > 0.05
print("RESULT:", "OSCILLATING" if osc else "FLAT (no oscillation)")
sys.exit(0 if osc else 1)
