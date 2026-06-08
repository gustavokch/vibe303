"""
Fast iteration helper for the ultra-fast tier: cache the SPICE reference once
(ngspice is slow + flaky), then score arbitrary Rust render configs against it.

    uv run python -m eval.wdf_ultrafast.measure \\
        --stim pink_m3_s0 --gain 0.5 \\
        --config "realtime:--tier realtime" \\
        --config "os1:--tier fast --os 1" \\
        --config "os1_nodec:--tier fast --os 1 --no-decimate"

The SPICE ref is cached at out/spice_<stim>_g<gain>.npy; delete it to regen.
"""
from __future__ import annotations

import argparse
from pathlib import Path

import numpy as np
import soundfile as sf

from data_gen.netlist import gain_norm_to_rkj
from eval.wdf_rust_vs_cpp.run_rust_wdf import run_rust_wdf
from eval.wdf_vs_spice.compare import _welch_tf, metrics, run_spice


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--stim", default="pink_m3_s0")
    ap.add_argument("--gain", type=float, default=0.5)
    ap.add_argument("--bias", type=float, default=0.77)
    ap.add_argument("--config", action="append", default=[],
                    help="repeatable 'label:--flag val ...' Rust render spec.")
    ap.add_argument("--out", type=Path, default=Path("eval/wdf_ultrafast/out"))
    args = ap.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)

    dry_p = Path.home() / f"GoogleDrive/ba283_spice/dry/{args.stim}_dry.wav"
    dry = np.asarray(sf.read(dry_p, always_2d=False)[0], dtype=np.float64)
    g = gain_norm_to_rkj(args.gain)

    cache = args.out / f"spice_{args.stim}_g{args.gain}.npy"
    if cache.exists():
        ref = np.load(cache)
        print(f"[spice] loaded cached ref {cache.name} ({len(ref)} samp)")
    else:
        print("[spice] generating (ngspice)...", flush=True)
        ref = run_spice(dry_p, g, args.bias, dry.shape[0])
        np.save(cache, ref)
        print(f"[spice] cached -> {cache.name}")

    configs = args.config or ["realtime:--tier realtime", "fast:--tier fast",
                              "os1:--tier fast --os 1"]
    n = min(len(ref), len(dry))
    _, rdb, _ = _welch_tf(ref[:n], dry[:n])  # SPICE FR (for the >10k error column)
    print(f"\n{'config':<18}{'ESR_vs_SPICE':>13}{'rms':>8}{'coh200_5k':>11}"
          f"{'FRerr>10k':>11}")
    for spec in configs:
        label, _, flagstr = spec.partition(":")
        extra = flagstr.split()
        s, _ = run_rust_wdf(dry_p, rgain=g, bias=args.bias, tier="realtime",
                            extra_args=extra, allow_build=False)
        # run_rust_wdf injects --tier realtime first; a --tier in `extra` overrides it (last wins).
        out = s.astype(np.float64)
        m = metrics(ref, out)
        nn = min(len(ref), len(out), len(dry))
        fcs, odb, coh = _welch_tf(out[:nn], dry[:nn])
        mid = (fcs >= 200) & (fcs <= 5000)
        hi = (fcs >= 10000)
        fr_hi = float(np.max(np.abs((odb - rdb)[hi]))) if hi.any() else float("nan")
        print(f"{label:<18}{m['esr_db']:>+12.1f}{m['rms_db']:>+8.2f}"
              f"{float(np.mean(coh[mid])):>11.3f}{fr_hi:>11.2f}")


if __name__ == "__main__":
    main()
