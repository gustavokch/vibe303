#!/usr/bin/env python3
"""Full Tier-vs-SPICE Measurement Suite.

Renders SPICE reference waveforms (at 192 kHz, downsampled to 96 kHz) and
Rust voice tier renders (at 96 kHz) across a comprehensive parameter grid.
Computes time-domain, spectral, and loudness metrics, outputs formatted tables,
and generates waveform/spectrum plots and metric heatmaps.
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
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from matplotlib.figure import Figure
from matplotlib.backends.backend_agg import FigureCanvasAgg as FigureCanvas

# Insert current directory into path for local imports
HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
sys.path.insert(0, str(HERE))

import esr
import voice_ref_spice as vr
from parallel import run_parallel_with_fallback

# Optimize esr.best_lag: compute correlation only for required lags in max_lag.
# Demean and use numpy dot product vectorization to run 12x faster than original on full signal.
def optimized_full_best_lag(ref, mod, max_lag=256):
    n = min(len(ref), len(mod))
    if n <= 0:
        return 0
    r = ref[:n] - ref[:n].mean()
    m = mod[:n] - mod[:n].mean()
    
    best_lag_val = 0
    best_corr = -np.inf
    
    for lag in range(-max_lag, max_lag + 1):
        if lag > 0:
            corr_val = np.dot(r[lag:], m[:-lag])
        elif lag < 0:
            k = -lag
            corr_val = np.dot(r[:-k], m[k:])
        else:
            corr_val = np.dot(r, m)
            
        if corr_val > best_corr:
            best_corr = corr_val
            best_lag_val = lag
            
    return best_lag_val

esr.best_lag = optimized_full_best_lag

# Setup path to Rust renderer
RENDER303_REL = ROOT / "target" / "release" / "render303"
RENDER303_DBG = ROOT / "target" / "debug" / "render303"
RENDER303 = RENDER303_REL if RENDER303_REL.exists() else RENDER303_DBG


# --- Kaiser-Sinc Downsampling (192 kHz -> 96 kHz) ----------------------------
def lowpass_decimate_2(x: np.ndarray) -> np.ndarray:
    """Kaiser-sinc lowpass filter and decimate by 2.
    Converts 192 kHz signal to 96 kHz.
    Cutoff frequency fc = 44.1 kHz (normalized fc = 44.1 / 192.0 ≈ 0.23).
    """
    N = 65
    fc = 0.23
    t = np.arange(N) - (N - 1) / 2
    with np.errstate(divide='ignore', invalid='ignore'):
        h = np.sin(2 * np.pi * fc * t) / (np.pi * t)
    h[(N - 1) // 2] = 2 * fc
    h *= np.kaiser(N, 6.0)
    h /= np.sum(h)
    
    y = np.convolve(x, h, mode="same")
    return y[::2]


# --- Spectral Analysis Helpers ----------------------------------------------
def welch_pow(x: np.ndarray, seg: int = 1024) -> np.ndarray:
    """Hann, 50 % overlap, averaged power spectrum normalised to unit sum."""
    win = np.hanning(seg)
    hop = seg // 2
    nseg = max((len(x) - seg) // hop + 1, 1)
    acc = np.zeros(seg // 2)
    for s in range(0, nseg * hop, hop):
        seg_data = x[s:s + seg]
        if len(seg_data) < seg:
            break
        sp = np.fft.rfft(seg_data * win)[: seg // 2]
        acc += np.abs(sp) ** 2
    acc /= nseg
    return acc / max(acc.sum(), 1e-300)


def spectral_centroid(x: np.ndarray, sr: float, seg: int = 1024) -> float:
    """Calculate the spectral centroid (Hz) of the signal using Welch power spectrum."""
    p = welch_pow(x, seg)
    freqs = np.fft.rfftfreq(seg, 1 / sr)[:seg // 2]
    return float(np.sum(freqs * p) / max(np.sum(p), 1e-300))


def spectral_match(gold: np.ndarray, model: np.ndarray, sr: float, seg: int = 1024,
                   band: tuple[float, float] = (40.0, 16000.0)) -> tuple[float, float]:
    """Calculate power-weighted LSD (dB) and Pearson correlation of dB spectra."""
    gp = welch_pow(gold, seg)
    mp = welch_pow(model, seg)
    df = sr / seg
    lo, hi = int(band[0] / df), int(band[1] / df)
    gp, mp = gp[lo:hi], mp[lo:hi]
    gdb = 10 * np.log10(np.maximum(gp, 1e-300))
    mdb = 10 * np.log10(np.maximum(mp, 1e-300))
    gdb -= gdb.mean()
    mdb -= mdb.mean()
    lsd = float(np.sqrt(np.sum(gp * (gdb - mdb) ** 2) / max(gp.sum(), 1e-300)))
    corr_matrix = np.corrcoef(gdb, mdb)
    corr = float(corr_matrix[0, 1]) if np.all(np.isfinite(corr_matrix)) else 0.0
    return lsd, corr


# --- Loudness Helpers --------------------------------------------------------
def rms_dbfs(x: np.ndarray) -> float:
    """Root-mean-square level in dBFS."""
    rms = np.sqrt(np.mean(x ** 2))
    return float(20.0 * np.log10(rms + 1e-15))


def crest_factor_db(x: np.ndarray) -> float:
    """Crest factor (peak-to-RMS ratio) in dB."""
    peak = np.max(np.abs(x))
    rms = np.sqrt(np.mean(x ** 2))
    return float(20.0 * np.log10(peak / max(rms, 1e-15) + 1e-15))


def norm_signal(x: np.ndarray) -> np.ndarray:
    """Normalize signal to unit RMS."""
    rms = np.sqrt(np.mean(x ** 2))
    return x / rms if rms > 1e-20 else x


# --- Render Subprocess ------------------------------------------------------
def render_rust(note: int, cutoff_hz: float, reso_knob: float, wave_idx: int, tier_idx: int,
                sr: float, secs: float) -> np.ndarray:
    """Render Rust voice via subprocess to render303 binary."""
    with tempfile.TemporaryDirectory() as d:
        out = Path(d) / "r.f32"
        # render303 expects reso as a percentage [0, 100]
        subprocess.run(
            [
                str(RENDER303),
                "--note", str(note),
                "--sr", str(sr),
                "--secs", str(secs),
                "--cutoff", str(cutoff_hz),
                "--reso", str(reso_knob * 100.0),
                "--wave", str(wave_idx),
                "--tier", str(tier_idx),
                "--out", str(out)
            ],
            check=True, capture_output=True
        )
        return np.fromfile(out, dtype="<f4").astype(np.float64)


# --- Plotting Helpers --------------------------------------------------------
def plot_waveform(ref: np.ndarray, low: np.ndarray, high: np.ndarray, ultra: np.ndarray,
                  sr: float, title: str, filepath: Path):
    """Plot overlay of aligned waveforms (first 50 ms for visual detail)."""
    fig = Figure(figsize=(10, 4))
    canvas = FigureCanvas(fig)
    ax = fig.add_subplot(111)
    
    n_samples = min(int(0.05 * sr), len(ref))
    t = np.arange(n_samples) / sr * 1000.0  # ms
    
    # Lag align for visualization
    r_n = norm_signal(ref)
    l_n = norm_signal(low)
    h_n = norm_signal(high)
    u_n = norm_signal(ultra)
    
    # Get lags vs ref
    lag_l = esr.best_lag(r_n, l_n)
    lag_h = esr.best_lag(r_n, h_n)
    lag_u = esr.best_lag(r_n, u_n)
    
    def align_slice(sig, lag):
        if lag > 0:
            return np.pad(sig[lag:], (0, lag))[:n_samples]
        elif lag < 0:
            return np.pad(sig[:lag], (-lag, 0))[:n_samples]
        return sig[:n_samples]
        
    ax.plot(t, r_n[:n_samples], label="SPICE Ref", color="black", alpha=0.8, linewidth=2)
    ax.plot(t, align_slice(l_n, lag_l), label="Low (TPT)", color="#e06666", alpha=0.7, linestyle="--")
    ax.plot(t, align_slice(h_n, lag_h), label="High (Nodal 2x)", color="#6fa8dc", alpha=0.7)
    ax.plot(t, align_slice(u_n, lag_u), label="Ultra (Nodal 4x)", color="#8e7cc3", alpha=0.8)
    
    ax.set_title(title)
    ax.set_xlabel("Time (ms)")
    ax.set_ylabel("Normalized Amplitude")
    ax.legend()
    ax.grid(True, alpha=0.3)
    fig.tight_layout()
    fig.savefig(filepath, dpi=150)


def plot_spectrum(ref_spectrum: np.ndarray | None, low_spectrum: np.ndarray,
                  high_spectrum: np.ndarray, ultra_spectrum: np.ndarray,
                  sr: float, title: str, filepath: Path):
    """Plot spectrum overlays (Welch dB spectra)."""
    fig = Figure(figsize=(10, 4))
    canvas = FigureCanvas(fig)
    ax = fig.add_subplot(111)
    
    seg = len(low_spectrum) * 2
    freqs = np.fft.rfftfreq(seg, 1 / sr)[:seg // 2]
    
    if ref_spectrum is not None:
        ref_db = 10 * np.log10(np.maximum(ref_spectrum, 1e-30))
        ref_db -= ref_db.mean()
        ax.semilogx(freqs, ref_db, label="SPICE Ref", color="black", alpha=0.8, linewidth=2)
        
    low_db = 10 * np.log10(np.maximum(low_spectrum, 1e-30))
    low_db -= low_db.mean()
    high_db = 10 * np.log10(np.maximum(high_spectrum, 1e-30))
    high_db -= high_db.mean()
    ultra_db = 10 * np.log10(np.maximum(ultra_spectrum, 1e-30))
    ultra_db -= ultra_db.mean()
    
    ax.semilogx(freqs, low_db, label="Low (TPT)", color="#e06666", alpha=0.7, linestyle="--")
    ax.semilogx(freqs, high_db, label="High (Nodal 2x)", color="#6fa8dc", alpha=0.7)
    ax.semilogx(freqs, ultra_db, label="Ultra (Nodal 4x)", color="#8e7cc3", alpha=0.8)
    
    ax.set_title(title)
    ax.set_xlabel("Frequency (Hz)")
    ax.set_ylabel("Normalized Power (dB)")
    ax.set_xlim(40.0, 16000.0)
    ax.legend()
    ax.grid(True, which="both", alpha=0.3)
    fig.tight_layout()
    fig.savefig(filepath, dpi=150)


def plot_heatmap(data: np.ndarray, row_labels: list[str], col_labels: list[str],
                 title: str, filepath: Path, fmt: str = ".2f", cmap: str = "viridis"):
    """Plot summary heatmap for a metric."""
    fig = Figure(figsize=(8, 12))
    canvas = FigureCanvas(fig)
    ax = fig.add_subplot(111)
    im = ax.imshow(data, cmap=cmap, aspect="auto")
    
    fig.colorbar(im, ax=ax, label=title)
    
    ax.set_xticks(np.arange(len(col_labels)))
    ax.set_yticks(np.arange(len(row_labels)))
    
    ax.set_xticklabels(col_labels)
    ax.set_yticklabels(row_labels, fontsize=8)
    
    for label in ax.get_xticklabels():
        label.set_rotation(45)
        label.set_ha("right")
        label.set_rotation_mode("anchor")
    
    for i in range(len(row_labels)):
        for j in range(len(col_labels)):
            val = data[i, j]
            ax.text(j, i, f"{val:{fmt[1:]}}", ha="center", va="center", 
                    color="white" if im.norm(val) < 0.5 else "black", fontsize=8)
            
    ax.set_title(title)
    fig.tight_layout()
    fig.savefig(filepath, dpi=150)


# --- Main Executable --------------------------------------------------------
def main():
    parser = argparse.ArgumentParser(description="Grid evaluation suite comparing Vibe303 tiers to SPICE.")
    parser.add_argument("--out-dir", type=str, default="eval/303/results",
                        help="Output directory for plots, tables, and JSON.")
    parser.add_argument("-w", "--workers", type=int, default=4,
                        help="Number of workers for parallel execution")
    args = parser.parse_args()
    
    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    
    if not RENDER303.exists():
        print(f"Error: {RENDER303} does not exist. Run 'cargo build --bin render303 -p plugin303' first.", file=sys.stderr)
        sys.exit(1)
        
    # Grid settings
    waveforms = [("Saw", 0), ("Square", 1)]
    cutoffs = [("16uA", 16e-6), ("64uA", 64e-6), ("128uA", 128e-6)]
    resonances = [("0.0", 0.0, 1e12), ("0.35", 0.35, 150e3), ("0.60", 0.6, 68e3), ("0.90", 0.9, 33e3)]
    notes = [("A1", 45), ("A2", 57), ("A3", 69), ("A4", 81)]
    
    spice_sr = 192000.0
    rust_sr = 96000.0
    secs = 0.2
    
    # Cache CV pitch solves per note at 192 kHz
    def solve_note(name, note):
        freq = vr.note_to_freq(note)
        cv = vr.solve_cv(freq, sr=spice_sr, secs=0.2)
        return note, cv

    solved_cvs = run_parallel_with_fallback(
        solve_note, notes, desc="Solving CV", max_workers=args.workers
    )
    cv_cache = {note: cv for note, cv in solved_cvs}
    print(f"\nSolved {len(cv_cache)} CV pitch targets.")
        
    results = []
    
    # Store indices for heatmap matrices
    saw_conds = []
    sq_conds = []
    
    # Data structures for heatmaps
    saw_esr_matrix = []
    saw_lsd_matrix = []
    saw_corr_matrix = []
    saw_centroid_err_matrix = []
    saw_rms_delta_matrix = []
    
    sq_lsd_matrix = []
    sq_corr_matrix = []
    
    total_spice = len(cutoffs) * len(resonances) * len(notes)
    total_rust = total_spice * len(waveforms) * 3
    print(f"\nStarting Grid Evaluation: {total_spice} SPICE renders, {total_rust} Rust renders...")
    
    spice_cache = {}
    
    # 1. First pre-render all SPICE references to optimize
    spice_tasks = []
    for c_name, c_val in cutoffs:
        for r_name, r_val, r_reso in resonances:
            for n_name, n_val in notes:
                spice_tasks.append((n_val, c_val, r_reso))

    def render_spice_task(n_val, c_val, r_reso):
        ref_192k = vr.voice_reference(
            note=n_val,
            cutoff_itail=c_val,
            rreso=r_reso,
            sr=spice_sr,
            secs=secs,
            cv=cv_cache[n_val]
        )
        return (n_val, c_val, r_reso), lowpass_decimate_2(ref_192k)

    spice_results = run_parallel_with_fallback(
        render_spice_task, spice_tasks, desc="Rendering SPICE", max_workers=args.workers
    )
    for key, ref in spice_results:
        spice_cache[key] = ref
        
    print("\nSPICE references rendered.")
    
    # 2. Main loop over waveforms and tiers
    rust_tasks = []
    for w_name, w_idx in waveforms:
        for c_name, c_val in cutoffs:
            for r_name, r_val, r_reso in resonances:
                for n_name, n_val in notes:
                    rust_tasks.append((w_name, w_idx, c_name, c_val, r_name, r_val, r_reso, n_name, n_val))

    def evaluate_rust_task(w_name, w_idx, c_name, c_val, r_name, r_val, r_reso, n_name, n_val):
        cond_name = f"{n_name}_{c_name}_r{r_name}"
        cutoff_hz = vr.itail_to_hz(c_val)
        
        rust_renders = {}
        for tier_name, tier_idx in [("Low", 0), ("High", 1), ("Ultra", 2)]:
            rust_renders[tier_name] = render_rust(
                note=n_val,
                cutoff_hz=cutoff_hz,
                reso_knob=r_val,
                wave_idx=w_idx,
                tier_idx=tier_idx,
                sr=rust_sr,
                secs=secs
            )
            
        metrics_per_tier = {}
        
        tier_data = {
            "w_name": w_name,
            "cond_name": cond_name,
            "n_val": n_val,
            "c_val": c_val,
            "r_val": r_val,
        }
        
        if w_name == "Saw":
            ref_96k = spice_cache[(n_val, c_val, r_reso)]
            ref_norm = norm_signal(ref_96k)
            ref_centroid = spectral_centroid(ref_96k, rust_sr)
            ref_rms = rms_dbfs(ref_96k)
            ref_pow = welch_pow(ref_96k)
            
            tier_esr_row = []
            tier_lsd_row = []
            tier_corr_row = []
            tier_centroid_err_row = []
            tier_rms_delta_row = []
            
            for t_name in ["Low", "High", "Ultra"]:
                model = rust_renders[t_name]
                model_norm = norm_signal(model)
                
                m_time = esr.metrics(ref_norm, model_norm)
                lsd, corr = spectral_match(ref_96k, model, rust_sr)
                
                model_centroid = spectral_centroid(model, rust_sr)
                centroid_err = (model_centroid - ref_centroid) / ref_centroid * 100.0
                
                model_rms = rms_dbfs(model)
                rms_delta = model_rms - ref_rms
                crest = crest_factor_db(model)
                
                metrics_per_tier[t_name] = {
                    "esr_db": m_time["esr_db"],
                    "nrmse": m_time["nrmse"],
                    "lag": m_time["lag"],
                    "lsd_db": lsd,
                    "corr": corr,
                    "centroid_hz": model_centroid,
                    "centroid_err_pct": centroid_err,
                    "rms_dbfs": model_rms,
                    "rms_delta_db": rms_delta,
                    "crest_db": crest
                }
                
                tier_esr_row.append(m_time["esr_db"])
                tier_lsd_row.append(lsd)
                tier_corr_row.append(corr)
                tier_centroid_err_row.append(centroid_err)
                tier_rms_delta_row.append(rms_delta)
            
            tier_data["saw"] = {
                "esr_row": tier_esr_row,
                "lsd_row": tier_lsd_row,
                "corr_row": tier_corr_row,
                "centroid_err_row": tier_centroid_err_row,
                "rms_delta_row": tier_rms_delta_row,
            }
            
            # Defer plotting
            plot_waveform(
                ref=ref_96k,
                low=rust_renders["Low"],
                high=rust_renders["High"],
                ultra=rust_renders["Ultra"],
                sr=rust_sr,
                title=f"Waveform Overlay (Saw): {cond_name}",
                filepath=out_dir / f"waveform_saw_{cond_name}.png"
            )
            
            low_p = welch_pow(rust_renders["Low"])
            high_p = welch_pow(rust_renders["High"])
            ultra_p = welch_pow(rust_renders["Ultra"])
            
            plot_spectrum(
                ref_spectrum=ref_pow,
                low_spectrum=low_p,
                high_spectrum=high_p,
                ultra_spectrum=ultra_p,
                sr=rust_sr,
                title=f"Spectrum Overlay (Saw): {cond_name}",
                filepath=out_dir / f"spectrum_saw_{cond_name}.png"
            )
            
        else:
            low_baseline = rust_renders["Low"]
            
            tier_lsd_row = []
            tier_corr_row = []
            
            for t_name in ["Low", "High", "Ultra"]:
                model = rust_renders[t_name]
                
                lsd, corr = spectral_match(low_baseline, model, rust_sr)
                
                model_centroid = spectral_centroid(model, rust_sr)
                model_rms = rms_dbfs(model)
                crest = crest_factor_db(model)
                
                metrics_per_tier[t_name] = {
                    "lsd_vs_low_db": lsd,
                    "corr_vs_low": corr,
                    "centroid_hz": model_centroid,
                    "rms_dbfs": model_rms,
                    "crest_db": crest
                }
                
                if t_name in ["High", "Ultra"]:
                    tier_lsd_row.append(lsd)
                    tier_corr_row.append(corr)
            
            tier_data["square"] = {
                "lsd_row": tier_lsd_row,
                "corr_row": tier_corr_row,
            }
            
            low_p = welch_pow(rust_renders["Low"])
            high_p = welch_pow(rust_renders["High"])
            ultra_p = welch_pow(rust_renders["Ultra"])
            
            plot_spectrum(
                ref_spectrum=None,
                low_spectrum=low_p,
                high_spectrum=high_p,
                ultra_spectrum=ultra_p,
                sr=rust_sr,
                title=f"Spectrum Overlay (Square): {cond_name}",
                filepath=out_dir / f"spectrum_square_{cond_name}.png"
            )

        tier_data["metrics"] = metrics_per_tier
        return tier_data

    rust_results = run_parallel_with_fallback(
        evaluate_rust_task, rust_tasks, desc="Rendering Rust & evaluating", max_workers=args.workers
    )

    for tier_data in rust_results:
        w_name = tier_data["w_name"]
        cond_name = tier_data["cond_name"]
        
        if w_name == "Saw":
            saw_conds.append(cond_name)
            saw_esr_matrix.append(tier_data["saw"]["esr_row"])
            saw_lsd_matrix.append(tier_data["saw"]["lsd_row"])
            saw_corr_matrix.append(tier_data["saw"]["corr_row"])
            saw_centroid_err_matrix.append(tier_data["saw"]["centroid_err_row"])
            saw_rms_delta_matrix.append(tier_data["saw"]["rms_delta_row"])
        else:
            sq_conds.append(cond_name)
            sq_lsd_matrix.append(tier_data["square"]["lsd_row"])
            sq_corr_matrix.append(tier_data["square"]["corr_row"])
            
        results.append({
            "waveform": w_name,
            "condition": cond_name,
            "note": tier_data["n_val"],
            "cutoff_itail": tier_data["c_val"],
            "reso": tier_data["r_val"],
            "metrics": tier_data["metrics"]
        })
                    
    print("\nRendering and metric calculation complete.")
    
    # 3. Save JSON results dump
    json_path = out_dir / "measurement_results.json"
    with open(json_path, "w") as f:
        json.dump(results, f, indent=2)
    print(f"Saved JSON results to {json_path}")
    
    # 4. Generate summary heatmaps
    print("Generating summary heatmaps...")
    plot_heatmap(
        np.array(saw_esr_matrix), saw_conds, ["Low", "High", "Ultra"],
        "Saw - ESR (dB) vs SPICE", out_dir / "heatmap_saw_esr.png",
        fmt=".1f", cmap="magma_r"
    )
    plot_heatmap(
        np.array(saw_lsd_matrix), saw_conds, ["Low", "High", "Ultra"],
        "Saw - LSD (dB) vs SPICE", out_dir / "heatmap_saw_lsd.png",
        fmt=".2f", cmap="plasma_r"
    )
    plot_heatmap(
        np.array(saw_corr_matrix), saw_conds, ["Low", "High", "Ultra"],
        "Saw - dB Spectrum Correlation", out_dir / "heatmap_saw_correlation.png",
        fmt=".3f", cmap="viridis"
    )
    plot_heatmap(
        np.array(saw_centroid_err_matrix), saw_conds, ["Low", "High", "Ultra"],
        "Saw - Centroid Error (%) vs SPICE", out_dir / "heatmap_saw_centroid_error.png",
        fmt=".1f", cmap="coolwarm"
    )
    plot_heatmap(
        np.array(saw_rms_delta_matrix), saw_conds, ["Low", "High", "Ultra"],
        "Saw - RMS Level Delta (dB) vs SPICE", out_dir / "heatmap_saw_rms_delta.png",
        fmt=".1f", cmap="coolwarm"
    )
    
    # Square heatmaps vs Low tier
    plot_heatmap(
        np.array(sq_lsd_matrix), sq_conds, ["High", "Ultra"],
        "Square - LSD (dB) vs Low tier", out_dir / "heatmap_square_lsd.png",
        fmt=".2f", cmap="plasma_r"
    )
    plot_heatmap(
        np.array(sq_corr_matrix), sq_conds, ["High", "Ultra"],
        "Square - Correlation vs Low tier", out_dir / "heatmap_square_correlation.png",
        fmt=".3f", cmap="viridis"
    )
    print("Heatmaps saved.")
    
    # 5. Format text table report
    report_lines = []
    report_lines.append("# Vibe303 Tier-vs-SPICE Evaluation Report\n")
    report_lines.append("## Summary Tables\n")
    
    # Saw Table
    report_lines.append("### SAW WAVEFORM METRICS (vs SPICE reference)\n")
    report_lines.append(
        f"{'Condition':<25} | {'Metric':<18} | {'Low (TPT)':<12} | {'High (Nodal 2x)':<16} | {'Ultra (Nodal 4x)':<16}"
    )
    report_lines.append("-" * 97)
    
    saw_metrics = [
        ("ESR (dB)", "esr_db", ".1f"),
        ("LSD (dB)", "lsd_db", ".2f"),
        ("Corr", "corr", ".3f"),
        ("Centroid Err (%)", "centroid_err_pct", ".1f"),
        ("RMS Delta (dB)", "rms_delta_db", ".1f")
    ]
    
    for r in results:
        if r["waveform"] != "Saw":
            continue
        cond = r["condition"]
        first = True
        for label, key, fmt in saw_metrics:
            low_val = r["metrics"]["Low"][key]
            high_val = r["metrics"]["High"][key]
            ultra_val = r["metrics"]["Ultra"][key]
            
            c_str = cond if first else ""
            report_lines.append(
                f"{c_str:<25} | {label:<18} | {low_val:{fmt[1:]}}         | {high_val:{fmt[1:]}}             | {ultra_val:{fmt[1:]}}"
            )
            first = False
        report_lines.append("-" * 97)
        
    report_lines.append("\n")
    
    # Square Table
    report_lines.append("### SQUARE WAVEFORM METRICS (absolute + vs Low tier)\n")
    report_lines.append(
        f"{'Condition':<25} | {'Metric':<22} | {'Low (TPT)':<12} | {'High (Nodal 2x)':<16} | {'Ultra (Nodal 4x)':<16}"
    )
    report_lines.append("-" * 101)
    
    sq_metrics = [
        ("Centroid (Hz)", "centroid_hz", ".0f"),
        ("RMS (dBFS)", "rms_dbfs", ".1f"),
        ("Crest (dB)", "crest_db", ".1f"),
        ("LSD vs Low (dB)", "lsd_vs_low_db", ".2f"),
        ("Corr vs Low", "corr_vs_low", ".3f")
    ]
    
    for r in results:
        if r["waveform"] != "Square":
            continue
        cond = r["condition"]
        first = True
        for label, key, fmt in sq_metrics:
            low_val = r["metrics"]["Low"].get(key, 0.0)
            high_val = r["metrics"]["High"].get(key, 0.0)
            ultra_val = r["metrics"]["Ultra"].get(key, 0.0)
            
            l_str = f"{low_val:{fmt[1:]}}" if key in r["metrics"]["Low"] else "N/A"
            h_str = f"{high_val:{fmt[1:]}}" if key in r["metrics"]["High"] else "N/A"
            u_str = f"{ultra_val:{fmt[1:]}}" if key in r["metrics"]["Ultra"] else "N/A"
            
            c_str = cond if first else ""
            report_lines.append(
                f"{c_str:<25} | {label:<22} | {l_str:<12} | {h_str:<16} | {u_str:<16}"
            )
            first = False
        report_lines.append("-" * 101)
        
    report_content = "\n".join(report_lines)
    
    # Print table to stdout
    print(report_content)
    
    # Save table report to .txt file
    report_path = out_dir / "measurement_report.txt"
    report_path.write_text(report_content)
    print(f"Saved table report to {report_path}")
    print("\nEvaluation successfully completed.")


if __name__ == "__main__":
    main()
