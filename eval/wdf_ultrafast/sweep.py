"""
Measure-first lever sweep for the WDF **ultra-fast** (lossy) tier.

The ultra-fast tier spends part of the WDF model's intrinsic-floor headroom
(~-160 dB ESR fast-vs-realtime  →  ~-30 dB ESR model-vs-SPICE) on speed, via
lossy solver levers. This harness layers each candidate lever on top of the
``fast`` tier and reports, per lever:

  * **added error**   : ESR of the lever's output vs the ``realtime`` output
                        (the budget is <= -40 dB, i.e. >=10 dB below the -30 dB
                        SPICE floor, so SPICE-measured ESR barely moves). NO
                        SPICE needed — both are Rust renders at the same rate.
  * **fidelity floor**: ESR vs the on-the-fly SPICE reference, computed only for
                        ``realtime`` / ``fast`` / ``ultra-fast`` to bound cost.
  * **speed**         : subprocess wall (min of N runs) → speedup vs ``fast``,
                        plus the solver's avgIters / maxIters.

Gate (combined ultra-fast candidate): added-error <= -40 dB AND speedup >= 1.5x
AND SPICE-ESR within ~0.5 dB of realtime.

Usage (under the repo uv env so data_gen/eval resolve):
    uv run python -m eval.wdf_ultrafast.sweep \\
        [--stims pink_m3_s0,sweep_m3] [--gain 0.5] [--bias 0.77] \\
        [--runs 3] [--no-spice] [--out eval/wdf_ultrafast/out]
"""
from __future__ import annotations

import argparse
import json
import time
from pathlib import Path

import numpy as np

from data_gen.netlist import gain_norm_to_rkj
from eval.wdf_rust_vs_cpp.run_rust_wdf import build_if_needed, run_rust_wdf
from eval.wdf_vs_spice.compare import BIAS, DRY_DIR, metrics, run_spice

# Each lever is layered on the `fast` tier (except the two baselines and the
# assembled ultra-fast preset). (label, tier, extra_args).
LEVERS: list[tuple[str, str, list[str]]] = [
    ("realtime", "realtime", []),
    ("fast", "fast", []),
    ("cheap_exp", "fast", ["--cheap-exp"]),
    ("tol_1e-4", "fast", ["--tol", "1e-4"]),
    ("tol_1e-3", "fast", ["--tol", "1e-3"]),
    ("maxiters_8", "fast", ["--max-iters", "8"]),
    ("cap_depletion", "fast", ["--cap-tier", "depletion"]),
    ("no_ls_trim", "fast", ["--no-ls-trim"]),
    ("os_1", "fast", ["--os", "1"]),
    ("os_1_nodec", "fast", ["--os", "1", "--no-decimate"]),
    ("ultra_fast", "ultra-fast", []),  # == fast + os1 + no-decimate
    # Sub-rate Newton spike (GO/NO-GO): full solve every K-th sample, frozen-Jacobian linear
    # update on the rest. subrate_2_hold is the naive-hold baseline (what the linear update buys).
    ("subrate_2", "fast", ["--subrate", "2"]),
    ("subrate_3", "fast", ["--subrate", "3"]),
    ("subrate_4", "fast", ["--subrate", "4"]),
    ("subrate_2_hold", "fast", ["--subrate", "2", "--subrate-hold"]),
]
# Levers that get a SPICE fidelity check (kept small — SPICE is slow / RAM-bound).
# NOTE: for the OS levers the `added_esr vs realtime` column is MISLEADING (it counts the
# realtime model's own os=2 decimation phase, which is uncorrelated with SPICE error). The
# authoritative fidelity check for ultra-fast is ESR vs SPICE — use `measure.py` (cached ref).
SPICE_LABELS = {"realtime", "fast", "os_1", "os_1_nodec", "ultra_fast"}


def render(dry: Path, rgain: float, bias: float, tier: str, extra: list[str],
           runs: int) -> tuple[np.ndarray, dict, float]:
    """Render once for the waveform/stats, then time `runs` renders → min wall."""
    samples, _sr, stats = run_rust_wdf(
        dry, rgain=rgain, bias=bias, tier=tier, extra_args=extra,
        return_stats=True, allow_build=False,
    )
    best = float("inf")
    for _ in range(max(1, runs)):
        t0 = time.perf_counter()
        run_rust_wdf(dry, rgain=rgain, bias=bias, tier=tier, extra_args=extra,
                     allow_build=False)
        best = min(best, time.perf_counter() - t0)
    return samples.astype(np.float64), stats, best


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("--stims", default="pink_m3_s0,sweep_m3",
                    help="comma-separated dry-stimulus stems (without _dry.wav).")
    ap.add_argument("--gain", type=float, default=0.5, help="gain_norm in [0,1].")
    ap.add_argument("--bias", type=float, default=BIAS)
    ap.add_argument("--runs", type=int, default=3, help="timed renders per lever (min taken).")
    ap.add_argument("--no-spice", action="store_true", help="skip the SPICE fidelity check.")
    ap.add_argument("--out", type=Path, default=Path("eval/wdf_ultrafast/out"))
    ap.add_argument("--no-build", action="store_true")
    args = ap.parse_args()

    args.out.mkdir(parents=True, exist_ok=True)
    build_if_needed(allow_build=not args.no_build)

    stims = [s.strip() for s in args.stims.split(",") if s.strip()]
    rgain = gain_norm_to_rkj(args.gain)

    print("=" * 78)
    print("WDF ultra-fast lever sweep (measure-first)")
    print(f"  stimuli : {stims}")
    print(f"  gain    : gain_norm={args.gain} (R_KJ={rgain:.0f}Ω), bias={args.bias}")
    print(f"  budget  : added-error <= -40 dB vs realtime; speedup >= 1.5x vs fast")
    print(f"  SPICE   : {'OFF' if args.no_spice else 'on for ' + ','.join(sorted(SPICE_LABELS))}")
    print("=" * 78)

    rows: list[dict] = []
    per_stim: dict[str, dict] = {}

    for stem in stims:
        dry = DRY_DIR / f"{stem}_dry.wav"
        if not dry.exists():
            print(f"[skip] missing {dry}")
            continue
        print(f"\n--- {stem} ---")
        spice_ref = None
        if not args.no_spice:
            try:
                import soundfile as sf
                nfix = sf.read(dry, always_2d=False)[0].shape[0]
                spice_ref = run_spice(dry, rgain, args.bias, nfix)
            except Exception as exc:  # noqa: BLE001
                print(f"    [spice] FAILED ({exc}); fidelity check skipped for this stim.")

        outputs: dict[str, np.ndarray] = {}
        stim_rows: list[dict] = []
        for label, tier, extra in LEVERS:
            out, stats, wall = render(dry, rgain, args.bias, tier, extra, args.runs)
            outputs[label] = out
            stim_rows.append({"label": label, "tier": tier, "extra": extra,
                              "wall_s": wall, "avg_iters": stats.get("avg_iters"),
                              "max_iters": stats.get("max_iters")})

        fast_wall = next(r["wall_s"] for r in stim_rows if r["label"] == "fast")
        rt_out = outputs["realtime"]
        for r in stim_rows:
            lbl = r["label"]
            r["speedup_vs_fast"] = round(fast_wall / r["wall_s"], 3) if r["wall_s"] else None
            # added error vs realtime output (SPICE-free budget check)
            if lbl == "realtime":
                r["added_esr_db"] = None
            else:
                r["added_esr_db"] = round(metrics(rt_out, outputs[lbl])["esr_db"], 2)
            # fidelity vs SPICE for the selected labels
            if spice_ref is not None and lbl in SPICE_LABELS:
                r["spice_esr_db"] = round(metrics(spice_ref, outputs[lbl])["esr_db"], 2)
            else:
                r["spice_esr_db"] = None

        # report
        print(f"  {'lever':<14}{'added_esr':>11}{'spice_esr':>11}"
              f"{'avgIt':>7}{'maxIt':>6}{'wall_s':>9}{'x_vs_fast':>10}")
        for r in stim_rows:
            ae = f"{r['added_esr_db']:+.1f}" if r["added_esr_db"] is not None else "  base"
            se = f"{r['spice_esr_db']:+.1f}" if r["spice_esr_db"] is not None else "    -"
            ai = f"{r['avg_iters']:.2f}" if r["avg_iters"] is not None else "  -"
            mi = f"{r['max_iters']}" if r["max_iters"] is not None else "-"
            sp = f"{r['speedup_vs_fast']:.2f}" if r["speedup_vs_fast"] is not None else "-"
            print(f"  {r['label']:<14}{ae:>11}{se:>11}{ai:>7}{mi:>6}"
                  f"{r['wall_s']:>9.3f}{sp:>10}")
        per_stim[stem] = {"rkj_ohm": round(rgain, 1), "rows": stim_rows}
        rows.extend({**r, "stim": stem} for r in stim_rows)

    # aggregate gate on the assembled ultra_fast preset (worst across stimuli)
    uf = [r for r in rows if r["label"] == "ultra_fast"]
    uf_added = [r["added_esr_db"] for r in uf if r["added_esr_db"] is not None]
    uf_speed = [r["speedup_vs_fast"] for r in uf if r["speedup_vs_fast"] is not None]
    uf_spice = [r["spice_esr_db"] for r in uf if r["spice_esr_db"] is not None]
    rt_spice = [r["spice_esr_db"] for r in rows
                if r["label"] == "realtime" and r["spice_esr_db"] is not None]
    gate = {
        "ultra_fast_added_esr_db_worst": max(uf_added) if uf_added else None,
        "ultra_fast_speedup_vs_fast_worst": min(uf_speed) if uf_speed else None,
        "ultra_fast_spice_esr_db_worst": max(uf_spice) if uf_spice else None,
        "realtime_spice_esr_db_worst": max(rt_spice) if rt_spice else None,
        "added_budget_db": -40.0,
        "speedup_target": 1.5,
    }
    gate["added_pass"] = (gate["ultra_fast_added_esr_db_worst"] is not None
                          and gate["ultra_fast_added_esr_db_worst"] <= -40.0)
    gate["speed_pass"] = (gate["ultra_fast_speedup_vs_fast_worst"] is not None
                          and gate["ultra_fast_speedup_vs_fast_worst"] >= 1.5)
    if uf_spice and rt_spice:
        gate["spice_pass"] = (max(uf_spice) - max(rt_spice)) <= 0.5
    else:
        gate["spice_pass"] = None

    summary = {
        "config": {"stims": stims, "gain_norm": args.gain, "rkj_ohm": round(rgain, 1),
                   "bias": args.bias, "runs": args.runs, "spice": not args.no_spice},
        "levers": [{"label": l, "tier": t, "extra": e} for l, t, e in LEVERS],
        "per_stim": per_stim,
        "gate": gate,
    }
    (args.out / "sweep.json").write_text(json.dumps(summary, indent=2))

    print("\n" + "=" * 78)
    print("GATE (assembled ultra_fast preset, worst across stimuli)")
    print(f"  added error : {gate['ultra_fast_added_esr_db_worst']} dB "
          f"(<= -40)  -> {'PASS' if gate['added_pass'] else 'FAIL'}")
    print(f"  speedup     : {gate['ultra_fast_speedup_vs_fast_worst']}x vs fast "
          f"(>= 1.5)  -> {'PASS' if gate['speed_pass'] else 'FAIL'}")
    if gate["spice_pass"] is not None:
        print(f"  SPICE floor : ultra {gate['ultra_fast_spice_esr_db_worst']} dB vs "
              f"realtime {gate['realtime_spice_esr_db_worst']} dB "
              f"(<= 0.5 apart) -> {'PASS' if gate['spice_pass'] else 'FAIL'}")
    print("=" * 78)
    print(f"Wrote {args.out / 'sweep.json'}")


if __name__ == "__main__":
    main()
