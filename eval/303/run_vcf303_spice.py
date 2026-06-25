"""ngspice AC validation for the vcf303 diode-ladder core (STEP 1).

Stamps the bare 4-pole ladder from the generated ``_vcf303_netlist_gen`` and wraps
it in the missing active stages with a minimal harness:

  * bias  — ideal current sources inject I_bias down each rail at the TOP and ideal
            current sinks pull it out at the BOTTOM. r_diode = Vt/I, so the cutoff
            scales with the bias current (the parameterised cutoff control, standing
            in for the T7/T11 expo converter). Ideal sources are OPEN at AC, so the
            injected signal must climb the bias-dependent ladder (a resistive sink
            would short it to ground and the cutoff would stop tracking the current).
  * term  — a differential resistor across the top (tl,tr) stands in for the T20
            readout load and completes the AC loop; one 1 MΩ leg to ground sets the
            DC reference for the otherwise floating differential pair.
  * drive — a differential AC current source injects signal at the bottom (dl,dr).
  * read  — differential output V(tl)-V(tr) at the ladder top = the 4-pole LP.

Runs a ``.ac`` sweep at several bias currents and reports, per current, the -3 dB
cutoff and the high-frequency roll-off slope, then checks that the cutoff tracks
the bias current (f_c proportional to I).

NOTE: PySpice's bundled simulator does not support the installed ngspice 46 (it
silently returns all-zero vectors). We therefore use PySpice only to *stamp* the
generated netlist, then drive ngspice directly as a subprocess and parse `wrdata`.

    uv run --no-project --with PySpice --with numpy python eval/303/run_vcf303_spice.py
"""

from __future__ import annotations

import subprocess
import sys
import tempfile
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
import _vcf303_netlist_gen as gen  # noqa: E402

from PySpice.Spice.Netlist import Circuit  # noqa: E402

R_TOP = 1000.0  # differential readout termination across (tl,tr) — Ω
R_REF = 1e6     # DC reference leg dl->GND (open at AC; ~µV DC error) — Ω


def _netlist(i_bias_per_rail: float) -> str:
    """Stamp the ladder + harness and return a raw ngspice deck (PySpice is used
    only for netlisting; see module docstring)."""
    c = Circuit("vcf303_ac")
    c.raw_spice += gen.TRANSISTOR_MODELS
    gen.add_elements(c)
    ib = i_bias_per_rail
    # NB: emit the harness sources via raw_spice with PLAIN numbers. PySpice's
    # unit suffixes (`16e-6 @ u_A` -> "1.6e-05A", "1000Ohm") are mis-parsed by
    # ngspice 46 (trailing "A" reads as atto=1e-18 -> ~zero bias -> dead ladder).
    # The generated add_elements already emits suffix-free floats.
    c.raw_spice += (
        # Bias: inject I into each top rail, sink the same I from each bottom rail —
        # ideal current sources (open at AC) so the signal climbs the ladder.
        f"Ibias_l 0 tl {ib}\n"
        f"Ibias_r 0 tr {ib}\n"
        f"Isink_l dl 0 {ib}\n"
        f"Isink_r dr 0 {ib}\n"
        # Readout termination (completes the AC loop) + DC reference.
        f"Rtop tl tr {R_TOP}\n"
        f"Rref dl 0 {R_REF}\n"
        # Differential AC drive at the bottom (1 A AC, antiphase dl<-->dr). DC=0.
        "Iac dl dr DC 0 AC 1\n"
    )
    return str(c)


def ac_response(i_bias_per_rail: float):
    """Run ngspice on the stamped deck, return (freq[Hz], mag_db) of V(tl)-V(tr)."""
    deck = _netlist(i_bias_per_rail)
    with tempfile.TemporaryDirectory() as d:
        out = Path(d) / "ac.dat"
        cir = Path(d) / "vcf.cir"
        cir.write_text(
            deck
            + "\n.options gmin=1e-13 reltol=1e-4 abstol=1e-12 vntol=1e-7\n"
            + ".control\n"
            + "ac dec 40 1 3e6\n"
            + f"wrdata {out} vdb(tl,tr)\n"
            + ".endc\n.end\n"
        )
        subprocess.run(["ngspice", "-b", str(cir)], capture_output=True, timeout=120)
        data = np.loadtxt(out)
    return data[:, 0], data[:, 1]  # freq, magnitude(dB)


def cutoff_and_slope(f: np.ndarray, mag_db: np.ndarray):
    passband = mag_db[f < 10.0].max() if np.any(f < 10.0) else mag_db[0]
    below = np.where(mag_db <= passband - 3.0)[0]
    fc = float(f[below[0]]) if below.size else float("nan")
    # ASYMPTOTIC roll-off: fit well above fc (>=16x), where all 4 poles are active
    # (closer to fc the slope is still steepening through the poles).
    hi = (f > 16 * fc) & (f < 400 * fc) & np.isfinite(mag_db)
    slope = float("nan")
    if hi.sum() >= 2:
        slope = float(np.polyfit(np.log2(f[hi]), mag_db[hi], 1)[0])  # dB/octave
    return passband, fc, slope


def main() -> int:
    biases = [1e-6, 4e-6, 16e-6, 64e-6]  # per-rail; f_c ~ I/(Vt*C)
    print(f"{'I_rail':>8} {'passband':>9} {'f_-3dB':>10} {'slope':>12}")
    fcs = []
    for ib in biases:
        f, mag = ac_response(ib)
        pb, fc, slope = cutoff_and_slope(f, mag)
        fcs.append(fc)
        print(f"{ib*1e6:6.1f}uA {pb:7.2f}dB {fc:9.1f}Hz {slope:8.1f}dB/oct")

    ratios = [fcs[i + 1] / fcs[i] for i in range(len(fcs) - 1)
              if np.isfinite(fcs[i]) and fcs[i] > 0 and np.isfinite(fcs[i + 1])]
    tracks = len(ratios) == len(biases) - 1 and all(2.5 < r < 6.0 for r in ratios)
    _, _, mid_slope = cutoff_and_slope(*ac_response(16e-6))
    slope_ok = -26.0 < mid_slope < -21.0  # asymptotic 4-pole ~ -24 dB/oct
    print(f"\ncutoff/bias ratios (per 4x I): {[round(r, 2) for r in ratios]}")
    print("RESULT:", "4-POLE LP, cutoff tracks bias"
          if (tracks and slope_ok) else "CHECK (see numbers above)")
    return 0 if (tracks and slope_ok) else 1


if __name__ == "__main__":
    sys.exit(main())
