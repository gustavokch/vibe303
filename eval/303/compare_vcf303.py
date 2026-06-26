"""ACME-vs-SPICE for the vcf303 diode-ladder VCF (STEP 4) — NO-GO reproducer.

Step 4 attempted the BA283-style ACME-vs-SPICE parity on the VCF. It is a NO-GO
(see vcf303_acme.jl header / VCF_MAP §12): ACME's DK-method converges only to a
NONPHYSICAL operating point for the 303's ideal-current-source-biased ladder (the
bias current escapes the gmin reference leaks -> every diode reverse-biased OFF ->
the small-signal transimpedance collapses to ~1e-3 Ω vs the correct 20 kΩ), so
ngspice stays the VCF ground truth (run_vcf303_spice.py, steps 1-3).

This script REPRODUCES that NO-GO on the bare ladder core: both sides drive the
SAME ideal rig (per-rail DC bias current = cutoff, differential AC current drive,
differential Rtop readout v(tl)-v(tr)). ngspice runs a true small-signal `.ac` and
gives the correct 4-pole LP that tracks the bias; ACME (chirp transient -> H(f) =
FFT(out)/FFT(chirp)) gives the degenerate near-zero response. The script prints the
SPICE ladder (PASS) and flags the ACME side as degenerate -> RESULT: NO-GO.

Usage:
  uv run --no-project --with PySpice --with numpy python eval/303/compare_vcf303.py \
      [--render] [--biases 16,64,256]
  --render  invoke julia (eval/acme_vs_spice project) to (re)render the ACME bins
            first; omit to reuse bins already in the work dir.
"""

from __future__ import annotations

import subprocess
import sys
import tempfile
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import _vcf303_netlist_gen as gen  # noqa: E402  (for the 1N4148 .MODEL card)

WORK = HERE / "_acme_vcf_work"
SR = 192000.0
T_S = 1.0
FBAND = (4.0, 20000.0)    # band for the FR-shape error (ladder dominant pole is low)
PB = (4.0, 12.0)          # passband window for normalisation (flat below cutoff)


# --- SPICE side: the bare diode ladder, ideal bias + differential AC -----------

def _ladder_deck(i_bias: float) -> str:
    """Raw ngspice deck: 8-diode ladder + 4 rail caps, per-rail DC bias current
    (top->bottom), differential AC current at the bottom, 1GΩ common-mode leaks
    (matches vcf303_acme.jl). PLAIN numbers (ngspice-46 mis-parses unit suffixes)."""
    lines = ["* vcf303 bare diode ladder — ideal-source AC rig", gen.TRANSISTOR_MODELS]
    diodes = [("D1L", "tl", "al"), ("D1R", "tr", "ar"), ("D2L", "al", "bl"),
              ("D2R", "ar", "br"), ("D3L", "bl", "cl"), ("D3R", "br", "cr"),
              ("D4L", "cl", "dl"), ("D4R", "cr", "dr")]
    for nm, a, k in diodes:
        lines.append(f"{nm} {a} {k} 1N4148")
    for nm, n1, n2, v in [("C98", "al", "ar", 3.3e-8), ("C99", "bl", "br", 3.3e-8),
                          ("C100", "cl", "cr", 3.3e-8), ("C101", "dl", "dr", 1.8e-8)]:
        lines.append(f"{nm} {n1} {n2} {v}")
    # bias: push i_bias into each top rail, sink it from each bottom rail
    lines.append(f"Itl 0 tl {i_bias}")
    lines.append(f"Itr 0 tr {i_bias}")
    lines.append(f"Idl dl 0 {i_bias}")
    lines.append(f"Idr dr 0 {i_bias}")
    # differential AC drive (1 A) bottom dl->dr
    lines.append("Iac dl dr AC 1")
    # differential readout load at the top (no DC, balanced rails) = the AC load
    lines.append("Rtop tl tr 20000")
    # common-mode reference leaks (SPICE gmin surrogate, matches ACME)
    for n in ["tl", "al", "bl", "cl", "dl", "tr", "ar", "br", "cr", "dr"]:
        lines.append(f"Rg_{n} {n} 0 1e9")
    return "\n".join(lines) + "\n"


def spice_fr(i_bias: float):
    """ngspice .ac of the bare ladder; returns (freq, mag_db) of v(tl)-v(tr)."""
    deck = _ladder_deck(i_bias)
    with tempfile.TemporaryDirectory() as d:
        out = Path(d) / "ac.dat"
        cir = Path(d) / "lad.cir"
        cir.write_text(
            deck
            + ".options gmin=1e-13 reltol=1e-4 abstol=1e-12 vntol=1e-7\n"
            + ".control\nac dec 40 1 3e6\n"
            + f"wrdata {out} vdb(tl,tr)\n.endc\n.end\n"
        )
        subprocess.run(["ngspice", "-b", str(cir)], capture_output=True, timeout=120)
        data = np.loadtxt(out)
    return data[:, 0], data[:, 1]


# --- ACME side: recover H(f) from the chirp render -----------------------------

def render_acme(biases: list[int]) -> None:
    WORK.mkdir(exist_ok=True)
    cmd = ["julia", f"--project={HERE.parent}/acme_vs_spice", str(HERE / "vcf303_acme.jl"),
           str(WORK), str(int(SR)), str(T_S), *[str(b) for b in biases]]
    print("rendering ACME:", " ".join(cmd))
    subprocess.run(cmd, check=True)


def _meta() -> dict:
    m = {}
    for line in (WORK / "meta.txt").read_text().splitlines():
        k, _, v = line.partition("=")
        m[k] = v
    return m


def acme_out_ptp(i_uA: int) -> float:
    """Peak-to-peak of the ACME differential output over the chirp window. Expected
    ~ IAC*Rtop ≈ 2 mV in the passband for a working ladder; the degenerate NO-GO
    branch (diodes OFF) collapses it to numerical noise (~1e-10 V)."""
    m = _meta()
    nset = int(m["nset"])
    out = np.fromfile(WORK / f"out_{i_uA}.bin", dtype="<f8")
    win = out[nset:]
    return float(win.max() - win.min())


# --- metrics -------------------------------------------------------------------

def _on_grid(f, mag_db, grid):
    good = np.isfinite(mag_db)
    return np.interp(np.log10(grid), np.log10(f[good]), mag_db[good],
                     left=np.nan, right=np.nan)


def _passband(grid, mag_db):
    sel = (grid >= PB[0]) & (grid <= PB[1]) & np.isfinite(mag_db)
    return float(np.nanmean(mag_db[sel])) if sel.any() else float("nan")


def main() -> int:
    biases = [16, 64, 256]
    for a in sys.argv[1:]:
        if a.startswith("--biases"):
            v = a.split("=", 1)[1] if "=" in a else sys.argv[sys.argv.index(a) + 1]
            biases = [int(x) for x in v.split(",")]
    if "--render" in sys.argv:
        render_acme(biases)

    grid = np.logspace(np.log10(FBAND[0]), np.log10(FBAND[1]), 240)
    print(f"\nACME-vs-SPICE diode ladder (NO-GO reproducer)   band {FBAND[0]:.0f}-{FBAND[1]:.0f} Hz")
    # SPICE side: a real bias-dependent 4-pole — passband ~86 dB (=20log10 Rtop) and
    # |H| at 1 kHz climbs strongly as the bias opens the filter. ACME side: the
    # differential output collapses to numerical noise (the degenerate OFF branch).
    print(f"{'I_uA':>5} {'pb_spice':>9} {'|H|@1kHz':>9} | {'acme_ptp':>11} {'expect':>9}")
    h1k, acme_ptps, pbs_all = [], [], []
    expect_ptp = 1e-7 * 20000.0               # IAC * Rtop ≈ passband transimpedance
    for iua in biases:
        fs, ms = spice_fr(iua * 1e-6)
        gs = _on_grid(fs, ms, grid)
        pbs = _passband(grid, gs)
        pbs_all.append(pbs)
        m1k = float(np.interp(np.log10(1000.0), np.log10(grid), gs))
        h1k.append(m1k)
        ptp = acme_out_ptp(iua)
        acme_ptps.append(ptp)
        print(f"{iua:5d} {pbs:8.1f}d {m1k:8.1f}d | {ptp:10.2e}V {expect_ptp:8.1e}V")

    # SPICE is a real bias-dependent 4-pole iff the passband sits at ~86 dB (the
    # Rtop transimpedance, i.e. diodes conducting) AND |H|@1kHz climbs monotonically
    # as the bias opens the filter.
    spice_real = (all(80.0 < p < 90.0 for p in pbs_all)
                  and all(h1k[i] < h1k[i + 1] for i in range(len(h1k) - 1)))
    acme_degenerate = all(p < 1e-5 for p in acme_ptps)        # output collapsed <<2mV
    print(f"\nSPICE ladder: 4-pole LP, passband ~86 dB; |H|@1kHz rises "
          f"{h1k[0]:.0f}->{h1k[-1]:.0f} dB as bias opens the filter (real & bias-dependent={spice_real})")
    print(f"ACME ladder : output ~{np.median(acme_ptps):.1e} V vs expected ~{expect_ptp:.1e} V "
          f"— collapsed to noise (nonphysical OFF branch — see vcf303_acme.jl header)")
    print("RESULT:", "NO-GO confirmed — SPICE ladder correct, ACME degenerate "
          "(ACME VCF parity NOT achievable; ngspice is ground truth)"
          if (spice_real and acme_degenerate)
          else "UNEXPECTED — ACME no longer degenerate OR SPICE not bias-dependent (re-check)")
    return 0 if (spice_real and acme_degenerate) else 1


if __name__ == "__main__":
    sys.exit(main())
