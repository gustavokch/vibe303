#!/usr/bin/env python3
"""Evaluate and plot aliasing performance of WarmClipper DSP module.

Runs the compiled `eval_clipper` Rust binary across different signal configurations:
1. PSD Spectrum Comparison (Bypass vs 2x OS vs 4x OS) for off-grid test tone.
2. Alias-to-Signal Ratio (ASR dB) vs Fundamental Frequency (100 Hz to 10 kHz).
3. THD & Aliasing vs Drive Saturation (Drive resistance 22k to 100k).

Generates theme-aware, dataviz-compliant plots saved to `eval/303/results/clipper_aliasing/`.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path
import numpy as np

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt

# --- Path Configuration -----------------------------------------------------
HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
RESULTS_DIR = HERE / "results" / "clipper_aliasing"
RESULTS_DIR.mkdir(parents=True, exist_ok=True)

EVAL_CLIPPER_REL = ROOT / "target" / "release" / "eval_clipper"
EVAL_CLIPPER_DBG = ROOT / "target" / "debug" / "eval_clipper"
EVAL_CLIPPER = EVAL_CLIPPER_REL if EVAL_CLIPPER_REL.exists() else EVAL_CLIPPER_DBG

if not EVAL_CLIPPER.exists():
    raise FileNotFoundError(
        f"eval_clipper binary not found at {EVAL_CLIPPER_REL} or {EVAL_CLIPPER_DBG}. "
        "Run `cargo build --release --bin eval_clipper` first."
    )

# --- Palette & Styling (Dataviz skill compliant) -----------------------------
STYLE = {
    "slate": "#64748b",      # Bypass (1x)
    "blue": "#2563eb",       # 2x OS
    "emerald": "#059669",    # 4x OS
    "bg_light": "#ffffff",
    "bg_card": "#f8fafc",
    "text_dark": "#0f172a",
    "grid": "#e2e8f0",
}

plt.rcParams.update({
    "font.sans-serif": ["Inter", "Helvetica", "Arial", "sans-serif"],
    "axes.edgecolor": STYLE["grid"],
    "axes.linewidth": 1.0,
    "grid.color": STYLE["grid"],
    "grid.linestyle": "--",
    "grid.alpha": 0.7,
})


# --- Audio Processing Subprocess --------------------------------------------

def render_clipper(
    sr: float = 48000.0,
    os: str = "2x",
    bypass: bool = False,
    drive: float = 47000.0,
    thresh: float = 4700.0,
    bass: float = 8200.0,
    tone: float = 1e-9,
    asym: bool = False,
    signal: str = "sine",
    freq: float = 1700.0,
    amp: float = 1.0,
    secs: float = 1.0,
    mix: float = 1.0,
    in_gain: float = 0.0,
    out_gain: float = 0.0,
    tilt: float = 0.0,
    dynamics: float = 0.0,
) -> np.ndarray:
    """Render WarmClipper output via eval_clipper CLI binary."""
    with tempfile.NamedTemporaryFile(suffix=".f32", delete=False) as tmp_out:
        out_path = Path(tmp_out.name)

    try:
        cmd = [
            str(EVAL_CLIPPER),
            "--sr", str(sr),
            "--os", os,
            "--drive", str(drive),
            "--thresh", str(thresh),
            "--bass", str(bass),
            "--tone", str(tone),
            "--signal", signal,
            "--freq", str(freq),
            "--amp", str(amp),
            "--secs", str(secs),
            "--mix", str(mix),
            "--in-gain", str(in_gain),
            "--out-gain", str(out_gain),
            "--tilt", str(tilt),
            "--dynamics", str(dynamics),
            "--out", str(out_path),
        ]
        if bypass:
            cmd.append("--bypass")
        if asym:
            cmd.append("--asym")

        subprocess.run(cmd, check=True, capture_output=True)
        samples = np.fromfile(out_path, dtype="<f4").astype(np.float64)
        return samples
    finally:
        if out_path.exists():
            out_path.unlink()


# --- Spectral & Aliasing Analysis --------------------------------------------
def compute_aliasing_metrics(
    samples: np.ndarray,
    sr: float,
    f0: float,
    skip_transient_sec: float = 0.1,
    tolerance_hz: float = 15.0,
) -> dict:
    """Compute Harmonic Energy, Alias (non-harmonic) Energy, and ASR (dB)."""
    skip_samples = int(sr * skip_transient_sec)
    steady = samples[skip_samples:]
    if len(steady) < 1024:
        steady = samples

    n = len(steady)
    window = np.hanning(n)
    fft_vals = np.fft.rfft(steady * window)
    freqs = np.fft.rfftfreq(n, d=1.0 / sr)
    power = (np.abs(fft_vals) / (n / 2.0)) ** 2

    # Identify harmonic frequencies k * f0 within Nyquist
    max_k = int((sr / 2.0) / f0)
    harmonic_mask = np.zeros(len(freqs), dtype=bool)

    for k in range(1, max_k + 1):
        fk = k * f0
        if fk < sr / 2.0:
            harmonic_mask |= np.abs(freqs - fk) <= tolerance_hz

    # Exclude DC and sub-bass (< 20 Hz)
    dc_mask = freqs < 20.0

    # Total signal power above 20 Hz
    total_power = np.sum(power[~dc_mask])
    harm_power = np.sum(power[harmonic_mask & ~dc_mask])
    alias_power = np.sum(power[(~harmonic_mask) & (~dc_mask)])

    fund_band = np.abs(freqs - f0) <= tolerance_hz
    fund_power = np.sum(power[fund_band])
    harm_minus_fund = np.sum(power[harmonic_mask & ~dc_mask & ~fund_band])

    asr_db = 10.0 * np.log10(max(alias_power, 1e-300) / max(harm_power, 1e-300))
    thd_db = 10.0 * np.log10(max(harm_minus_fund, 1e-300) / max(fund_power, 1e-300))

    return {
        "freqs": freqs,
        "power_db": 10.0 * np.log10(np.maximum(power, 1e-12)),
        "total_power": float(total_power),
        "harm_power": float(harm_power),
        "alias_power": float(alias_power),
        "asr_db": float(asr_db),
        "thd_db": float(thd_db),
    }


# --- Evaluation Sweeps & Plotting --------------------------------------------
def plot_psd_comparison(sr: float = 48000.0, f0: float = 1700.0, secs: float = 1.0):
    """Plot 1: PSD Spectrum comparison of Bypass vs 2x OS vs 4x OS."""
    print(f"[1/3] Running PSD Spectrum comparison (f0 = {f0} Hz, sr = {sr} Hz)...")

    s_bypass = render_clipper(sr=sr, os="2x", bypass=True, signal="sine", freq=f0, secs=secs)
    s_2x = render_clipper(sr=sr, os="2x", bypass=False, signal="sine", freq=f0, secs=secs)
    s_4x = render_clipper(sr=sr, os="4x", bypass=False, signal="sine", freq=f0, secs=secs)

    m_bypass = compute_aliasing_metrics(s_bypass, sr, f0)
    m_2x = compute_aliasing_metrics(s_2x, sr, f0)
    m_4x = compute_aliasing_metrics(s_4x, sr, f0)

    fig, ax = plt.subplots(figsize=(10, 5), dpi=150)
    fig.patch.set_facecolor(STYLE["bg_light"])
    ax.set_facecolor(STYLE["bg_card"])

    freqs = m_bypass["freqs"] / 1000.0  # kHz

    ax.plot(freqs, m_bypass["power_db"], label=f"Bypass / 1x (ASR: {m_bypass['asr_db']:.1f} dB)", color=STYLE["slate"], alpha=0.7, lw=1.2)
    ax.plot(freqs, m_2x["power_db"], label=f"2x Oversampling (ASR: {m_2x['asr_db']:.1f} dB)", color=STYLE["blue"], alpha=0.85, lw=1.5)
    ax.plot(freqs, m_4x["power_db"], label=f"4x Oversampling (ASR: {m_4x['asr_db']:.1f} dB)", color=STYLE["emerald"], alpha=0.9, lw=1.5)

    ax.set_xlim(0, sr / 2000.0)
    ax.set_ylim(-120, 10)
    ax.set_xlabel("Frequency (kHz)", fontsize=11, fontweight="medium", color=STYLE["text_dark"])
    ax.set_ylabel("Power Spectral Density (dBFS)", fontsize=11, fontweight="medium", color=STYLE["text_dark"])
    ax.set_title(f"WarmClipper PSD Spectrum & Aliasing Fold-down ($f_0 = {f0:.0f}$ Hz, $f_s = {sr/1000:.1f}$ kHz)", fontsize=12, fontweight="bold", pad=12)

    ax.grid(True)
    ax.legend(loc="upper right", frameon=True, facecolor=STYLE["bg_light"], edgecolor=STYLE["grid"])

    out_png = RESULTS_DIR / "clipper_psd_spectrum.png"
    plt.tight_layout()
    plt.savefig(out_png)
    plt.close()
    print(f" Saved: {out_png}")

    return {
        "f0_hz": f0,
        "sr_hz": sr,
        "bypass_asr_db": m_bypass["asr_db"],
        "os2x_asr_db": m_2x["asr_db"],
        "os4x_asr_db": m_4x["asr_db"],
    }


def plot_asr_freq_sweep(sr: float = 48000.0):
    """Plot 2: Alias-to-Signal Ratio vs Fundamental Frequency (100 Hz - 10 kHz)."""
    print("[2/3] Sweeping fundamental frequency f0 (100 Hz to 10 kHz)...")

    # Pick off-grid frequencies to avoid exact harmonic cancellation
    test_freqs = np.geomspace(100.0, 9500.0, num=30)

    asr_bypass = []
    asr_2x = []
    asr_4x = []

    for f0 in test_freqs:
        s_b = render_clipper(sr=sr, os="2x", bypass=True, signal="sine", freq=f0, secs=0.4)
        s_2 = render_clipper(sr=sr, os="2x", bypass=False, signal="sine", freq=f0, secs=0.4)
        s_4 = render_clipper(sr=sr, os="4x", bypass=False, signal="sine", freq=f0, secs=0.4)

        asr_bypass.append(compute_aliasing_metrics(s_b, sr, f0)["asr_db"])
        asr_2x.append(compute_aliasing_metrics(s_2, sr, f0)["asr_db"])
        asr_4x.append(compute_aliasing_metrics(s_4, sr, f0)["asr_db"])

    fig, ax = plt.subplots(figsize=(10, 5), dpi=150)
    fig.patch.set_facecolor(STYLE["bg_light"])
    ax.set_facecolor(STYLE["bg_card"])

    ax.semilogx(test_freqs, asr_bypass, "o-", label="Bypass (1x)", color=STYLE["slate"], lw=1.5, ms=4)
    ax.semilogx(test_freqs, asr_2x, "s-", label="2x Oversampling", color=STYLE["blue"], lw=1.8, ms=5)
    ax.semilogx(test_freqs, asr_4x, "^-", label="4x Oversampling", color=STYLE["emerald"], lw=1.8, ms=5)

    ax.set_xlim(100, 10000)
    ax.set_ylim(-100, 5)
    ax.set_xlabel("Fundamental Frequency $f_0$ (Hz)", fontsize=11, fontweight="medium", color=STYLE["text_dark"])
    ax.set_ylabel("Alias-to-Signal Ratio (dB)", fontsize=11, fontweight="medium", color=STYLE["text_dark"])
    ax.set_title("WarmClipper Aliasing Floor vs Fundamental Frequency ($f_s = 48$ kHz)", fontsize=12, fontweight="bold", pad=12)

    ax.grid(True, which="both")
    ax.legend(loc="upper left", frameon=True, facecolor=STYLE["bg_light"], edgecolor=STYLE["grid"])

    out_png = RESULTS_DIR / "clipper_asr_vs_freq.png"
    plt.tight_layout()
    plt.savefig(out_png)
    plt.close()
    print(f" Saved: {out_png}")


def plot_drive_aliasing_sweep(sr: float = 48000.0, f0: float = 2500.0):
    """Plot 3: THD & Aliasing vs Drive Resistance (Drive saturation sweep)."""
    print("[3/3] Sweeping Drive resistance (22 kΩ to 100 kΩ)...")

    drive_ohms = np.linspace(22000.0, 100000.0, num=20)

    thd_2x, asr_2x = [], []
    thd_4x, asr_4x = [], []

    for d in drive_ohms:
        s_2 = render_clipper(sr=sr, os="2x", bypass=False, drive=d, signal="sine", freq=f0, secs=0.4)
        s_4 = render_clipper(sr=sr, os="4x", bypass=False, drive=d, signal="sine", freq=f0, secs=0.4)

        m_2 = compute_aliasing_metrics(s_2, sr, f0)
        m_4 = compute_aliasing_metrics(s_4, sr, f0)

        thd_2x.append(m_2["thd_db"])
        asr_2x.append(m_2["asr_db"])
        thd_4x.append(m_4["thd_db"])
        asr_4x.append(m_4["asr_db"])

    fig, ax = plt.subplots(figsize=(10, 5), dpi=150)
    fig.patch.set_facecolor(STYLE["bg_light"])
    ax.set_facecolor(STYLE["bg_card"])

    ax.plot(drive_ohms / 1000.0, asr_2x, "s--", label="Alias Floor (2x OS)", color=STYLE["blue"], lw=1.8, ms=5)
    ax.plot(drive_ohms / 1000.0, asr_4x, "^--", label="Alias Floor (4x OS)", color=STYLE["emerald"], lw=1.8, ms=5)
    ax.plot(drive_ohms / 1000.0, thd_2x, "o-", label="Total Harmonic Distortion (THD)", color=STYLE["slate"], lw=1.5, ms=4)

    ax.set_xlabel("Drive Resistance $R_{\\text{DRIVE}}$ (kΩ)", fontsize=11, fontweight="medium", color=STYLE["text_dark"])
    ax.set_ylabel("Level (dB)", fontsize=11, fontweight="medium", color=STYLE["text_dark"])
    ax.set_title(f"Harmonic Distortion & Aliasing Rejection vs Drive Saturation ($f_0 = {f0:.0f}$ Hz)", fontsize=12, fontweight="bold", pad=12)

    ax.grid(True)
    ax.legend(loc="center right", frameon=True, facecolor=STYLE["bg_light"], edgecolor=STYLE["grid"])

    out_png = RESULTS_DIR / "clipper_aliasing_vs_drive.png"
    plt.tight_layout()
    plt.savefig(out_png)
    plt.close()
    print(f" Saved: {out_png}")


def run_control_sweeps(sr: float = 48000.0, f0: float = 2500.0):
    """Sweep 4: THD & ASR across the new control dimensions (mix, tilt, dynamics)."""
    print("[4/4] Running control-dimension sweeps (mix, tilt, dynamics)...")

    rows = []
    for m in [0.0, 0.25, 0.5, 0.75, 1.0]:
        s = render_clipper(sr=sr, os="2x", signal="sine", freq=f0, mix=m)
        met = compute_aliasing_metrics(s, sr, f0)
        rows.append({"sweep": "mix", "value": m, "thd_db": met["thd_db"], "asr_db": met["asr_db"]})
    for d in [0.0, 0.5, 1.0]:
        s = render_clipper(sr=sr, os="2x", signal="sine", freq=f0, drive=100000.0, dynamics=d)
        met = compute_aliasing_metrics(s, sr, f0)
        rows.append({"sweep": "dynamics", "value": d, "thd_db": met["thd_db"], "asr_db": met["asr_db"]})
    for t in [-6.0, 0.0, 6.0]:
        s = render_clipper(sr=sr, os="2x", signal="sine", freq=f0, tilt=t)
        met = compute_aliasing_metrics(s, sr, f0)
        rows.append({"sweep": "tilt", "value": t, "thd_db": met["thd_db"], "asr_db": met["asr_db"]})

    out_json = RESULTS_DIR / "clipper_control_sweeps.json"
    out_json.write_text(json.dumps(rows, indent=2))
    print(f"  -> saved {out_json}")
    for r in rows:
        print(f"  {r['sweep']:>8} = {r['value']:>5}: THD {r['thd_db']:.1f} dB, ASR {r['asr_db']:.1f} dB")


def main():
    parser = argparse.ArgumentParser(description="Evaluate & plot WarmClipper aliasing performance.")
    parser.add_argument("--sr", type=float, default=48000.0, help="Sample rate in Hz (default: 48000)")
    parser.add_argument("--f0", type=float, default=1700.0, help="Test frequency in Hz (default: 1700)")
    args = parser.parse_args()

    print(f"Starting WarmClipper aliasing evaluation pipeline (sr={args.sr} Hz)...")
    summary = plot_psd_comparison(sr=args.sr, f0=args.f0)
    plot_asr_freq_sweep(sr=args.sr)
    plot_drive_aliasing_sweep(sr=args.sr)
    run_control_sweeps(sr=args.sr)

    # Save summary report JSON
    json_path = RESULTS_DIR / "summary.json"
    with open(json_path, "w") as f:
        json.dump(summary, f, indent=2)

    print("\n--- Evaluation Summary ---")
    print(f"Test Frequency : {summary['f0_hz']} Hz")
    print(f"Sample Rate    : {summary['sr_hz']} Hz")
    print(f"Bypass ASR     : {summary['bypass_asr_db']:.2f} dB")
    print(f"2x OS ASR      : {summary['os2x_asr_db']:.2f} dB")
    print(f"4x OS ASR      : {summary['os4x_asr_db']:.2f} dB")
    print(f"Summary saved  : {json_path}")
    print("All plots successfully generated in:", RESULTS_DIR)


if __name__ == "__main__":
    main()
