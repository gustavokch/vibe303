"""os=1 aliasing bound for the ultra-fast tier — SPICE-free.

The vs-SPICE gate (`measure.py`) could not cover the worst aliasing case — full-gain
pink — because ngspice OOMs on it. This probe closes that gap without ngspice: it
compares ultra-fast (os=1, decimate=false) against a high-OS self-reference
(os=8, decimate=true ≈ alias-free truth) on the *same* circuit + solver, so the only
variable is the oversampling factor. That isolates the aliasing term exactly, whereas
vs-SPICE conflates model error + decimation phase.

Read-out:
  * coh200-5k  — in-band coherence. < 1.0 would mean aliasing is destroying linearity.
  * HF>12k_dB  — ultra-fast HF energy relative to the os=8 ref. A *positive* value would
                 mean nonlinear harmonics folded back down into the top octave (alias
                 inflation); ~0 or negative means no spurious alias products.
  * ESR_vs_os8 — broadband RMS delta vs the os=8 ref (OS-axis only; dominated by
                 legitimate HF magnitude/phase resolution, not aliasing — coherence is
                 the discriminator).

    uv run python -m eval.wdf_ultrafast.alias_probe
"""
from __future__ import annotations

import numpy as np
from pathlib import Path
from scipy.signal import coherence, welch

from data_gen.netlist import gain_norm_to_rkj
from eval.wdf_rust_vs_cpp.run_rust_wdf import run_rust_wdf

SR = 44_100
DRY_DIR = Path.home() / "GoogleDrive/ba283_spice/dry"


def main() -> None:
    print(f"{'stim':14s}{'gain':>5}  {'ESR_vs_os8':>10}  {'coh200-5k':>9}  {'HF>12k':>7}")
    for stim in ("pink_m3_s0", "pink_m12_s0"):       # bright, full-band
        for gain in (1.0,):                          # full gain = worst nonlinear-harmonic case
            dry_p = DRY_DIR / f"{stim}_dry.wav"
            if not dry_p.exists():
                print(f"{stim:14s} skip (no dry wav)")
                continue
            g = gain_norm_to_rkj(gain)
            ref, _ = run_rust_wdf(dry_p, rgain=g, bias=0.77, tier="realtime",
                                  extra_args=["--tier", "fast", "--os", "8", "--decimate"],
                                  allow_build=False)
            uf, _ = run_rust_wdf(dry_p, rgain=g, bias=0.77, tier="ultra-fast", allow_build=False)
            n = min(len(ref), len(uf))
            ref, uf = ref[:n].astype(float), uf[:n].astype(float)

            esr = 20 * np.log10(np.linalg.norm(ref - uf) / (np.linalg.norm(ref) + 1e-20))
            f, c = coherence(ref, uf, fs=SR, nperseg=8192)
            mid = (f >= 200) & (f <= 5000)
            fr, pr = welch(ref, fs=SR, nperseg=8192)
            _, pu = welch(uf, fs=SR, nperseg=8192)
            hi = fr >= 12_000
            hf_db = 10 * np.log10((pu[hi].sum() + 1e-30) / (pr[hi].sum() + 1e-30))
            print(f"{stim:14s}{gain:>5}  {esr:>+9.1f}dB  {c[mid].mean():>9.4f}  {hf_db:>+5.2f}dB")


if __name__ == "__main__":
    main()
