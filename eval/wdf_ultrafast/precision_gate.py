"""Precision-dial ESR gate for the device-model precision axis (post 3-axis refactor).

Precision (High/Medium/Low) is now device-model-only (cap_tier + cheap_exp). This gates the
ADDED error each precision step costs:

  * added_vs_tierHigh : ESR of <tier>+<prec> vs the SAME tier at High — the precision-specific
                        cost, isolated from the tier's own loss. The meaningful gate on every tier.
  * added_vs_rtHigh   : ESR vs Realtime+High — the plan's literal phrasing; only well-posed on the
                        os=2 tiers (realtime/fast), since os=1 tiers (ultra-fast/warp) differ from
                        Realtime+High mostly by their os=1 rate, not by precision.

No SPICE needed — all Rust renders at the same rate, lag-aligned by `metrics`.

Usage:
    uv run python -m eval.wdf_ultrafast.precision_gate [--stims sweep_m3,pink_m3_s0] [--gain 0.5]
"""
from __future__ import annotations

import argparse
from pathlib import Path

from data_gen.netlist import gain_norm_to_rkj
from eval.wdf_rust_vs_cpp.run_rust_wdf import build_if_needed, run_rust_wdf
from eval.wdf_vs_spice.compare import BIAS, DRY_DIR, metrics

TIERS = ["realtime", "fast", "ultra-fast", "warp"]
PRECS = ["high", "medium", "low"]
GATE_DB = -50.0  # added-error budget vs same-tier High


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--stims", default="sweep_m3,pink_m3_s0")
    ap.add_argument("--gain", type=float, default=0.5)
    ap.add_argument("--bias", type=float, default=BIAS)
    args = ap.parse_args()

    build_if_needed()
    rgain = gain_norm_to_rkj(args.gain)
    stims = [s.strip() for s in args.stims.split(",") if s.strip()]

    worst_gate = -999.0
    for stim in stims:
        dry = DRY_DIR / f"{stim}_dry.wav"
        if not dry.exists():
            print(f"[skip] {dry} missing")
            continue
        # render every (tier, prec); cache outputs
        out: dict[tuple[str, str], object] = {}
        for tier in TIERS:
            for prec in PRECS:
                samp, _ = run_rust_wdf(
                    dry, rgain=rgain, bias=args.bias, tier=tier,
                    extra_args=["--precision", prec],
                )
                out[(tier, prec)] = samp
        rt_high = out[("realtime", "high")]

        print(f"\n=== {stim}  (R_KJ={rgain:.0f}Ω, bias={args.bias}) ===")
        print(f"  {'tier':<11}{'prec':<8}{'added_vs_tierHigh':>18}{'added_vs_rtHigh':>16}")
        for tier in TIERS:
            tier_high = out[(tier, "high")]
            for prec in PRECS:
                mod = out[(tier, prec)]
                a_tier = metrics(tier_high, mod)["esr_db"] if prec != "high" else None
                a_rt = metrics(rt_high, mod)["esr_db"]
                at = f"{a_tier:+.1f}" if a_tier is not None else "   base"
                print(f"  {tier:<11}{prec:<8}{at:>18}{a_rt:>+16.1f}")
                # gate: precision added-error vs the SAME tier's High. This is a pure device-model
                # delta on EVERY tier (same iteration strategy, same os, only cap_tier/cheap_exp
                # differ), so it is well-posed on the os=1 tiers too — a fidelity regression in
                # UltraFast/Warp Med/Low must fail here, not slip through.
                if a_tier is not None:
                    worst_gate = max(worst_gate, a_tier)

    print(f"\nGATE (worst Med/Low added-error vs same-tier High, all tiers): "
          f"{worst_gate:+.1f} dB  budget <= {GATE_DB:.0f} dB  -> "
          f"{'PASS' if worst_gate <= GATE_DB else 'FAIL'}")


if __name__ == "__main__":
    main()
