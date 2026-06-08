"""
Rust WDF port vs the C++ original — full-stream ESR / FR parity, per tier.

The in-tree golden vectors null Rust *realtime* against C++ on a 256-sample
fixture; the lever A/B tests bound deviation *within Rust*. This harness closes
the gap: it renders the SAME dry stimuli through BOTH binaries at MATCHED tiers
(realtime, fast) on real-length program material and reports Rust-vs-C++ ESR,
NRMSE, level match, and the Welch frequency-response error.

It is Rust-vs-C++ only (no SPICE) — the C++ binary is already SPICE-validated by
``eval.wdf_vs_spice``; here we measure how faithfully the port reproduces it.

Both binaries run at os=2 (C++ ``--quality realtime|fast`` presets and the Rust
``OS`` const), unity gain, 100 % wet, so their outputs share the volts-at-SEC_IN /
DC-removed domain. Metrics + Welch FR are reused verbatim from the validated
``eval.wdf_vs_spice.compare`` harness.

Usage (under this repo's uv env):
    uv run python -m eval.wdf_rust_vs_cpp.compare \\
        --stims sweep_m6,pink_m6 --gains 0.0,0.5,1.0 \\
        --tiers realtime,fast --bias 0.77 \\
        --dry-dir data/dry --out eval/wdf_rust_vs_cpp/out
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

# Reuse the validated metric + Welch helpers verbatim.
from eval.wdf_vs_spice.compare import SR, metrics, _welch_tf
from eval.wdf_vs_spice.run_wdf import run_wdf
from eval.wdf_rust_vs_cpp.run_rust_wdf import run_rust_wdf, build_if_needed as build_rust

REPO = Path(__file__).resolve().parents[2]

# Pass thresholds (dB). realtime must hold near golden parity (cross-compiler f64 drift only);
# fast is looser because both run the lossy freeze-caps lever and g++/rustc fast-math diverge.
# ultra-fast = fast + os1 + no-decimate (same lossy levers) → same parity budget as fast.
GATE_ESR_DB = {"realtime": -60.0, "fast": -40.0, "ultra-fast": -40.0}
FR_MAX_HZ = 15_000.0   # FR-error gate band ceiling
GATE_FR_DB = 0.1       # max |Rust-C++| FR error to FR_MAX_HZ


def resolve_dry(dry_dir: Path, stim: str) -> Path:
    """Find a dry WAV for *stim* under *dry_dir*, tolerating bare/'.wav'/'_dry.wav' spellings."""
    for cand in (stim, f"{stim}.wav", f"{stim}_dry.wav"):
        p = dry_dir / cand
        if p.exists():
            return p
    raise FileNotFoundError(f"no dry WAV for stim {stim!r} in {dry_dir} (tried .wav / _dry.wav)")


def fr_error(rust: np.ndarray, cpp: np.ndarray, dry: np.ndarray) -> tuple[float, np.ndarray, np.ndarray, np.ndarray]:
    """Welch FR of both outputs vs the dry input; return (max|err| dB to FR_MAX_HZ, fcs, rust_db, cpp_db)."""
    fcs, rust_db, _ = _welch_tf(rust, dry)
    _, cpp_db, _ = _welch_tf(cpp, dry)
    band = fcs <= FR_MAX_HZ
    err = float(np.max(np.abs(rust_db[band] - cpp_db[band]))) if band.any() else float("nan")
    return err, fcs, rust_db, cpp_db


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--stims", default="sweep_m6,pink_m6")
    ap.add_argument("--gains", default="0.0,0.5,1.0", help="gain_norm grid in [0,1]")
    ap.add_argument("--tiers", default="realtime,fast")
    ap.add_argument("--bias", type=float, default=0.77)
    ap.add_argument("--dry-dir", default="data/dry")
    ap.add_argument("--out", default="eval/wdf_rust_vs_cpp/out")
    ap.add_argument("--no-build", action="store_true", help="skip the Rust cargo build step")
    args = ap.parse_args()

    stims = [s.strip() for s in args.stims.split(",") if s.strip()]
    gains = [float(g) for g in args.gains.split(",") if g.strip()]
    tiers = [t.strip() for t in args.tiers.split(",") if t.strip()]
    dry_dir = (REPO / args.dry_dir) if not Path(args.dry_dir).is_absolute() else Path(args.dry_dir)
    out_dir = (REPO / args.out) if not Path(args.out).is_absolute() else Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)

    if not args.no_build:
        build_rust()  # one cargo build up front; per-call builds are then no-ops

    rows: list[dict] = []
    # For the FR plot we keep one representative cell per tier (mid gain, first stim).
    fr_repr: dict[str, dict] = {}

    for stim in stims:
        dry_path = resolve_dry(dry_dir, stim)
        dry, dsr = sf.read(dry_path, dtype="float32")
        if dry.ndim > 1:
            dry = dry[:, 0]
        if dsr != SR:
            print(f"[compare] WARNING: {stim} sr={dsr} != {SR}; FR metrics assume {SR}.")

        for tier in tiers:
            for g in gains:
                rgain = gain_norm_to_rkj(g)
                cpp, _, cpp_stats = run_wdf(
                    dry_path, rgain=rgain, bias=args.bias, quality=tier, return_stats=True
                )
                rust, _, rust_stats = run_rust_wdf(
                    dry_path, rgain=rgain, bias=args.bias, tier=tier,
                    return_stats=True, allow_build=False,
                )
                m = metrics(cpp, rust)  # ref=C++, mod=Rust
                fr_err, fcs, rust_db, cpp_db = fr_error(rust, cpp, dry[: min(len(rust), len(cpp))])

                row = {
                    "stim": stim, "tier": tier, "gain_norm": g, "rgain_ohm": rgain,
                    "esr_db": m["esr_db"], "nrmse": m["nrmse"], "rms_db": m["rms_db"],
                    "lag": m["lag"], "fr_err_db": fr_err,
                    "rust_avg_iters": rust_stats.get("avg_iters"),
                    "rust_max_iters": rust_stats.get("max_iters"),
                    "cpp_avg_iters": cpp_stats.get("avg_iters"),
                    "cpp_max_iters": cpp_stats.get("max_iters"),
                }
                rows.append(row)
                print(
                    f"[{stim} {tier} g={g:.2f}] ESR={m['esr_db']:6.1f} dB  "
                    f"FRerr={fr_err:.3f} dB  rms={m['rms_db']:+.3f} dB  "
                    f"iters rust={rust_stats.get('avg_iters')} cpp={cpp_stats.get('avg_iters')}"
                )
                if tier not in fr_repr and stim == stims[0]:
                    fr_repr[tier] = {"fcs": fcs, "rust_db": rust_db, "cpp_db": cpp_db,
                                     "stim": stim, "gain": g}

    # ---- ESR-vs-gain plot (one line per stim×tier) ----
    plt.figure(figsize=(8, 5))
    for stim in stims:
        for tier in tiers:
            xs = [r["gain_norm"] for r in rows if r["stim"] == stim and r["tier"] == tier]
            ys = [r["esr_db"] for r in rows if r["stim"] == stim and r["tier"] == tier]
            if xs:
                plt.plot(xs, ys, "o-", label=f"{stim} / {tier}")
    for tier, gate in GATE_ESR_DB.items():
        if tier in tiers:
            plt.axhline(gate, ls="--", lw=0.8, color="grey")
    plt.xlabel("gain_norm"); plt.ylabel("Rust-vs-C++ ESR (dB)")
    plt.title("Rust WDF port vs C++ original — ESR"); plt.grid(True, alpha=0.3); plt.legend(fontsize=8)
    plt.tight_layout(); plt.savefig(out_dir / "esr_vs_gain.png", dpi=110); plt.close()

    # ---- FR overlay (representative cell per tier) ----
    if fr_repr:
        n = len(fr_repr)
        _fig, axes = plt.subplots(n, 1, figsize=(8, 3.2 * n), squeeze=False)
        for ax, (tier, d) in zip(axes[:, 0], fr_repr.items()):
            ax.semilogx(d["fcs"], d["cpp_db"], label="C++", lw=1.4)
            ax.semilogx(d["fcs"], d["rust_db"], label="Rust", lw=1.0, ls="--")
            ax2 = ax.twinx()
            ax2.semilogx(d["fcs"], d["rust_db"] - d["cpp_db"], color="red", lw=0.7, alpha=0.6)
            ax2.set_ylabel("err (dB)", color="red"); ax2.set_ylim(-1, 1)
            ax.axvline(FR_MAX_HZ, ls=":", color="grey", lw=0.8)
            ax.set_title(f"FR — {tier} ({d['stim']}, g={d['gain']:.2f})")
            ax.set_xlabel("Hz"); ax.set_ylabel("|H| (dB)"); ax.grid(True, alpha=0.3); ax.legend(fontsize=8)
        plt.tight_layout(); plt.savefig(out_dir / "fr_rust_vs_cpp.png", dpi=110); plt.close()

    # ---- gates ----
    failures = []
    for r in rows:
        gate = GATE_ESR_DB.get(r["tier"])
        if gate is not None and r["esr_db"] > gate:
            failures.append(f"ESR {r['esr_db']:.1f} > {gate} dB at {r['stim']}/{r['tier']}/g={r['gain_norm']}")
        if r["fr_err_db"] == r["fr_err_db"] and r["fr_err_db"] > GATE_FR_DB:
            failures.append(f"FRerr {r['fr_err_db']:.3f} > {GATE_FR_DB} dB at {r['stim']}/{r['tier']}/g={r['gain_norm']}")

    worst = {t: max((r["esr_db"] for r in rows if r["tier"] == t), default=None) for t in tiers}
    summary = {
        "config": {"stims": stims, "gains": gains, "tiers": tiers, "bias": args.bias,
                   "gate_esr_db": GATE_ESR_DB, "gate_fr_db": GATE_FR_DB, "fr_max_hz": FR_MAX_HZ},
        "worst_esr_db_per_tier": worst,
        "rows": rows,
        "pass": not failures,
        "failures": failures,
    }
    (out_dir / "summary.json").write_text(json.dumps(summary, indent=2))

    print("\n=== worst Rust-vs-C++ ESR per tier ===")
    for t, w in worst.items():
        print(f"  {t:8s}: {w:.1f} dB  (gate {GATE_ESR_DB.get(t)})")
    print("PASS" if not failures else "FAIL:\n  " + "\n  ".join(failures))
    print(f"wrote {out_dir/'summary.json'}, esr_vs_gain.png, fr_rust_vs_cpp.png")


if __name__ == "__main__":
    main()
