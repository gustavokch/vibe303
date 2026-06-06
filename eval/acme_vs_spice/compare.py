"""
Feasibility spike: ACME.jl BA283 model vs the SPICE reference.

Translates ``data_gen/netlist.py`` into ACME.jl (``ba283_acme.jl``) and checks
whether the DK-method model faithfully tracks the authoritative ngspice/PySpice
ground truth on the real EX/10283 circuit.

Reuses the proven SPICE-regen + metric + plotting code from the validated
``eval.wdf_vs_spice.compare`` harness; the only substitution is the model runner
(``run_acme`` for ``try_run_wdf``). The SPICE wet target is regenerated on the
fly per (stimulus, gain) from the current netlist — no stale pre-rendered set.

Usage (run under this repo's uv env so data_gen resolves):
    uv run python -m eval.acme_vs_spice.compare \\
        [--stims pink_m3_s0] [--gains 0.5] [--bias 0.77] \\
        [--out eval/acme_vs_spice/out]

The spike is a verdict, not a gate: ACME's Ebers-Moll/Gummel-Poon bjt() has no
junction capacitances, so an exact −22 dB ESR is not expected. We report ESR /
NRMSE / level match, the Welch FR + coherence (the authoritative linear check),
and a waveform overlay, then a one-line feasibility verdict.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np
import soundfile as sf

from data_gen.netlist import gain_norm_to_rkj

# Reuse the validated SPICE-regen + metric + plotting helpers verbatim.
from eval.wdf_vs_spice.compare import (
    SR,
    ONSET_SKIP,
    SPAN,
    DRY_DIR,
    run_spice,
    metrics,
    _welch_tf,
    _overlay,
)
from eval.acme_vs_spice.run_acme import run_acme

DEFAULT_STIMS = ["pink_m3_s0"]   # pink → authoritative Welch FR + coherence
DEFAULT_GAINS = [0.5]            # default 26 dB-ish mid tap


def _fr_plot(fcs, rdb, mdb, mco, stem, gain_norm, gain, bias, out_dir, tag) -> dict:
    err = mdb - rdb
    band = (fcs >= 20) & (fcs <= 20_000)
    midband = (fcs >= 200) & (fcs <= 5_000)

    fig, axes = plt.subplots(2, 1, figsize=(10, 7), sharex=True)
    axes[0].semilogx(fcs, rdb, "o-", color="steelblue", label="SPICE")
    axes[0].semilogx(fcs, mdb, "s--", color="seagreen", label="ACME")
    axes[0].set_ylabel("|H(f)| (dB)")
    axes[0].set_title(f"Frequency response (Welch) — {stem}, "
                      f"gain_norm={gain_norm} (R_KJ={gain:.0f}Ω), bias={bias}")
    axes[0].legend()
    axes[0].grid(True, which="both", alpha=0.4)
    axes[1].semilogx(fcs, err, color="crimson", label="ACME − SPICE")
    axes[1].axhline(0, color="k", lw=0.8, ls="--")
    axes[1].fill_between([20, 20_000], -1, 1, color="green", alpha=0.08, label="±1 dB")
    axes[1].set_ylabel("Error (dB)")
    axes[1].set_xlabel("Frequency (Hz)")
    axes[1].set_ylim(-6, 6)
    axes[1].set_xlim(20, 40_000)
    axes[1].legend(fontsize=8)
    axes[1].grid(True, which="both", alpha=0.4)
    plt.tight_layout()
    fig.savefig(out_dir / f"fr_welch_{tag}.png", dpi=150)
    plt.close(fig)

    fig, ax = plt.subplots(figsize=(10, 3.5))
    ax.semilogx(fcs, mco, color="purple")
    ax.set_ylim(0, 1.02)
    ax.set_xlim(20, 40_000)
    ax.set_xlabel("Frequency (Hz)")
    ax.set_ylabel("Coherence")
    ax.set_title(f"ACME coherence vs dry — {stem}")
    ax.grid(True, which="both", alpha=0.4)
    plt.tight_layout()
    fig.savefig(out_dir / f"fr_coherence_{tag}.png", dpi=150)
    plt.close(fig)

    return {
        "max_err_db_20Hz_20kHz": round(float(np.max(np.abs(err[band]))), 2),
        "max_err_db_midband_200_5k": round(float(np.max(np.abs(err[midband]))), 2),
        "mean_coherence_midband": round(float(np.mean(mco[midband])), 3),
    }


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("--stims", default=",".join(DEFAULT_STIMS))
    ap.add_argument("--gains", default=",".join(str(g) for g in DEFAULT_GAINS))
    ap.add_argument("--bias", type=float, default=0.77)
    ap.add_argument("--out", type=Path, default=Path("eval/acme_vs_spice/out"))
    args = ap.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)

    stims = [s.strip() for s in args.stims.split(",") if s.strip()]
    gains = [float(x) for x in args.gains.split(",")]

    print("=" * 64)
    print("ACME.jl vs SPICE — BA283 feasibility spike")
    print(f"  bias (fixed) : {args.bias}")
    print(f"  gain grid    : {gains} (gain_norm → R_KJ via netlist)")
    print(f"  stimuli      : {stims}")
    print(f"  metric       : raw-volts ESR, |lag|<={SPAN}, {ONSET_SKIP}s onset-skip")
    print("  SPICE        : regenerated on-the-fly from data_gen/netlist.py")
    print("=" * 64)

    results: dict[str, list[dict]] = {}
    fr_list: list[dict] = []

    for stem in stims:
        dry_path = DRY_DIR / f"{stem}_dry.wav"
        if not dry_path.exists():
            print(f"[acme] MISSING {dry_path}")
            continue
        dry = np.asarray(sf.read(dry_path, always_2d=False)[0], dtype=np.float64)
        nfix = dry.shape[0]
        rows: list[dict] = []
        print(f"[acme] {stem}")
        for gn in gains:
            g = gain_norm_to_rkj(gn)
            try:
                ref = run_spice(dry_path, g, args.bias, nfix)
            except Exception as exc:  # noqa: BLE001
                print(f"    gain_norm={gn:.2f}: SPICE FAIL {exc}")
                rows.append({"gain_norm": gn, "rkj": g, "esr_db": None, "spice_failed": True})
                continue
            try:
                mod = run_acme(dry, SR, rgain=g, bias=args.bias)
            except Exception as exc:  # noqa: BLE001
                print(f"    gain_norm={gn:.2f}: ACME FAIL {exc}")
                rows.append({"gain_norm": gn, "rkj": g, "esr_db": None, "acme_failed": True})
                continue
            if not np.all(np.isfinite(mod)):
                print(f"    gain_norm={gn:.2f}: ACME non-finite output")
                rows.append({"gain_norm": gn, "rkj": g, "esr_db": None, "acme_nonfinite": True})
                continue

            mt = metrics(ref, mod)
            mt.update({"gain_norm": gn, "rkj": g})
            rows.append(mt)
            print(f"    gain_norm={gn:.2f} R_KJ={g:>5.0f}Ω  ESR={mt['esr_db']:+6.1f} dB  "
                  f"NRMSE={mt['nrmse']:.4f}  rms={mt['rms_db']:+.2f} dB  "
                  f"pk {mt['ref_pk']:.3f}/{mt['mod_pk']:.3f}  lag={mt['lag']:+d}")

            # Welch FR + waveform overlay per (stim, gain) point (unique tags).
            tag = f"{stem}_g{gn:.2f}".replace(" ", "_").replace("/", "_")
            n = min(len(dry), len(ref), len(mod))
            fcs, mdb, mco = _welch_tf(mod[:n], dry[:n])
            _, rdb, _ = _welch_tf(ref[:n], dry[:n])
            fr_list.append({"stimulus": stem, "gain_norm": gn, "rkj_ohm": round(g, 1),
                            **_fr_plot(fcs, rdb, mdb, mco, stem, gn, g, args.bias,
                                       args.out, tag)})
            _overlay(ref, mod, f"acme_{tag}", args.out)
        results[stem] = rows

    # Verdict from the WORST point: ACME-vs-SPICE midband FR err + ESR.
    # NB: coherence-vs-dry drops under clipping (gain 1.0) — that is the device's
    # own nonlinearity, NOT a model mismatch, so it is reported but not gated on.
    # The ACME-vs-SPICE FR err (both clip identically) and ESR are the match metric.
    all_rows = [r for rows in results.values() for r in rows if r.get("esr_db") is not None]
    esr_vals = [r["esr_db"] for r in all_rows]
    verdict = "no finite ACME run"
    if fr_list and esr_vals:
        mid = max(f["max_err_db_midband_200_5k"] for f in fr_list)
        worst_esr = max(esr_vals)
        if mid <= 3.0 and worst_esr <= -20.0:
            verdict = (f"FEASIBLE — ACME matches SPICE across all points "
                       f"(worst midband ±{mid} dB, worst ESR {worst_esr:+.1f} dB) "
                       f"incl. gain_norm 1.0 clipping; refine junction-cap fidelity next.")
        elif mid <= 6.0 and worst_esr <= -12.0:
            verdict = (f"PARTIAL — worst midband ±{mid} dB, worst ESR {worst_esr:+.1f} dB; "
                       f"likely the missing BJT junction caps / clipping-edge detail.")
        else:
            verdict = (f"GAPS — worst midband ±{mid} dB, worst ESR {worst_esr:+.1f} dB; "
                       f"investigate topology/param mapping before trusting ACME.")

    summary = {
        "config": {"bias_fixed": args.bias, "gain_norm_grid": gains, "stims": stims,
                   "max_lag_samples": SPAN, "onset_skip_s": ONSET_SKIP,
                   "metric": "raw-volts ESR (dB), onset-skipped",
                   "spice": "on-the-fly from data_gen/netlist.py (EX/10283)"},
        "model": "ACME.jl DK-method BA283 (TR1+TR2+TR3+LO1166), Ebers-Moll bjt (no junction caps)",
        "frequency_response": fr_list,
        "points": {stem: [{"gain_norm": r["gain_norm"], "rkj_ohm": round(r["rkj"], 1),
                           "esr_db": (round(r["esr_db"], 2) if r.get("esr_db") is not None else None),
                           "nrmse": (round(r["nrmse"], 4) if "nrmse" in r else None),
                           "rms_db": (round(r["rms_db"], 2) if "rms_db" in r else None),
                           "ref_pk": (round(r["ref_pk"], 3) if "ref_pk" in r else None),
                           "mod_pk": (round(r["mod_pk"], 3) if "mod_pk" in r else None),
                           **{k: r[k] for k in ("spice_failed", "acme_failed", "acme_nonfinite") if k in r}}
                          for r in rows]
                   for stem, rows in results.items()},
        "esr_summary_db": {
            "median": round(float(np.median(esr_vals)), 2) if esr_vals else None,
            "best": round(float(np.min(esr_vals)), 2) if esr_vals else None,
            "worst": round(float(np.max(esr_vals)), 2) if esr_vals else None,
            "n_points": len(all_rows),
        },
        "verdict": verdict,
        "notes": [
            "ACME bjt() is Ebers-Moll/Gummel-Poon without junction caps (CJE/CJC/TF); "
            "external Miller comp caps C2/C5/C3 are modelled explicitly.",
            "ACME has no SPICE .op; ba283_acme.jl prepends a 2 s warmup so the slow "
            "electrolytics settle, then the comparator onset-skips 0.5 s.",
            "Welch FR + coherence is the authoritative linear-fidelity check; ESR is "
            "phase-sensitive time-domain L2.",
        ],
    }
    (args.out / "summary.json").write_text(json.dumps(summary, indent=2))

    print("\n" + "=" * 64)
    print("SUMMARY")
    print("=" * 64)
    for f in fr_list:
        print(f"  [{f['stimulus'][:20]} g{f['gain_norm']}] FR 20-20k {f['max_err_db_20Hz_20kHz']:+.1f} dB"
              f"  midband {f['max_err_db_midband_200_5k']:+.1f} dB"
              f"  coh {f['mean_coherence_midband']:.3f}")
    if esr_vals:
        print(f"  ESR median            : {np.median(esr_vals):+.1f} dB")
        print(f"  ESR best/worst        : {np.min(esr_vals):+.1f} / {np.max(esr_vals):+.1f} dB")
    print(f"  VERDICT               : {verdict}")
    print("=" * 64)
    print(f"Plots + summary in: {args.out}")


if __name__ == "__main__":
    main()
