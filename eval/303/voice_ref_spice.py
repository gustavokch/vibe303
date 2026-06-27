"""Whole-voice SPICE transient reference: vco303 .tran -> PWL -> vcf303 .tran -> VCA.

Chains the Phase-1 SPICE decks the same way the analog 303 voice flows:

  1. note -> VCO control voltage (calibrated from measured vco303 .tran points).
  2. ngspice .tran on vco303 (ramped-rail self-start + Phase-1 solver opts), read the
     ramp/saw output `o_r`, resampled to a uniform `sr` grid.
  3. that saw, DC-biased into the vcf303 input `sig`, drives a vcf303 .tran with the
     cutoff tail sink `Itail` and resonance leg `Rreso`; read V(out).
  4. multiply by a linear AD VCA envelope matching `dsp/env.rs` defaults.

Returns a float64 array at `sr`.

ngspice-46 notes (Phase-1 HANDOFF): decks emit sources as PLAIN numbers (unit
suffixes mis-parse as atto); `np.ptp(x)` not `ndarray.ptp()`. VCO uses PySpice's
simulator (verified to drive ngspice-46 for this transient); VCF uses a subprocess
ngspice deck so the PWL drive can be a file-free inline source we fully control.

    uv run --no-project --with PySpice --with numpy --with pytest \
        python -m pytest eval/303/test_voice_ref.py -q
"""

from __future__ import annotations

import subprocess
import sys
import tempfile
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
import _vco303_netlist_gen as vco_gen  # noqa: E402
import _vcf303_netlist_gen as vcf_gen  # noqa: E402

from PySpice.Spice.Netlist import Circuit  # noqa: E402

# --- VCF bias network (from run_vcf303_spice.py) ----------------------------
V_RAIL = 8.0    # +8V analog rail (vp8)
V_A3 = 3.3      # 3V3 mod-offset rail ("A" net)
V_BIAS = 4.0    # input DC bias (sig / bref) — keeps the T38 pair forward-active
V_M2 = 3.9      # T22 base bias (abstracted gain-stage divider)

# --- VCO control-voltage calibration ----------------------------------------
# (CV, measured o_p frequency) from run_vco303_spice.py .tran. The VCO is slightly
# sub-1V/oct; fit CV as a quadratic in log2(freq) and invert to pitch the saw.
_VCO_CAL = np.array([
    [2.9, 107.7],
    [3.4, 153.9],
    [4.0, 231.0],
    [5.0, 477.0],
    [6.0, 1000.0],
])
_CV_FIT = np.polyfit(np.log2(_VCO_CAL[:, 1]), _VCO_CAL[:, 0], 2)


def note_to_freq(note: float) -> float:
    """Equal-tempered MIDI note -> Hz. Mirrors Rust `dsp::voice::midi_to_freq`."""
    return 440.0 * 2.0 ** ((note - 69.0) / 12.0)


def cv_for_freq(freq: float) -> float:
    """VCO control voltage that yields `freq` (inverse of the measured calibration).
    This is only a seed — the 5-point fit is ~10% off, so `solve_cv` refines it to
    a tight pitch (frequency mismatch is NOT removed by ESR lag-alignment)."""
    return float(np.polyval(_CV_FIT, np.log2(freq)))


def _measure_freq(saw: np.ndarray, sr: float) -> float:
    """Fundamental of a (centered) saw via rising zero-crossings — one per period."""
    x = saw - saw.mean()
    zc = int(np.sum((x[:-1] < 0) & (x[1:] >= 0)))
    return zc * sr / len(x)


def solve_cv(target: float, sr: float, secs: float = 0.1,
             tol: float = 0.004, iters: int = 5) -> float:
    """Secant-solve the control voltage so the VCO's measured pitch hits `target` Hz.
    Frequency is ~exponential in CV, so the secant runs on log2(freq)."""
    cv0 = cv_for_freq(target)
    f0 = _measure_freq(render_vco(cv0, sr, secs), sr)
    cv1 = cv0 + 0.1
    f1 = _measure_freq(render_vco(cv1, sr, secs), sr)
    for _ in range(iters):
        if f1 > 0 and abs(f1 - target) <= tol * target:
            break
        slope = (np.log2(max(f1, 1e-6)) - np.log2(max(f0, 1e-6))) / (cv1 - cv0)
        if not np.isfinite(slope) or slope == 0.0:
            break
        cv2 = cv1 + (np.log2(target) - np.log2(max(f1, 1e-6))) / slope
        cv0, f0 = cv1, f1
        cv1, f1 = cv2, _measure_freq(render_vco(cv2, sr, secs), sr)
    return cv1


# --- VCA envelope (matches dsp/env.rs defaults: linear AD, atk 3ms, dec 200ms) ---
def _ad_envelope(n: int, sr: float, atk_ms: float = 3.0, dec_ms: float = 200.0) -> np.ndarray:
    atk = max(int(atk_ms * 1e-3 * sr), 1)
    dec = max(int(dec_ms * 1e-3 * sr), 1)
    env = np.zeros(n)
    rise = np.linspace(0.0, 1.0, atk, endpoint=False)
    env[:min(atk, n)] = rise[:min(atk, n)]
    if atk < n:
        fall = np.linspace(1.0, 0.0, dec, endpoint=False)
        end = min(atk + dec, n)
        env[atk:end] = fall[:end - atk]
    return env


# --- Stage 1: VCO .tran ------------------------------------------------------
def render_vco(cv: float, sr: float, secs: float, settle: float = 0.03) -> np.ndarray:
    """Run a vco303 .tran at control voltage `cv`; return the ramp output `o_r`
    resampled to `sr` over `secs` (after a `settle` warm-up)."""
    c = Circuit("vco303")
    c.raw_spice += vco_gen.TRANSISTOR_MODELS
    # Ramp the rails 0->V over 1ms (PWL) so the relaxation core self-starts.
    c.raw_spice += "Vvp vp 0 PWL(0 0 1m 12)\n"
    c.raw_spice += "Vvh vh 0 PWL(0 0 1m 5.333)\n"
    c.raw_spice += f"Vcvb cvb 0 PWL(0 0 1m {-4.7062 + cv})\n"
    c.raw_spice += "Vcv cv 0 0\n"
    vco_gen.add_elements(c)

    sim = c.simulator(temperature=25, nominal_temperature=25)
    sim.options(method="gear", rshunt=1e12, gmin=1e-10,
                reltol=1e-3, abstol=1e-9, vntol=1e-6, itl4=1000, trtol=7)
    step = 5e-6  # 200 kHz internal grid — capture the ramp edges before decimating
    an = sim.transient(step_time=step, end_time=settle + secs)

    t = np.array(an.time)
    o_r = np.array(an["o_r"])
    grid = settle + np.arange(int(secs * sr)) / sr
    y = np.interp(grid, t, o_r)
    return y - y.mean()  # center the saw (drop the DC operating point)


# --- Stage 2: VCF .tran (subprocess ngspice, inline PWL drive) ---------------
def _pwl(name: str, npos: str, nneg: str, t: np.ndarray, v: np.ndarray) -> str:
    pts = " ".join(f"{ti:.8e} {vi:.6e}" for ti, vi in zip(t, v))
    return f"{name} {npos} {nneg} PWL({pts})\n"


def render_vcf(drive: np.ndarray, sr: float, cutoff_itail: float, rreso: float) -> np.ndarray:
    """Drive the vcf303 input `sig` with `drive` (a centered saw) and return V(out)
    over the same span, resampled to `sr`. `cutoff_itail` = the tail bias sink (cutoff),
    `rreso` = the resonance feedback leg (Ω)."""
    n = drive.size
    secs = n / sr
    t = np.arange(n) / sr

    c = Circuit("vcf303")
    c.raw_spice += vcf_gen.TRANSISTOR_MODELS
    vcf_gen.add_elements(c, rreso=rreso)
    c.raw_spice += f"Vrail vp8 0 {V_RAIL}\n"
    c.raw_spice += f"Va3 a3 0 {V_A3}\n"
    c.raw_spice += f"Vm2 m2 0 {V_M2}\n"
    c.raw_spice += f"Vbref bref 0 {V_BIAS}\n"
    c.raw_spice += f"Itail tail 0 {cutoff_itail}\n"
    # Signal = DC bias + the VCO saw into `sig` (-> R27 -> in_l).
    c.raw_spice += _pwl("Vsig", "sig", "0", t, V_BIAS + drive)

    dt = 1.0 / (sr * 2.0)
    with tempfile.TemporaryDirectory() as d:
        out = Path(d) / "vcf.dat"
        cir = Path(d) / "vcf.cir"
        cir.write_text(
            str(c)
            + "\n.options gmin=1e-13 reltol=1e-4 abstol=1e-12 vntol=1e-7 method=gear\n"
            + ".control\n"
            + f"tran {dt:.8e} {secs:.8e}\n"
            + f"wrdata {out} v(out)\n"
            + ".endc\n.end\n"
        )
        subprocess.run(["ngspice", "-b", str(cir)], capture_output=True, timeout=300)
        data = np.loadtxt(out)
    tt, vv = data[:, 0], data[:, 1]
    y = np.interp(t, tt, vv)
    return y - y.mean()


# --- Orchestration -----------------------------------------------------------
def voice_reference(note: float, cutoff_itail: float, rreso: float,
                    sr: float = 48_000.0, secs: float = 0.2,
                    cv: float | None = None) -> np.ndarray:
    """Whole-voice SPICE reference for one operating point. Returns float64 @ sr.
    Pass a pre-solved `cv` (from `solve_cv`) to skip the per-note pitch solve."""
    if cv is None:
        cv = solve_cv(note_to_freq(note), sr)
    saw = render_vco(cv, sr, secs)
    filtered = render_vcf(saw, sr, cutoff_itail, rreso)
    env = _ad_envelope(filtered.size, sr)
    return filtered * env


if __name__ == "__main__":
    y = voice_reference(note=57, cutoff_itail=32e-6, rreso=47e3, secs=0.2)
    f = np.fft.rfftfreq(y.size, 1 / 48_000.0)
    mag = np.abs(np.fft.rfft(y))
    print(f"peak {f[np.argmax(mag[1:]) + 1]:.1f} Hz  max|y| {np.max(np.abs(y)):.4f}")
