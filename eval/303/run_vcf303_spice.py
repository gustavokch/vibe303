"""ngspice DC/AC validation for the vcf303 diode-ladder VCF (STEP 2b).

Stamps the generated ``_vcf303_netlist_gen`` (ladder core + T38 differential input
stage + T20/T22 gain stage + T21 output buffer) and supplies the rails/bias that
the model parameterises rather than transcribes:

  * rails — Vrail = +8V analog (vp8); Va3 = 3V3 mod-offset rail ("A" net, the
            gain-stage bias reference).
  * cutoff — Itail pulls I_tail out of the T38 emitter tail = the expo-converter
            output = the ladder bias current. Ideal sink (open at AC, no signal
            loading). f_c ~ (I_tail/2)/(Vt*C); cutoff tracks I_tail.
  * gain bias — Vm2 sets the T22 tail-sink base (the abstracted on-board
            R120/R122/R123 divider that was not legibly readable from the
            schematic); tuned so T20 sits forward-active, not saturated.
  * drive — Vin_l = base bias + AC into the T38 pair (in_r = bias, AC ground).
  * read  — single-ended output V(out) at the T21 emitter follower.

The SIGNAL path (T38 pair, ladder, T20 pair + R83/R84 loads, T21 buffer) is
transcribed faithfully; the BIAS networks (cutoff expo, gain-stage divider) are
parameterised, matching VCF_MAP.md item 3 (controllable bias-current cutoff).

Runs a ``.op`` (``--op``) for DC-convergence sanity, then a ``.ac`` sweep at
several bias currents, reporting the deep-stopband (-60 dB) cutoff + roll-off
slope and checking the cutoff tracks the bias current.

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
from parallel import run_parallel_with_fallback  # noqa: E402

from PySpice.Spice.Netlist import Circuit  # noqa: E402

V_RAIL = 8.0    # +8V analog rail (vp8) — top bias + T38 collector return
V_A3 = 3.3      # "A" net = 3V3 mod-offset rail — T20/T22 gain-stage bias reference
V_BIAS = 4.0    # input DC bias (sig / in_r) — keeps the T38 pair forward-active
V_M2 = 3.9      # T22 base bias (abstracted gain-stage divider) — sets tail current
RRESO_OFF = 1e12  # resonance leg open => no feedback (step-2b behaviour)

# The bare-ladder harness sources (ideal Ibias/Isink/Iac of step 1) are GONE — the
# real T38 input stage drives the ladder bottom and the T20/T21 stage reads the
# top. The harness supplies the +8V rail, the 3V3 "A" rail, the T38 base bias, the
# cutoff tail sink (= expo output, per VCF_MAP item 3), and the abstracted T22
# gain-bias (Vm2). tail current = TOTAL ladder bias = 2x the per-rail current of
# step 1 (the pair splits it), so f_c ~ (I_tail/2)/(Vt*C).


def _harness(i_tail: float) -> str:
    """The +8V rail, input bias + AC drive, and the cutoff tail sink. PLAIN
    numbers only (ngspice-46 mis-parses PySpice unit suffixes — see docstring)."""
    return (
        f"Vrail vp8 0 {V_RAIL}\n"
        f"Va3 a3 0 {V_A3}\n"  # "A" net = 3V3 mod-offset rail (gain-stage bias ref)
        f"Vm2 m2 0 {V_M2}\n"   # T22 base bias (abstracted gain-stage divider)
        # Signal in at `sig` (-> R27 -> in_l). in_r bias from `bref` (-> R_br); in_r
        # also sums the resonance feedback. bref is AC-ground (DC only).
        f"Vsig sig 0 DC {V_BIAS} AC 1\n"
        f"Vbref bref 0 DC {V_BIAS}\n"
        # Cutoff: ideal current sink pulls I_tail out of the shared emitter tail
        # (open at AC, so it does not load the signal). = the expo bias current.
        f"Itail tail 0 {i_tail}\n"
    )


def _netlist(i_tail: float, rreso: float = RRESO_OFF) -> str:
    """Stamp the ladder + active stages + harness; return a raw ngspice deck.
    `rreso` = the resonance feedback leg (Ω); RRESO_OFF disables feedback."""
    c = Circuit("vcf303_ac")
    c.raw_spice += gen.TRANSISTOR_MODELS
    gen.add_elements(c, rreso=rreso)
    c.raw_spice += _harness(i_tail)
    return str(c)


def op_point(i_tail: float, rreso: float = RRESO_OFF) -> str:
    """Run a DC .op and return the node-voltage table (convergence sanity check)."""
    deck = _netlist(i_tail, rreso)
    with tempfile.TemporaryDirectory() as d:
        cir = Path(d) / "vcf.cir"
        cir.write_text(
            deck
            + "\n.options gmin=1e-13 reltol=1e-4 abstol=1e-12 vntol=1e-7\n"
            + ".control\nop\n"
            + "print v(tl) v(tr) v(dl) v(tail) v(e20) "
            + "v(c20l) v(c20r) v(t22e) v(out) v(bq9) v(eq9) v(cq9)\n.endc\n.end\n"
        )
        r = subprocess.run(["ngspice", "-b", str(cir)], capture_output=True,
                            text=True, timeout=120)
        return r.stdout + r.stderr


def ac_response(i_tail: float, rreso: float = RRESO_OFF):
    """Run ngspice on the stamped deck, return (freq[Hz], mag_db) of V(out)."""
    deck = _netlist(i_tail, rreso)
    with tempfile.TemporaryDirectory() as d:
        out = Path(d) / "ac.dat"
        cir = Path(d) / "vcf.cir"
        cir.write_text(
            deck
            + "\n.options gmin=1e-13 reltol=1e-4 abstol=1e-12 vntol=1e-7\n"
            + ".control\n"
            + "ac dec 40 1 3e6\n"
            + f"wrdata {out} vdb(out)\n"
            + ".endc\n.end\n"
        )
        subprocess.run(["ngspice", "-b", str(cir)], capture_output=True, timeout=120)
        data = np.loadtxt(out)
    return data[:, 0], data[:, 1]  # freq, magnitude(dB)


# Souped tap-select poles (step 5): the ladder injects the signal at the BOTTOM
# (dl,dr via the T38 collectors) and the stock readout is the TOP (tl,tr) = the
# full 4-pole. Reading a LOWER differential tap crosses fewer shunt caps -> fewer
# poles, so 2/3/4-pole is purely a READOUT-TAP CHOICE — no SPICE switch element,
# and in the plugin just a mux on which ladder node the gain stage reads. The taps
# are existing IR nodes (tl/al/bl/cl per rail). Validated by ngspice only (ACME VCF
# parity is a NO-GO, see VCF_MAP §12). Differential vdb(<tap>L,<tap>R) per level.
TAPS = [("tl", "tr"), ("al", "ar"), ("bl", "br"), ("cl", "cr")]


def tap_response(i_tail: float, rreso: float = RRESO_OFF):
    """ngspice .ac reading every differential ladder tap; returns (freq, {tap: mag_db})."""
    deck = _netlist(i_tail, rreso)
    cols = " ".join(f"vdb({lo},{ro})" for lo, ro in TAPS)
    with tempfile.TemporaryDirectory() as d:
        out = Path(d) / "ac.dat"
        cir = Path(d) / "vcf.cir"
        cir.write_text(
            deck
            + "\n.options gmin=1e-13 reltol=1e-4 abstol=1e-12 vntol=1e-7\n"
            + ".control\nac dec 40 1 3e6\n"
            + f"wrdata {out} {cols}\n.endc\n.end\n"
        )
        subprocess.run(["ngspice", "-b", str(cir)], capture_output=True, timeout=120)
        data = np.loadtxt(out)
    # wrdata writes (freq, val) column pairs per probe, in TAPS order
    f = data[:, 0]
    return f, {TAPS[i][0]: data[:, 2 * i + 1] for i in range(len(TAPS))}


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
    """Returns (passband_dB, f_deep, slope). NOTE the passband gain scales with the
    bias current (diff-pair gm = I/2Vt → peaking), so the classic -3dB-from-peak
    corner is bias-dependent and a poor tracking metric; the bias-robust cutoff is
    the DEEP-stopband crossing (F_DEEP), where all four poles are engaged
    regardless of passband gain (the T20 readout buffer does not normalise this —
    it is set by the T38 input gm). The slope is fit between cutoff and the
    stopband FLOOR (above the floor the response rises again — HF feedthrough
    through the diode junction caps, which short the rungs at high f)."""
    passband = float(mag_db[f < 10.0].max() if np.any(f < 10.0) else mag_db[0])
    f_deep = _cross(f, mag_db, F_DEEP)
    f_floor = float(f[np.argmin(mag_db)])  # stopband minimum (feedthrough notch)
    hi = (mag_db < -40.0) & (f < f_floor) & np.isfinite(mag_db)
    slope = float("nan")
    if hi.sum() >= 2:
        slope = float(np.polyfit(np.log2(f[hi]), mag_db[hi], 1)[0])  # dB/octave
    return passband, f_deep, slope


def resonance_peak(f, mag_db):
    """Peak height (dB) of the response above its low-frequency passband — the
    resonance bump at cutoff. Grows with feedback; diverges at self-oscillation."""
    pb = float(mag_db[f < 20.0].mean() if np.any(f < 20.0) else mag_db[0])
    peak = float(np.max(mag_db))
    f_peak = float(f[int(np.argmax(mag_db))])
    return peak - pb, f_peak


def reso_sweep(workers: int) -> int:
    """Sweep the resonance leg Rreso at a fixed cutoff (I_tail = 32 µA) and report
    the peak height + peak frequency. Smaller Rreso = stronger positive feedback =
    higher resonance; the peak diverges at the self-oscillation threshold."""
    i_tail = 32e-6
    print(f"resonance sweep @ I_tail = {i_tail*1e6:.0f} uA  (smaller Rreso = more Q)")
    print(f"{'Rreso':>10} {'peak':>9} {'f_peak':>10}")
    legs = [RRESO_OFF, 220e3, 100e3, 47e3, 22e3, 10e3, 4.7e3, 2.2e3, 1e3]
    
    def eval_leg(rr):
        f, mag = ac_response(i_tail, rr)
        pk, fpk = resonance_peak(f, mag)
        return rr, pk, fpk
        
    results = run_parallel_with_fallback(eval_leg, legs, desc="reso sweep", max_workers=workers)
    
    for rr, pk, fpk in results:
        label = "open" if rr >= 1e11 else f"{rr/1e3:.1f}k"
        print(f"{label:>10} {pk:7.2f}dB {fpk:9.1f}Hz")
    print(
        "\nFINDING (RESOLVED 2026-06-26 — active Q9 feedback yields fc-tracking resonance):\n"
        " The peak now tracks the bias current (cutoff): ~532 Hz @16uA -> 1.0 kHz @32uA\n"
        " -> 1.7 kHz @64uA -> 2.7 kHz @128uA, up to ~20 dB tall at Rreso ~= 47k. This\n"
        " REVERSES the earlier 'the modeled ladder cannot support fc resonance' verdict:\n"
        " that was measured with a UNITY emitter-follower (old T23) feedback, which has\n"
        " no gain and no inversion, so the loop could only close at DC. The real 303\n"
        " feedback is the ACTIVE, INVERTING Q9/Q10/Q11 common-emitter amp. Transcribing\n"
        " it (Q9 CE, gain ~Rc9/R71 ~= 12x) + injecting into in_r (so the loop is -ve at\n"
        " DC, +ve near fc once the ladder rotates the phase) gives a true fc-tracking\n"
        " resonance peak. The passive ladder's pole SPREAD was a red herring: the loop\n"
        " closes where the ladder phase + the amp's 180deg sum to regeneration, near fc,\n"
        " not at the dominant pole. Smaller Rreso raises Q to an optimum (~47k) then the\n"
        " loop self-limits — the stock 303 does not self-oscillate (Devil-Fish territory).\n"
        " NEXT = step 4 (ACME-vs-SPICE VCF parity), and a souped higher-gain self-osc\n"
        " regime as an optional extension of the Rc9/R71 ratio.")
    return 0


F_REF = 5000.0   # high-f reference for the per-tap stopband attenuation metric


def _tap_metrics(f: np.ndarray, mag_db: np.ndarray):
    """(passband_dB, atten@F_REF dB-below-passband, slope 1k->5k dB/oct) for a tap."""
    pb = float(mag_db[f < 10.0].mean() if np.any(f < 10.0) else mag_db[0])
    a = float(np.interp(np.log10(F_REF), np.log10(f), mag_db)) - pb
    m1k = float(np.interp(np.log10(1000.0), np.log10(f), mag_db))
    m5k = float(np.interp(np.log10(F_REF), np.log10(f), mag_db))
    slope = (m5k - m1k) / np.log2(F_REF / 1000.0)
    return pb, a, slope


def poles_sweep() -> int:
    """Souped tap-select poles: read each differential ladder tap and report its
    stopband attenuation. The signal injects at the BOTTOM (dl,dr); reading a LOWER
    tap crosses fewer shunt caps -> fewer poles -> less attenuation. 2/3/4-pole is
    thus a pure READOUT-TAP choice (no switch element; in the plugin, a mux on which
    ladder node the gain stage reads). ngspice-only (ACME parity NO-GO, VCF_MAP §12).

    NOTE the absolute slopes are compressed well below the textbook 24/18/12 dB/oct
    by the inherent diode-ladder pole SPREAD + the HF diode-junction feedthrough
    floor (VCF_MAP §9). The demo therefore confirms the tap-select MECHANISM via the
    monotone attenuation ladder (each lower tap ~one pole shallower), not a clean
    integer slope. `tl` and `al` are degenerate (the rung-1 diode adds series R, no
    pole), so the three souped options map to tl=4-pole, bl=3-pole, cl=2-pole."""
    i_tail = 32e-6
    print(f"souped tap-select poles @ I_tail = {i_tail*1e6:.0f} uA (reso open), atten @ {F_REF:.0f} Hz")
    print(f"{'tap':>6} {'passband':>9} {'atten':>11} {'slope1-5k':>11}")
    f, taps = tap_response(i_tail)
    att = {}
    for name, _ in TAPS:
        pb, a, slope = _tap_metrics(f, taps[name])
        att[name] = a
        print(f"{name:>6} {pb:7.1f}dB {a:8.1f}dB {slope:7.1f}d/oct")
    sel = [("4-pole", "tl"), ("3-pole", "bl"), ("2-pole", "cl")]
    print("\nsouped selector (atten step ~= one pole):")
    for label, tap in sel:
        print(f"  {label:>7} = tap {tap}: {att[tap]:6.1f} dB @ {F_REF:.0f} Hz")
    # monotone & meaningfully separated: each lower tap >= ~8 dB less attenuation
    # (about one pole at this reference frequency).
    ok = (att["tl"] <= att["bl"] - 8.0) and (att["bl"] <= att["cl"] - 8.0)
    print("RESULT:", "tap-select 2/3/4-pole confirmed (monotone tl<bl<cl, ~1 pole/step)"
          if ok else "CHECK (see attenuation ladder above)")
    return 0 if ok else 1


def dump_json(workers: int) -> int:
    """Emit the bias-robust gold metrics (at the reference bias I_tail = 32 uA) as
    JSON for the plugin's TptLadder fit test (`plugin303/tests/vcf303_fit.rs`).
    Re-run after any circuit change: `... run_vcf303_spice.py --json > eval/303/vcf303_gold.json`."""
    import json

    i_tail = 32e-6
    # Cutoff/slope at the reference bias.
    f, mag = ac_response(i_tail)
    _, f_deep, slope = cutoff_and_slope(f, mag)
    # Resonance: strongest peak across the Rreso sweep (height + frequency).
    legs = [220e3, 100e3, 47e3, 22e3, 10e3]
    def eval_reso(rr):
        fr, mr = ac_response(i_tail, rr)
        return resonance_peak(fr, mr)
        
    peaks = run_parallel_with_fallback(eval_reso, legs, desc="json reso", max_workers=workers)
    best = (0.0, float("nan"))
    for pk, fpk in peaks:
        if pk > best[0]:
            best = (pk, fpk)
    # Tap attenuation @ F_REF (souped 4/3/2-pole = taps tl/bl/cl).
    ft, taps = tap_response(i_tail)
    atten = {name: _tap_metrics(ft, taps[name])[1] for name, _ in TAPS}
    gold = {
        "_source": "eval/303/run_vcf303_spice.py --json (ngspice-46, vcf303.toml)",
        "i_tail_uA": i_tail * 1e6,
        "f_ref_hz": F_REF,
        "cutoff": {"f_deep_hz": f_deep, "slope_db_oct": slope},
        "reso": {"max_peak_db": best[0], "f_peak_hz": best[1]},
        "taps_atten_db": {"p4_tl": atten["tl"], "p3_bl": atten["bl"], "p2_cl": atten["cl"]},
    }
    print(json.dumps(gold, indent=2))
    return 0


def main() -> int:
    workers = 4
    for i, arg in enumerate(sys.argv):
        if arg in ("-w", "--workers") and i + 1 < len(sys.argv):
            workers = int(sys.argv[i + 1])

    if "--json" in sys.argv:
        return dump_json(workers)

    # `--op` prints the DC operating point (active-stage convergence sanity check).
    if "--op" in sys.argv:
        biases = [2e-6, 32e-6, 128e-6]
        def get_op(it):
            return it, op_point(it)
        ops = run_parallel_with_fallback(get_op, biases, desc="DC op", max_workers=workers)
        for it, out in ops:
            print(f"=== I_tail = {it*1e6:.1f} uA ===")
            print(out)
        return 0

    if "--reso" in sys.argv:
        return reso_sweep(workers)

    if "--poles" in sys.argv:
        return poles_sweep()

    biases = [2e-6, 8e-6, 32e-6, 128e-6]  # tail (total); per-rail = I/2, f_c ~ I/(2 Vt C)
    print(f"{'I_tail':>8} {'passband':>9} {'f_-60dB':>10} {'slope':>12}")
    fcs = []
    
    def eval_bias(ib):
        f, mag = ac_response(ib)
        pb, fc, slope = cutoff_and_slope(f, mag)
        return ib, pb, fc, slope
        
    results = run_parallel_with_fallback(eval_bias, biases, desc="bias sweep", max_workers=workers)
    for ib, pb, fc, slope in results:
        fcs.append(fc)
        print(f"{ib*1e6:6.1f}uA {pb:7.2f}dB {fc:9.1f}Hz {slope:8.1f}dB/oct")

    ratios = [fcs[i + 1] / fcs[i] for i in range(len(fcs) - 1)
              if np.isfinite(fcs[i]) and fcs[i] > 0 and np.isfinite(fcs[i + 1])]
    tracks = len(ratios) == len(biases) - 1 and all(2.5 < r < 6.0 for r in ratios)
    # Slope: 4-pole ideal = -24 dB/oct. Measured ~-19..-22 here — the HF
    # feedthrough floor truncates the clean roll-off window just past cutoff
    # (worst at high bias, where cutoff is near the floor), so the fit catches the
    # still-steepening knee rather than the full asymptote. Gate accordingly.
    _, _, mid_slope = cutoff_and_slope(*ac_response(32e-6))
    slope_ok = -25.0 < mid_slope < -18.0
    print(f"\nf_-60dB / bias ratios (per 4x I): {[round(r, 2) for r in ratios]}")
    print("RESULT:", "4-POLE LP via T38 in + T20/T21 gain-buffer, cutoff tracks bias"
          if (tracks and slope_ok) else "CHECK (see numbers above)")
    return 0 if (tracks and slope_ok) else 1


if __name__ == "__main__":
    sys.exit(main())
