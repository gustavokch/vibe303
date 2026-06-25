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

V_RAIL = 8.0    # +8V analog rail (vp8) — top bias + T38 collector return
V_BIAS = 4.0    # T38 base DC bias (in_l/in_r); keeps the pair in forward-active

# STEP 2a: the bare-ladder harness sources (ideal Ibias/Isink/Iac) are GONE — the
# real T38 differential input stage now drives the ladder bottom. We supply only
# the +8V rail, the T38 base bias, and the parameterised cutoff = the T38 tail
# sink current (= the expo-converter output, per VCF_MAP.md item 3). The signal
# is the differential base AC. tail current = TOTAL ladder bias = 2x the per-rail
# current of step 1 (the pair splits it), so f_c ~ (I_tail/2)/(Vt*C).


def _harness(i_tail: float) -> str:
    """The +8V rail, T38 base bias + AC drive, and the cutoff tail sink. PLAIN
    numbers only (ngspice-46 mis-parses PySpice unit suffixes — see docstring)."""
    return (
        f"Vrail vp8 0 {V_RAIL}\n"
        # Single-ended AC into the diff pair: in_l = bias + AC, in_r = bias (AC gnd).
        f"Vin_l in_l 0 DC {V_BIAS} AC 1\n"
        f"Vin_r in_r 0 DC {V_BIAS}\n"
        # Cutoff: ideal current sink pulls I_tail out of the shared emitter tail
        # (open at AC, so it does not load the signal). = the expo bias current.
        f"Itail tail 0 {i_tail}\n"
    )


def _netlist(i_tail: float) -> str:
    """Stamp the ladder + T38 stage + harness; return a raw ngspice deck."""
    c = Circuit("vcf303_ac")
    c.raw_spice += gen.TRANSISTOR_MODELS
    gen.add_elements(c)
    c.raw_spice += _harness(i_tail)
    return str(c)


def op_point(i_tail: float) -> str:
    """Run a DC .op and return the node-voltage table (convergence sanity check)."""
    deck = _netlist(i_tail)
    with tempfile.TemporaryDirectory() as d:
        cir = Path(d) / "vcf.cir"
        cir.write_text(
            deck
            + "\n.options gmin=1e-13 reltol=1e-4 abstol=1e-12 vntol=1e-7\n"
            + ".control\nop\nprint v(tl) v(dl) v(eL) v(tail) v(tr) v(dr)\n.endc\n.end\n"
        )
        r = subprocess.run(["ngspice", "-b", str(cir)], capture_output=True,
                            text=True, timeout=120)
        return r.stdout + r.stderr


def ac_response(i_tail: float):
    """Run ngspice on the stamped deck, return (freq[Hz], mag_db) of V(tl)-V(tr)."""
    deck = _netlist(i_tail)
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


F_DEEP = -60.0  # deep-stopband level (dB) used as the bias-robust cutoff metric


def _cross(f, mag_db, level):
    """First frequency where mag_db falls to `level` (linear interp in log f)."""
    below = np.where(mag_db <= level)[0]
    if not below.size or below[0] == 0:
        return float("nan")
    i = below[0]
    f0, f1, m0, m1 = f[i - 1], f[i], mag_db[i - 1], mag_db[i]
    t = (level - m0) / (m1 - m0) if m1 != m0 else 0.0
    return float(np.exp(np.log(f0) + t * (np.log(f1) - np.log(f0))))


def cutoff_and_slope(f: np.ndarray, mag_db: np.ndarray):
    """Returns (passband_dB, f_deep, slope). NOTE the step-2a passband gain scales
    with the bias current (diff-pair gm = I/2Vt → peaking), so the classic
    -3dB-from-peak corner is bias-dependent and a poor tracking metric; the
    bias-robust cutoff is the DEEP-stopband crossing (F_DEEP), where all four
    poles are engaged regardless of passband gain. The T20 gain stage (step 2b)
    normalises the passband; until then we track on F_DEEP."""
    passband = float(mag_db[f < 10.0].max() if np.any(f < 10.0) else mag_db[0])
    f_deep = _cross(f, mag_db, F_DEEP)
    # ASYMPTOTIC roll-off: fit the deep region (-120..-40 dB), all 4 poles active.
    hi = (mag_db < -40.0) & (mag_db > -130.0) & np.isfinite(mag_db)
    slope = float("nan")
    if hi.sum() >= 2:
        slope = float(np.polyfit(np.log2(f[hi]), mag_db[hi], 1)[0])  # dB/octave
    return passband, f_deep, slope


def main() -> int:
    # `--op` prints the DC operating point (T38-stage convergence sanity check).
    if "--op" in sys.argv:
        for it in [2e-6, 32e-6, 128e-6]:
            print(f"=== I_tail = {it*1e6:.1f} uA ===")
            print(op_point(it))
        return 0

    biases = [2e-6, 8e-6, 32e-6, 128e-6]  # tail (total); per-rail = I/2, f_c ~ I/(2 Vt C)
    print(f"{'I_tail':>8} {'passband':>9} {'f_-60dB':>10} {'slope':>12}")
    fcs = []
    for ib in biases:
        f, mag = ac_response(ib)
        pb, fc, slope = cutoff_and_slope(f, mag)
        fcs.append(fc)
        print(f"{ib*1e6:6.1f}uA {pb:7.2f}dB {fc:9.1f}Hz {slope:8.1f}dB/oct")

    ratios = [fcs[i + 1] / fcs[i] for i in range(len(fcs) - 1)
              if np.isfinite(fcs[i]) and fcs[i] > 0 and np.isfinite(fcs[i + 1])]
    tracks = len(ratios) == len(biases) - 1 and all(2.5 < r < 6.0 for r in ratios)
    _, _, mid_slope = cutoff_and_slope(*ac_response(32e-6))
    slope_ok = -26.0 < mid_slope < -21.0  # asymptotic 4-pole ~ -24 dB/oct
    print(f"\nf_-60dB / bias ratios (per 4x I): {[round(r, 2) for r in ratios]}")
    print("RESULT:", "4-POLE LP via T38 input stage, cutoff tracks bias"
          if (tracks and slope_ok) else "CHECK (see numbers above)")
    return 0 if (tracks and slope_ok) else 1


if __name__ == "__main__":
    sys.exit(main())
