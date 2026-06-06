"""
Evaluate the full-chain WDF model vs the SPICE reference — GAIN-conditioned.

The corrected BA283 dataset (data_gen/netlist.py, drawing EX/10283) is conditioned
on **gain at a fixed bias**: RV1 is the set-once bias trim, pinned at 0.77
(~65 mA TR3 idle, symmetric clipping), while the K↔J gain resistor R_KJ is swept
(``gain_norm ∈ [0,1]`` → ``gain_norm_to_rkj`` → ~82 Ω…10 kΩ ≈ 33…14 dB).

This harness mirrors the WDF repo's authoritative ``validate/gain_validate.py``:
for each (dry stimulus, gain) it **regenerates the SPICE wet target on the fly**
from the current corrected netlist (each sim isolated in a one-shot child process
so PySpice's ngspice singleton can't leak RAM), runs the C++ model on the same dry
stimulus, time-aligns them (tight lag, 0.5 s onset skip), and reports ESR / NRMSE /
level match.  It adds plots + a summary.json + a Welch FR/coherence check on top of
gain_validate.py's text table.

Usage (run under this repo's uv env so data_gen resolves):
    uv run python -m eval.wdf_vs_spice.compare \\
        [--stims pink_m3_s0,sweep_m3,...] [--gains 0.0,0.25,0.5,0.75,1.0] \\
        [--bias 0.77] [--out eval/wdf_vs_spice/out] [--no-build]
"""
from __future__ import annotations

import argparse
import json
import multiprocessing as mp
import tempfile
from pathlib import Path

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np
import soundfile as sf
from scipy.interpolate import PchipInterpolator  # type: ignore[import-untyped]
from scipy.signal import csd, welch  # type: ignore[import-untyped]

from data_gen.netlist import gain_norm_to_rkj
from data_gen.run_spice import simulate_one
from eval.wdf_vs_spice.run_wdf import build_if_needed, run_wdf

# Stimuli on which the WDF binary hard-failed (crashed on every retry).
WDF_CRASHES: list[str] = []
# Stimuli that crashed at least once but succeeded on a later retry.
WDF_RECOVERED: list[str] = []
WDF_RETRIES = 3
# (stim, gain_norm) SPICE points that failed (e.g. ngspice memory cap on ~5 s sims).
SPICE_FAILURES: list[str] = []
SPICE_RETRIES = 2

SR = 96_000
BIAS = 0.77            # fixed bias for the gain-conditioned dataset
ONSET_SKIP = 0.5       # seconds trimmed before scoring (startup-transient guard)
SPAN = 64              # cross-correlation lag bound (samples)
DEFAULT_STIMS = ["pink_m3_s0", "sweep_m3", "music3_08 - Shed - Rave Echoes_m1"]
DEFAULT_GAINS = [0.0, 0.25, 0.5, 0.75, 1.0]  # gain_norm grid
DRY_DIR = Path.home() / "GoogleDrive/ba283_spice/dry"


# ---------------------------------------------------------------------------
# SPICE (on-the-fly) and model runners
# ---------------------------------------------------------------------------


def _spice_worker(args: tuple[str, float, float, str]) -> None:
    """One-shot SPICE sim in a throwaway child process."""
    dry, bias, gain, npz = args
    simulate_one(Path(dry), bias, Path(npz), gain_rkj=gain)


def run_spice(dry: Path, gain: float, bias: float, nfix: int) -> np.ndarray:
    """Regenerate the SPICE wet target for (dry, gain, bias) → uniform SR grid (volts).

    Each sim runs in a fresh child (maxtasksperchild=1) so PySpice's ngspice
    singleton cannot accumulate RAM across a sweep. ngspice can still hit its
    internal memory cap on a ~5 s 96 kHz×2 sim (affects gain_validate.py too); we
    retry a couple of times since a fresh process with more free system RAM may
    succeed, then raise so the caller records the failed point.
    """
    last: Exception | None = None
    for _ in range(SPICE_RETRIES + 1):
        npz = Path(tempfile.mktemp(suffix=".npz"))
        try:
            with mp.Pool(1, maxtasksperchild=1) as pool:
                pool.map(_spice_worker, [(str(dry), bias, gain, str(npz))])
            z = np.load(npz)
            t, v = z["t"], z["v"]
            tu = np.arange(0.0, t[-1], 1.0 / SR)
            vu = PchipInterpolator(t, v)(tu)
            return vu[:nfix]  # drop the settle tail; match dry length
        except Exception as exc:  # noqa: BLE001 - ngspice failure is opaque
            last = exc
        finally:
            npz.unlink(missing_ok=True)
            Path(str(npz)[:-4] + ".src").unlink(missing_ok=True)
    raise RuntimeError(f"SPICE failed after {SPICE_RETRIES + 1} attempts: {last}")


def try_run_wdf(dry_path: Path, rgain: float, bias: float) -> np.ndarray | None:
    """Run the WDF binary, tolerating the intermittent UB via retries."""
    last_err = ""
    for attempt in range(1, WDF_RETRIES + 1):
        try:
            out, _ = run_wdf(dry_path, rgain=rgain, bias=bias)
            out = out.astype(np.float64)
            if not np.all(np.isfinite(out)):
                last_err = "non-finite output"
                raise ValueError(last_err)
            if attempt > 1:
                print(f"    [WDF recovered on attempt {attempt}] {dry_path.name}")
                WDF_RECOVERED.append(dry_path.name)
            return out
        except Exception as exc:  # noqa: BLE001 - binary crash is opaque
            last_err = str(exc).splitlines()[0] if str(exc) else "crash"
            print(f"    [WDF crash {attempt}/{WDF_RETRIES}] {dry_path.name}: {last_err}")
    WDF_CRASHES.append(dry_path.name)
    return None


# ---------------------------------------------------------------------------
# Alignment + metrics (mirrors validate/gain_validate.py)
# ---------------------------------------------------------------------------


def best_lag(ref: np.ndarray, mod: np.ndarray, span: int = SPAN) -> int:
    n = min(len(ref), len(mod))
    a = ref[:n] - ref[:n].mean()
    b = mod[:n] - mod[:n].mean()
    best, blag = -1e30, 0
    for lag in range(-span, span + 1):
        if lag >= 0:
            c = float(np.dot(a[lag:], b[: n - lag]))
        else:
            c = float(np.dot(a[: n + lag], b[-lag:]))
        if c > best:
            best, blag = c, lag
    return blag


def _align(ref: np.ndarray, mod: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    lag = best_lag(ref, mod)
    if lag >= 0:
        r, m = ref[lag:], mod[: len(ref) - lag]
    else:
        r, m = ref[: len(ref) + lag], mod[-lag:]
    n = min(len(r), len(m))
    r, m = r[:n], m[:n]
    k = min(int(ONSET_SKIP * SR), n // 4)  # skip startup transient
    return r[k:], m[k:]


def metrics(ref: np.ndarray, mod: np.ndarray) -> dict:
    lag = best_lag(ref, mod)
    r, m = _align(ref, mod)
    err = r - m
    esr = float(np.sum(err**2) / max(np.sum(r**2), 1e-20))
    nrmse = float(np.sqrt(np.mean(err**2)) / max(r.max() - r.min(), 1e-20))
    rr, mr = float(np.sqrt(np.mean(r**2))), float(np.sqrt(np.mean(m**2)))
    return {
        "lag": lag,
        "esr": esr,
        "esr_db": float(10.0 * np.log10(esr + 1e-30)),
        "nrmse": nrmse,
        "ref_rms": rr,
        "mod_rms": mr,
        "rms_db": float(20.0 * np.log10(mr / max(rr, 1e-20))),
        "ref_pk": float(np.max(np.abs(r))),
        "mod_pk": float(np.max(np.abs(m))),
    }


# ---------------------------------------------------------------------------
# Welch FR + coherence
# ---------------------------------------------------------------------------


def _welch_tf(out: np.ndarray, inp: np.ndarray, nbands: int = 28):
    n = min(len(out), len(inp))
    nper = 16384
    f, Pxx = welch(inp[:n], SR, nperseg=nper)
    _, Pyy = welch(out[:n], SR, nperseg=nper)
    _, Pxy = csd(inp[:n], out[:n], SR, nperseg=nper)
    H = np.abs(Pxy) / (Pxx + 1e-30)
    coh = (np.abs(Pxy) ** 2) / (Pxx * Pyy + 1e-30)
    edges = np.geomspace(20.0, min(40000.0, SR / 2 * 0.98), nbands + 1)
    fcs, dbs, cohs = [], [], []
    for k in range(nbands):
        sel = (f >= edges[k]) & (f < edges[k + 1])
        if not sel.any():
            continue
        fcs.append(float(np.sqrt(edges[k] * edges[k + 1])))
        dbs.append(float(20.0 * np.log10(np.sqrt(np.mean(H[sel] ** 2)) + 1e-12)))
        cohs.append(float(np.mean(coh[sel])))
    return np.array(fcs), np.array(dbs), np.array(cohs)


def run_fr_analysis(gain_norm: float, out_dir: Path) -> dict:
    """Welch FR + coherence on pink at one gain (authoritative linear check)."""
    stem = "pink_m3_s0"
    dry_path = DRY_DIR / f"{stem}_dry.wav"
    if not dry_path.exists():
        print("[fr] pink stimulus missing, skipping FR.")
        return {}
    gain = gain_norm_to_rkj(gain_norm)
    print(f"[fr] Welch FR on {stem} @ gain_norm={gain_norm} (R_KJ={gain:.0f}Ω)...")
    dry = np.asarray(sf.read(dry_path, always_2d=False)[0], dtype=np.float64)
    nfix = dry.shape[0]
    try:
        ref = run_spice(dry_path, gain, BIAS, nfix)
    except Exception as exc:  # noqa: BLE001 - ngspice may hit its memory cap on pink
        print(f"[fr] SPICE failed ({exc}); skipping FR.")
        SPICE_FAILURES.append(f"{stem}@{gain_norm}")
        return {}
    model = try_run_wdf(dry_path, gain, BIAS)
    if model is None:
        return {}
    n = min(len(dry), len(ref), len(model))
    fcs, mdb, mco = _welch_tf(model[:n], dry[:n])
    _, rdb, _ = _welch_tf(ref[:n], dry[:n])
    err = mdb - rdb
    band = (fcs >= 20) & (fcs <= 20_000)
    midband = (fcs >= 200) & (fcs <= 5_000)

    fig, axes = plt.subplots(2, 1, figsize=(10, 7), sharex=True)
    axes[0].semilogx(fcs, rdb, "o-", color="steelblue", label="SPICE")
    axes[0].semilogx(fcs, mdb, "s--", color="darkorange", label="WDF")
    axes[0].set_ylabel("|H(f)| (dB)")
    axes[0].set_title(f"Frequency response (Welch) — {stem}, "
                      f"gain_norm={gain_norm} (R_KJ={gain:.0f}Ω), bias={BIAS}")
    axes[0].legend()
    axes[0].grid(True, which="both", alpha=0.4)
    axes[1].semilogx(fcs, err, color="crimson", label="WDF − SPICE")
    axes[1].axhline(0, color="k", lw=0.8, ls="--")
    axes[1].fill_between([20, 20_000], -1, 1, color="green", alpha=0.08, label="±1 dB")
    axes[1].set_ylabel("Error (dB)")
    axes[1].set_xlabel("Frequency (Hz)")
    axes[1].set_ylim(-6, 6)
    axes[1].set_xlim(20, 40_000)
    axes[1].legend(fontsize=8)
    axes[1].grid(True, which="both", alpha=0.4)
    plt.tight_layout()
    fig.savefig(out_dir / "fr_welch.png", dpi=150)
    plt.close(fig)

    fig, ax = plt.subplots(figsize=(10, 3.5))
    ax.semilogx(fcs, mco, color="purple")
    ax.set_ylim(0, 1.02)
    ax.set_xlim(20, 40_000)
    ax.set_xlabel("Frequency (Hz)")
    ax.set_ylabel("Coherence")
    ax.set_title(f"Model coherence vs dry — {stem}")
    ax.grid(True, which="both", alpha=0.4)
    plt.tight_layout()
    fig.savefig(out_dir / "fr_coherence.png", dpi=150)
    plt.close(fig)

    print("[fr] saved fr_welch.png and fr_coherence.png")
    return {
        "stimulus": stem,
        "gain_norm": gain_norm,
        "rkj_ohm": round(gain, 1),
        "max_err_db_20Hz_20kHz": round(float(np.max(np.abs(err[band]))), 2),
        "max_err_db_midband_200_5k": round(float(np.max(np.abs(err[midband]))), 2),
        "mean_coherence_midband": round(float(np.mean(mco[midband])), 3),
    }


# ---------------------------------------------------------------------------
# Gain ladder (the core: per-stimulus ESR across the gain grid)
# ---------------------------------------------------------------------------


def run_gain_ladder(stims: list[str], gains: list[float], out_dir: Path,
                    overlay_at: float = 0.5) -> dict:
    """For each stimulus, sweep the gain grid; SPICE regenerated per point."""
    pts = [(gn, gain_norm_to_rkj(gn)) for gn in gains]
    results: dict[str, list[dict]] = {}
    overlay_done = False

    for stem in stims:
        dry_path = DRY_DIR / f"{stem}_dry.wav"
        if not dry_path.exists():
            print(f"[gain] MISSING {dry_path}")
            continue
        dry = np.asarray(sf.read(dry_path, always_2d=False)[0], dtype=np.float64)
        nfix = dry.shape[0]
        rows: list[dict] = []
        print(f"[gain] {stem}")
        for gn, g in pts:
            try:
                ref = run_spice(dry_path, g, BIAS, nfix)
            except Exception as exc:  # noqa: BLE001
                print(f"    gain_norm={gn:.2f} (R_KJ={g:.0f}): SPICE FAIL {exc}")
                SPICE_FAILURES.append(f"{stem}@{gn}")
                rows.append({"gain_norm": gn, "rkj": g, "esr_db": None, "spice_failed": True})
                continue
            mod = try_run_wdf(dry_path, g, BIAS)
            if mod is None:
                rows.append({"gain_norm": gn, "rkj": g, "esr_db": None, "crashed": True})
                continue
            mt = metrics(ref, mod)
            mt.update({"gain_norm": gn, "rkj": g, "crashed": False})
            rows.append(mt)
            print(f"    gain_norm={gn:.2f} R_KJ={g:>5.0f}Ω  ESR={mt['esr_db']:+6.1f} dB  "
                  f"NRMSE={mt['nrmse']:.4f}  rms={mt['rms_db']:+.2f} dB  "
                  f"pk {mt['ref_pk']:.3f}/{mt['mod_pk']:.3f}  lag={mt['lag']:+d}")

            # one waveform overlay (mid-gain, first available stim)
            if not overlay_done and abs(gn - overlay_at) < 1e-9:
                _overlay(ref, mod, f"{stem}_g{gn:.2f}", out_dir)
                overlay_done = True
        results[stem] = rows

    # ESR-vs-gain plot (all stimuli)
    fig, ax = plt.subplots(figsize=(9, 5))
    for stem, rows in results.items():
        ok = [r for r in rows if r.get("esr_db") is not None]
        if ok:
            ax.plot([r["gain_norm"] for r in ok], [r["esr_db"] for r in ok],
                    "o-", label=stem[:28])
    ax.axhline(-22, color="green", ls="--", lw=1, label="−22 dB (neural gate)")
    ax.set_xlabel("gain_norm (0 = low gain ~14 dB → 1 = high ~33 dB)")
    ax.set_ylabel("Time-aligned ESR (dB)  [lower = better]")
    ax.set_title(f"WDF vs SPICE across gain sweep — bias={BIAS} (0.5 s onset-skipped)")
    ax.legend(fontsize=8)
    ax.grid(True, alpha=0.4)
    ax.text(0.99, 0.02, "Authoritative linear fidelity: fr_welch.png",
            transform=ax.transAxes, ha="right", fontsize=8, color="gray")
    plt.tight_layout()
    fig.savefig(out_dir / "esr_vs_gain.png", dpi=150)
    plt.close(fig)
    print("[gain] saved esr_vs_gain.png")
    return results


def _overlay(ref: np.ndarray, mod: np.ndarray, label: str, out_dir: Path,
             segment_s: float = 0.05, start_s: float = 2.0) -> None:
    r, m = _align(ref, mod)
    start = int(start_s * SR)
    n = int(segment_s * SR)
    if start + n > len(r):
        start = 0
    t = np.arange(n) / SR * 1000
    r_seg, m_seg = r[start:start + n], m[start:start + n]
    fig, axes = plt.subplots(2, 1, figsize=(10, 6), sharex=True)
    axes[0].plot(t, r_seg, color="steelblue", lw=0.9, label="SPICE")
    axes[0].plot(t, m_seg, color="darkorange", lw=0.9, ls="--", label="WDF")
    axes[0].set_ylabel("Volts")
    axes[0].set_title(f"Time-domain overlay — {label}")
    axes[0].legend(fontsize=9)
    axes[0].grid(True, alpha=0.4)
    axes[1].plot(t, r_seg - m_seg, color="crimson", lw=0.9)
    axes[1].set_ylabel("Error (V)")
    axes[1].set_xlabel("Time (ms)")
    axes[1].set_title("Residual (SPICE − WDF)")
    axes[1].grid(True, alpha=0.4)
    plt.tight_layout()
    safe = label.replace(" ", "_").replace("/", "_")
    fig.savefig(out_dir / f"waveform_overlay_{safe}.png", dpi=150)
    plt.close(fig)
    print(f"[overlay] saved waveform_overlay_{safe}.png")


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------


def main() -> None:
    global BIAS
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("--stims", default=",".join(DEFAULT_STIMS),
                    help="comma-separated dry-stimulus stems (without _dry.wav).")
    ap.add_argument("--gains", default=",".join(str(g) for g in DEFAULT_GAINS),
                    help="comma-separated gain_norm grid in [0,1].")
    ap.add_argument("--bias", type=float, default=BIAS,
                    help="fixed RV1 bias (default 0.77).")
    ap.add_argument("--out", type=Path, default=Path("eval/wdf_vs_spice/out"))
    ap.add_argument("--no-build", action="store_true",
                    help="never rebuild ba283_sim; use the binary on disk as-is.")
    args = ap.parse_args()

    BIAS = args.bias
    args.out.mkdir(parents=True, exist_ok=True)
    build_if_needed(allow_build=not args.no_build)

    stims = [s.strip() for s in args.stims.split(",") if s.strip()]
    gains = [float(x) for x in args.gains.split(",")]

    print("=" * 64)
    print("WDF (full-chain MNA) vs SPICE — GAIN-conditioned evaluation")
    print(f"  bias (fixed) : {BIAS}")
    print(f"  gain grid    : {gains} (gain_norm → R_KJ via netlist)")
    print(f"  stimuli      : {stims}")
    print(f"  metric       : raw-volts ESR, |lag|<={SPAN}, {ONSET_SKIP}s onset-skip")
    print("  SPICE        : regenerated on-the-fly from data_gen/netlist.py")
    print("=" * 64)

    print("\n--- Frequency response (Welch, pink) ---")
    fr = run_fr_analysis(0.5, args.out)

    print("\n--- Gain ladder (SPICE regenerated per point) ---")
    ladder = run_gain_ladder(stims, gains, args.out)

    # Aggregate
    all_rows = [r for rows in ladder.values() for r in rows if r.get("esr_db") is not None]
    esr_vals = [r["esr_db"] for r in all_rows]

    summary = {
        "config": {"bias_fixed": BIAS, "gain_norm_grid": gains, "stims": stims,
                   "max_lag_samples": SPAN, "onset_skip_s": ONSET_SKIP,
                   "metric": "raw-volts ESR (dB), 0.5s onset-skipped",
                   "spice": "on-the-fly from data_gen/netlist.py (corrected EX/10283)"},
        "model": "full-chain BA283 (TR1+TR2+TR3+LO1166), nodal MNA + Newton, gain-conditioned",
        "wdf_stability": {
            "crashed_stimuli": sorted(set(WDF_CRASHES)),
            "recovered_on_retry": sorted(set(WDF_RECOVERED)),
            "retry_budget": WDF_RETRIES,
        },
        "spice_failures": {
            "points": sorted(set(SPICE_FAILURES)),
            "note": "ngspice memory cap on ~5 s 96 kHz×2 sims (affects gain_validate.py too).",
        },
        "frequency_response": fr,
        "gain_ladder": {
            stem: [{"gain_norm": r["gain_norm"], "rkj_ohm": round(r["rkj"], 1),
                    "esr_db": (round(r["esr_db"], 2) if r.get("esr_db") is not None else None),
                    "nrmse": (round(r["nrmse"], 4) if "nrmse" in r else None),
                    "rms_db": (round(r["rms_db"], 2) if "rms_db" in r else None),
                    "ref_pk": (round(r["ref_pk"], 3) if "ref_pk" in r else None),
                    "mod_pk": (round(r["mod_pk"], 3) if "mod_pk" in r else None)}
                   for r in rows]
            for stem, rows in ladder.items()
        },
        "esr_summary_db": {
            "median": round(float(np.median(esr_vals)), 2) if esr_vals else None,
            "best": round(float(np.min(esr_vals)), 2) if esr_vals else None,
            "worst": round(float(np.max(esr_vals)), 2) if esr_vals else None,
            "n_points": len(all_rows),
        },
        "notes": [
            "Dataset is conditioned on GAIN at fixed bias 0.77 (the corrected netlist).",
            "SPICE wet is regenerated per point from data_gen/netlist.py — no stale "
            "pre-rendered set.",
            "ESR is time-domain L2 (phase-sensitive on clean broadband/sweeps); the "
            "Welch FR + coherence is the authoritative linear-fidelity check.",
            "First 0.5 s skipped: SPICE starts from .op steady-state, the model from a "
            "rail-ramp warmup — a switch-on artifact a streaming plugin never sees.",
        ],
    }
    (args.out / "summary.json").write_text(json.dumps(summary, indent=2))

    print("\n" + "=" * 64)
    print("SUMMARY")
    print("=" * 64)
    if fr:
        print(f"  FR max err 20Hz–20kHz : {fr['max_err_db_20Hz_20kHz']:+.1f} dB")
        print(f"  FR max err midband    : {fr['max_err_db_midband_200_5k']:+.1f} dB")
        print(f"  Midband coherence     : {fr['mean_coherence_midband']:.3f}")
    if esr_vals:
        print(f"  ESR median            : {np.median(esr_vals):+.1f} dB")
        print(f"  ESR best/worst        : {np.min(esr_vals):+.1f} / {np.max(esr_vals):+.1f} dB")
    if WDF_RECOVERED:
        print(f"  Recovered on retry    : {len(set(WDF_RECOVERED))} stimuli")
    if WDF_CRASHES:
        print(f"  Hard crashes          : {sorted(set(WDF_CRASHES))}")
    if SPICE_FAILURES:
        print(f"  SPICE failed points   : {sorted(set(SPICE_FAILURES))} (ngspice mem cap)")
    print("=" * 64)
    print(f"Plots + summary in: {args.out}")


if __name__ == "__main__":
    main()
