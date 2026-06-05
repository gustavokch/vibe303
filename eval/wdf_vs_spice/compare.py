"""
Evaluate the full-chain WDF model accuracy vs the SPICE reference.

The WDF model at ``~/Git/ba283_wdf`` is a faithful full-chain BA283 emulation
(TR1+TR2+TR3+LO1166) solved by nodal MNA + Newton-Raphson — the same method
SPICE uses.  Its ``ba283_sim [Rgain] [bias]`` binary outputs **raw volts at node
SEC_IN** (DC removed), in the same volt domain as the SPICE wet WAVs, so the two
are compared **directly** (no gain normalization).

This harness reads the pre-rendered SPICE validation set (dry + wet WAVs) and runs
the WDF binary on the matching dry files at the SPICE-matched operating point
(Rgain=47 Ω ≈ 26 dB, bias per stimulus).  It produces:
  - level-ladder ESR (sweep m1..m24)            -> esr_vs_level.png, level_ladder.png
  - frequency response + coherence (pink noise) -> fr_welch.png
  - bias-tracking ESR (model is bias-aware)     -> bias_tracking.png
  - per-pair val-set ESR                        -> val_esr_bar.png, val_esr_table.csv
  - waveform overlay                            -> waveform_overlay_sweep_m6.png
  - summary.json

Methodology mirrors the WDF repo's own ``validate/compare.py``: time-aligned ESR
on raw volts with a tight cross-correlation lag (|lag| <= 256), plus Welch
transfer-function + coherence for the (phase-insensitive) linear FR check.

Usage:
    uv run python -m eval.wdf_vs_spice.compare \\
        [--spice-set ~/GoogleDrive/ba283_spice] [--rgain 47] [--bias 0.5] \\
        [--out eval/wdf_vs_spice/out]
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np
import pandas as pd
import soundfile as sf
from scipy.signal import csd, welch  # type: ignore[import-untyped]

from eval.wdf_vs_spice.run_wdf import build_if_needed, run_wdf

# Stimuli on which the WDF binary hard-failed (crashed on every retry).
WDF_CRASHES: list[str] = []
# Stimuli that crashed at least once but succeeded on a later retry.
WDF_RECOVERED: list[str] = []
# Retry budget for the intermittent WDF UB (segfaults are non-deterministic; a
# fresh process on the same input often succeeds).
WDF_RETRIES = 3

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

SR = 96_000
SWEEP_LEVELS = [1, 2, 3, 4, 6, 8, 10, 13, 16, 20, 24]  # dBFS magnitude in dataset
MAX_LAG = 256  # samples; true model latency is tiny (decimation-filter group delay)
DEFAULT_RGAIN = 47.0  # Ω, ≈26 dB, matches SPICE netlist GAIN_RKJ_OHMS
DEFAULT_BIAS = 0.5


# ---------------------------------------------------------------------------
# Signal utilities
# ---------------------------------------------------------------------------


def load_wav(path: Path) -> np.ndarray:
    """Load a WAV as mono float64 (volts), assert 96 kHz."""
    x, sr = sf.read(path, dtype="float64")
    if sr != SR:
        raise ValueError(f"Expected {SR} Hz, got {sr} Hz: {path}")
    if x.ndim > 1:
        x = x[:, 0]
    return np.asarray(x, dtype=np.float64)


def try_run_wdf(dry_path: Path, rgain: float, bias: float) -> np.ndarray | None:
    """Run the WDF binary, tolerating the intermittent UB via retries.

    The binary segfaults non-deterministically on some inputs; a fresh process on
    the same input usually succeeds. Retry up to ``WDF_RETRIES`` times and only
    record a hard failure (``WDF_CRASHES``) if every attempt fails. A stimulus that
    needed a retry is logged in ``WDF_RECOVERED``.
    """
    last_err = ""
    for attempt in range(1, WDF_RETRIES + 1):
        try:
            out, _ = run_wdf(dry_path, rgain=rgain, bias=bias)
            out = out.astype(np.float64)
            if not np.all(np.isfinite(out)):
                last_err = "non-finite output"
                raise ValueError(last_err)
            if attempt > 1:
                print(f"  [WDF recovered on attempt {attempt}] {dry_path.name}")
                WDF_RECOVERED.append(dry_path.name)
            return out
        except Exception as exc:  # noqa: BLE001 - binary crash is opaque
            last_err = str(exc).splitlines()[0] if str(exc) else "crash"
            print(f"  [WDF crash {attempt}/{WDF_RETRIES}] {dry_path.name}: {last_err}")
    WDF_CRASHES.append(dry_path.name)
    return None


def best_lag(model: np.ndarray, ref: np.ndarray, max_lag: int = MAX_LAG) -> int:
    """Integer lag (samples) maximizing cross-correlation, |lag| <= max_lag.

    Mirrors the WDF repo's validate/compare.py: a tight bound prevents the
    correlation from locking onto a spurious far-offset peak when the loud-level
    waveshapes diverge.
    """
    n = min(len(model), len(ref))
    a = model[:n] - model[:n].mean()
    b = ref[:n] - ref[:n].mean()
    best, blag = -1e30, 0
    for lag in range(-max_lag, max_lag + 1):
        if lag >= 0:
            x, y = a[lag:], b[: n - lag]
        else:
            x, y = a[: n + lag], b[-lag:]
        m = min(len(x), len(y))
        if m < n // 2:
            continue
        c = float(np.dot(x[:m], y[:m]))
        if c > best:
            best, blag = c, lag
    return blag


def align(model: np.ndarray, ref: np.ndarray, lag: int) -> tuple[np.ndarray, np.ndarray]:
    n = min(len(model), len(ref))
    if lag >= 0:
        m, r = model[lag:n], ref[: n - lag]
    else:
        m, r = model[: n + lag], ref[-lag:n]
    k = min(len(m), len(r))
    return m[:k], r[:k]


def metrics(model: np.ndarray, ref: np.ndarray) -> dict:
    """Time-aligned comparison on raw volts.

    Primary metric: ESR in dB = 10*log10(sum(err^2)/sum(ref^2)) — directly
    comparable to the project's -22 dB neural-model gate.  Also reports best-fit
    gain g (should be ~1.0 if the volt scale is correct) and NRMSE.
    """
    lag = best_lag(model, ref)
    m, r = align(model, ref, lag)
    err = m - r
    esr = float(np.sum(err**2) / (np.sum(r**2) + 1e-20))
    g = float(np.dot(m, r) / (np.dot(m, m) + 1e-20))  # diagnostic gain (volts/volt)
    esr_g = float(np.sum((g * m - r) ** 2) / (np.sum(r**2) + 1e-20))
    return {
        "lag": lag,
        "esr_db": float(10.0 * np.log10(esr + 1e-30)),
        "esr_gfit_db": float(10.0 * np.log10(esr_g + 1e-30)),
        "gain": g,
        "gain_err_db": float(20.0 * np.log10(abs(g) + 1e-30)),
        "nrmse": float(np.sqrt(np.mean(err**2)) / (r.std() + 1e-20)),
        "model_rms": float(m.std()),
        "ref_rms": float(r.std()),
        "model_peak": float(np.max(np.abs(m))),
        "ref_peak": float(np.max(np.abs(r))),
    }


# ---------------------------------------------------------------------------
# Level ladder
# ---------------------------------------------------------------------------


def run_level_ladder(
    spice_set: Path, bias: float, rgain: float, out_dir: Path
) -> dict:
    """Run each sweep level (m1..m24) and compare to SPICE at fixed bias."""
    bias_str = f"{bias:.2f}"
    results: dict[int, dict] = {}

    for lvl in SWEEP_LEVELS:
        dry_path = spice_set / "dry" / f"sweep_m{lvl}_dry.wav"
        wet_path = spice_set / "wet" / f"sweep_m{lvl}_bias{bias_str}.wav"
        if not dry_path.exists() or not wet_path.exists():
            continue
        print(f"[level_ladder] sweep_m{lvl}...")
        wet = load_wav(wet_path)
        ref_rms = float(wet.std())
        out = try_run_wdf(dry_path, rgain, bias)
        if out is None:
            results[lvl] = {"esr_db": None, "ref_rms": ref_rms, "wdf_crashed": True}
            continue
        mt = metrics(out, wet)
        mt["wdf_crashed"] = False
        results[lvl] = mt
        print(
            f"  m{lvl:2d}: ESR={mt['esr_db']:+.1f} dB  g={mt['gain']:.3f}  "
            f"lag={mt['lag']:+d}  m_rms={mt['model_rms']:.3f}V  r_rms={mt['ref_rms']:.3f}V"
        )

    if not results:
        return {}

    levels = sorted(results.keys())
    ok = [l for l in levels if results[l].get("esr_db") is not None]

    # Time-aligned ESR vs level. NOTE: sweep ESR is phase-sensitive and, at loud
    # levels, dominated by nonlinear clip-shape differences — it is NOT a pure
    # fidelity measure. The Welch FR (fr_welch.png) is the authoritative linear check.
    if ok:
        fig, ax = plt.subplots(figsize=(8, 5))
        ax.bar([f"−{l}" for l in ok], [results[l]["esr_db"] for l in ok], color="steelblue")
        ax.axhline(-22, color="green", ls="--", lw=1, label="−22 dB (neural gate)")
        ax.set_xlabel("Input level (dBFS magnitude)")
        ax.set_ylabel("Time-aligned ESR (dB)  [lower = better]")
        ax.set_title(f"Time-aligned ESR — sweep (phase-sensitive), bias={bias:.2f}, "
                     f"Rgain={rgain:.0f}Ω")
        ax.legend(fontsize=8)
        ax.grid(True, axis="y", alpha=0.4)
        ax.text(0.99, 0.02, "Authoritative linear fidelity: fr_welch.png",
                transform=ax.transAxes, ha="right", fontsize=8, color="gray")
        plt.tight_layout()
        fig.savefig(out_dir / "esr_vs_level.png", dpi=150)
        plt.close(fig)

    # RMS tracking (model vs ref volts)
    have_ref = [l for l in levels if results[l].get("ref_rms") is not None]
    fig, ax = plt.subplots(figsize=(8, 5))
    ax.plot(have_ref, [results[l]["ref_rms"] for l in have_ref], "o-", label="SPICE (V)")
    if ok:
        ax.plot(ok, [results[l]["model_rms"] for l in ok], "s--", label="WDF (V)")
    ax.set_xlabel("Input level (dBFS magnitude)")
    ax.set_ylabel("Output RMS (V)")
    ax.set_title(f"Output level tracking — bias={bias:.2f}, Rgain={rgain:.0f}Ω")
    ax.legend()
    ax.grid(True, alpha=0.4)
    ax.invert_xaxis()
    ax.set_xticks(levels)
    ax.set_xticklabels([f"−{l}" for l in levels])
    plt.tight_layout()
    fig.savefig(out_dir / "level_ladder.png", dpi=150)
    plt.close(fig)

    print("[level_ladder] saved esr_vs_level.png and level_ladder.png")
    return results


# ---------------------------------------------------------------------------
# Frequency response — Welch transfer function + coherence (pink noise)
# ---------------------------------------------------------------------------


def _welch_tf(out: np.ndarray, inp: np.ndarray, nbands: int = 28):
    """Banded |H(f)| (dB) + coherence of out relative to inp via Welch cross-spectra."""
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


def run_fr_analysis(spice_set: Path, rgain: float, out_dir: Path) -> dict:
    """Welch FR + coherence on a low-level pink-noise pair (authoritative linear check)."""
    # Prefer the lowest-level pink available (most linear / cleanest FR).
    candidates = ["pink_m18_s0", "pink_m12_s0", "pink_m6_s0", "pink_m3_s0"]
    bias = 0.5
    bias_str = f"{bias:.2f}"
    chosen = None
    for name in candidates:
        dp = spice_set / "dry" / f"{name}_dry.wav"
        wp = spice_set / "wet" / f"{name}_bias{bias_str}.wav"
        if dp.exists() and wp.exists():
            chosen = (name, dp, wp)
            break
    if chosen is None:
        print("[fr] no pink pair found, skipping FR.")
        return {}

    name, dry_path, wet_path = chosen
    print(f"[fr] Welch FR on {name} (bias {bias})...")
    dry = load_wav(dry_path)
    wet = load_wav(wet_path)
    model = try_run_wdf(dry_path, rgain, bias)
    if model is None:
        print("[fr] WDF crashed on pink, skipping FR.")
        return {}

    fcs, mdb, mco = _welch_tf(model, dry)
    _, rdb, _ = _welch_tf(wet, dry)
    err = mdb - rdb

    band = (fcs >= 20) & (fcs <= 20_000)
    midband = (fcs >= 200) & (fcs <= 5_000)

    fig, axes = plt.subplots(2, 1, figsize=(10, 7), sharex=True)
    axes[0].semilogx(fcs, rdb, "o-", color="steelblue", label="SPICE")
    axes[0].semilogx(fcs, mdb, "s--", color="darkorange", label="WDF")
    axes[0].set_ylabel("|H(f)| (dB)")
    axes[0].set_title(f"Frequency response (Welch) — {name}, bias={bias}, Rgain={rgain:.0f}Ω")
    axes[0].legend()
    axes[0].grid(True, which="both", alpha=0.4)

    axes[1].semilogx(fcs, err, color="crimson", label="WDF − SPICE")
    axes[1].axhline(0, color="k", lw=0.8, ls="--")
    axes[1].fill_between([20, 20_000], -1, 1, color="green", alpha=0.08, label="±1 dB")
    axes[1].set_ylabel("Error (dB)")
    axes[1].set_xlabel("Frequency (Hz)")
    axes[1].set_ylim(-6, 6)
    axes[1].legend(fontsize=8)
    axes[1].grid(True, which="both", alpha=0.4)
    axes[1].set_xlim(20, 40_000)
    plt.tight_layout()
    fig.savefig(out_dir / "fr_welch.png", dpi=150)
    plt.close(fig)

    # Coherence plot
    fig, ax = plt.subplots(figsize=(10, 3.5))
    ax.semilogx(fcs, mco, color="purple")
    ax.set_ylim(0, 1.02)
    ax.set_xlim(20, 40_000)
    ax.set_xlabel("Frequency (Hz)")
    ax.set_ylabel("Coherence")
    ax.set_title(f"Model coherence vs dry input — {name}")
    ax.grid(True, which="both", alpha=0.4)
    plt.tight_layout()
    fig.savefig(out_dir / "fr_coherence.png", dpi=150)
    plt.close(fig)

    print("[fr] saved fr_welch.png and fr_coherence.png")
    return {
        "stimulus": name,
        "max_err_db_20Hz_20kHz": round(float(np.max(np.abs(err[band]))), 2),
        "max_err_db_midband_200_5k": round(float(np.max(np.abs(err[midband]))), 2),
        "mean_coherence_midband": round(float(np.mean(mco[midband])), 3),
    }


# ---------------------------------------------------------------------------
# Bias tracking (the model IS bias-aware now)
# ---------------------------------------------------------------------------


def run_bias_tracking(spice_set: Path, rgain: float, out_dir: Path) -> dict:
    """Run the model at each bias matching the SPICE wet, on sweep_m6."""
    dry_path = spice_set / "dry" / "sweep_m6_dry.wav"
    if not dry_path.exists():
        return {}
    biases = [0.0, 0.25, 0.5, 0.75, 1.0]
    esr: dict[float, float] = {}
    m_rms: dict[float, float] = {}
    r_rms: dict[float, float] = {}

    for b in biases:
        wet_path = spice_set / "wet" / f"sweep_m6_bias{b:.2f}.wav"
        if not wet_path.exists():
            continue
        wet = load_wav(wet_path)
        out = try_run_wdf(dry_path, rgain, b)
        if out is None:
            continue
        mt = metrics(out, wet)
        esr[b] = mt["esr_db"]
        m_rms[b] = mt["model_rms"]
        r_rms[b] = mt["ref_rms"]
        print(f"[bias] bias={b:.2f}: ESR={mt['esr_db']:+.1f} dB  "
              f"m_rms={mt['model_rms']:.3f}V  r_rms={mt['ref_rms']:.3f}V")

    if not esr:
        return {}

    bs = sorted(esr.keys())
    fig, axes = plt.subplots(1, 2, figsize=(12, 4))
    axes[0].plot(bs, [r_rms[b] for b in bs], "o-", label="SPICE")
    axes[0].plot(bs, [m_rms[b] for b in bs], "s--", label="WDF")
    axes[0].set_xlabel("Bias (RV1, 0–1)")
    axes[0].set_ylabel("Output RMS (V)")
    axes[0].set_title("Bias tracking — sweep_m6")
    axes[0].legend()
    axes[0].grid(True, alpha=0.4)

    axes[1].plot(bs, [esr[b] for b in bs], "s-", color="darkorange")
    axes[1].axhline(-22, color="green", ls="--", lw=1, label="−22 dB gate")
    axes[1].set_xlabel("Bias")
    axes[1].set_ylabel("ESR (dB)  [lower = better]")
    axes[1].set_title("WDF vs SPICE per bias")
    axes[1].legend(fontsize=8)
    axes[1].grid(True, alpha=0.4)
    plt.tight_layout()
    fig.savefig(out_dir / "bias_tracking.png", dpi=150)
    plt.close(fig)
    print("[bias] saved bias_tracking.png")

    return {
        "esr_per_bias_db": {str(k): round(v, 2) for k, v in esr.items()},
        "model_rms_per_bias": {str(k): round(v, 4) for k, v in m_rms.items()},
        "spice_rms_per_bias": {str(k): round(v, 4) for k, v in r_rms.items()},
    }


# ---------------------------------------------------------------------------
# Waveform overlay
# ---------------------------------------------------------------------------


def plot_overlay(model: np.ndarray, ref: np.ndarray, label: str, out_dir: Path,
                 segment_s: float = 0.05, start_s: float = 2.0) -> None:
    lag = best_lag(model, ref)
    m, r = align(model, ref, lag)
    start = int(start_s * SR)
    n = int(segment_s * SR)
    t = np.arange(n) / SR * 1000
    m_seg, r_seg = m[start:start + n], r[start:start + n]
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
    fig.savefig(out_dir / f"waveform_overlay_{label}.png", dpi=150)
    plt.close(fig)
    print(f"[overlay] saved waveform_overlay_{label}.png")


# ---------------------------------------------------------------------------
# Val-set ESR (each pair at its own bias)
# ---------------------------------------------------------------------------


def run_val_esr(spice_set: Path, rgain: float, out_dir: Path) -> list[dict]:
    val_csv = spice_set / "val.csv"
    if not val_csv.exists():
        return []
    df = pd.read_csv(val_csv)
    df["dry_base"] = df["dry_path"].apply(lambda p: Path(p).name)
    df["wet_base"] = df["wet_path"].apply(lambda p: Path(p).name)

    records = []
    for _, row in df.iterrows():
        dry_path = spice_set / "dry" / row["dry_base"]
        wet_path = spice_set / "wet" / row["wet_base"]
        bias = float(row["bias_value"])
        if not dry_path.exists() or not wet_path.exists():
            continue
        print(f"[val] {row['dry_base']} (bias={bias:.2f}, {row['stim_kind']})...")
        wet = load_wav(wet_path)
        out = try_run_wdf(dry_path, rgain, bias)
        if out is None:
            records.append({"dry": row["dry_base"], "stim_kind": row["stim_kind"],
                            "bias": bias, "esr_db": None, "wdf_crashed": True})
            continue
        mt = metrics(out, wet)
        records.append({
            "dry": row["dry_base"], "stim_kind": row["stim_kind"], "bias": bias,
            "esr_db": round(mt["esr_db"], 2), "gain": round(mt["gain"], 3),
            "nrmse": round(mt["nrmse"], 3), "wdf_crashed": False,
        })
        print(f"  ESR={mt['esr_db']:+.1f} dB  g={mt['gain']:.3f}  NRMSE={mt['nrmse']:.3f}")

    if records:
        pd.DataFrame(records).to_csv(out_dir / "val_esr_table.csv", index=False)
        ok = [r for r in records if not r.get("wdf_crashed")]
        if ok:
            fig, ax = plt.subplots(figsize=(max(8, len(ok) * 0.6), 5))
            labels = [f"{r['dry'].replace('_dry.wav','')[:24]}\n@{r['bias']:.2f}" for r in ok]
            ax.bar(range(len(ok)), [r["esr_db"] for r in ok], color="steelblue")
            ax.axhline(-22, color="green", ls="--", lw=1, label="−22 dB gate")
            ax.set_xticks(range(len(ok)))
            ax.set_xticklabels(labels, rotation=45, ha="right", fontsize=7)
            ax.set_ylabel("Time-aligned ESR (dB)  [lower = better]")
            ax.set_title(f"Val-set time-aligned ESR — WDF vs SPICE (Rgain={rgain:.0f}Ω)")
            ax.legend(fontsize=8)
            ax.grid(True, axis="y", alpha=0.4)
            plt.tight_layout()
            fig.savefig(out_dir / "val_esr_bar.png", dpi=150)
            plt.close(fig)
        print("[val] saved val_esr_table.csv and val_esr_bar.png")
    return records


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("--spice-set", type=Path,
                    default=Path.home() / "GoogleDrive/ba283_spice")
    ap.add_argument("--rgain", type=float, default=DEFAULT_RGAIN,
                    help="WDF gain shunt R_KJ in ohms (default 47 ≈ 26 dB).")
    ap.add_argument("--bias", type=float, default=DEFAULT_BIAS,
                    help="Bias for the level-ladder stage (default 0.5).")
    ap.add_argument("--out", type=Path, default=Path("eval/wdf_vs_spice/out"))
    ap.add_argument("--no-build", action="store_true",
                    help="Never rebuild ba283_sim; use the binary on disk as-is "
                         "(for runs while the WDF source is mid-edit).")
    args = ap.parse_args()

    args.out.mkdir(parents=True, exist_ok=True)
    build_if_needed(allow_build=not args.no_build)

    print("=" * 60)
    print("WDF (full-chain MNA) vs SPICE accuracy evaluation")
    print(f"  SPICE set : {args.spice_set}")
    print(f"  Rgain     : {args.rgain} Ω    bias (ladder): {args.bias}")
    print(f"  metric    : raw-volts ESR, |lag|<= {MAX_LAG}")
    print("=" * 60)

    print("\n--- Frequency response (Welch, pink) ---")
    fr_results = run_fr_analysis(args.spice_set, args.rgain, args.out)

    print("\n--- Level ladder ---")
    ladder = run_level_ladder(args.spice_set, args.bias, args.rgain, args.out)

    print("\n--- Bias tracking ---")
    bias_results = run_bias_tracking(args.spice_set, args.rgain, args.out)

    print("\n--- Waveform overlay (sweep_m6) ---")
    mid_dry = args.spice_set / "dry" / "sweep_m6_dry.wav"
    mid_wet = args.spice_set / "wet" / f"sweep_m6_bias{args.bias:.2f}.wav"
    if mid_dry.exists() and mid_wet.exists():
        mid_out = try_run_wdf(mid_dry, args.rgain, args.bias)
        if mid_out is not None:
            plot_overlay(mid_out, load_wav(mid_wet), "sweep_m6", args.out)

    print("\n--- Val-set ESR ---")
    val_records = run_val_esr(args.spice_set, args.rgain, args.out)

    # Aggregate
    usable = [r for r in val_records if not r.get("wdf_crashed")]
    esr_vals = [r["esr_db"] for r in usable]
    ladder_ok = {k: v for k, v in ladder.items() if v.get("esr_db") is not None}

    summary = {
        "config": {"rgain_ohm": args.rgain, "ladder_bias": args.bias,
                   "max_lag_samples": MAX_LAG, "metric": "raw-volts ESR (dB)"},
        "model": "full-chain BA283 (TR1+TR2+TR3+LO1166), nodal MNA + Newton",
        "wdf_stability": {
            "crashed_stimuli": sorted(set(WDF_CRASHES)),
            "recovered_on_retry": sorted(set(WDF_RECOVERED)),
            "retry_budget": WDF_RETRIES,
        },
        "frequency_response": fr_results,
        "level_ladder": {str(k): {"esr_db": v.get("esr_db"), "gain": v.get("gain"),
                                  "model_rms_v": v.get("model_rms"),
                                  "ref_rms_v": v.get("ref_rms")}
                         for k, v in ladder.items()},
        "level_ladder_best_esr_db": round(min(v["esr_db"] for v in ladder_ok.values()), 2)
        if ladder_ok else None,
        "bias_tracking": bias_results,
        "val_set_esr": {
            "mean_db": round(float(np.mean(esr_vals)), 2) if esr_vals else None,
            "median_db": round(float(np.median(esr_vals)), 2) if esr_vals else None,
            "best_db": round(float(np.min(esr_vals)), 2) if esr_vals else None,
            "worst_db": round(float(np.max(esr_vals)), 2) if esr_vals else None,
            "n_pairs": len(val_records),
        },
        "notes": [
            "Model output is raw volts at SEC_IN; compared directly to SPICE wet (no gain fit).",
            "ESR on a frequency sweep is phase-sensitive — small group-delay/transformer-"
            "resonance phase diffs inflate it even when |H(f)| matches; the Welch FR + "
            "coherence is the authoritative linear-fidelity check.",
            "gain ~1.0 in metrics confirms the volt scale matches SPICE.",
            f"Rgain={args.rgain}Ω matches the SPICE netlist GAIN_RKJ_OHMS (≈26 dB).",
        ],
    }
    (args.out / "summary.json").write_text(json.dumps(summary, indent=2))

    print("\n" + "=" * 60)
    print("SUMMARY")
    print("=" * 60)
    if fr_results:
        print(f"  FR max err 20Hz–20kHz : {fr_results['max_err_db_20Hz_20kHz']:+.1f} dB")
        print(f"  FR max err midband    : {fr_results['max_err_db_midband_200_5k']:+.1f} dB")
        print(f"  Midband coherence     : {fr_results['mean_coherence_midband']:.3f}")
    if ladder_ok:
        print(f"  Best sweep ESR        : {summary['level_ladder_best_esr_db']:+.1f} dB")
    if esr_vals:
        print(f"  Val-set ESR median    : {np.median(esr_vals):+.1f} dB")
        print(f"  Val-set ESR best/worst: {np.min(esr_vals):+.1f} / {np.max(esr_vals):+.1f} dB")
    if WDF_RECOVERED:
        print(f"  Recovered on retry    : {len(set(WDF_RECOVERED))} stimuli")
    if WDF_CRASHES:
        print(f"  Hard crashes          : {sorted(set(WDF_CRASHES))}")
    print("=" * 60)
    print(f"Plots + summary in: {args.out}")


if __name__ == "__main__":
    main()
