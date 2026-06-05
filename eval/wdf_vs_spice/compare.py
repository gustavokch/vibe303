"""
Evaluate WDF model accuracy vs SPICE reference.

Reads the pre-rendered SPICE validation set (dry + wet WAVs) and runs the
WDF binary on the matching dry files.  Produces frequency-response plots,
an ESR-vs-level table, a bias-tracking gap plot, and a summary.json.

Usage:
    uv run python -m eval.wdf_vs_spice.compare \\
        [--spice-set /path/to/ba283_spice] \\
        [--bias 0.5] \\
        [--rgain 1200] \\
        [--out eval/wdf_vs_spice/out]

Key structural caveats:
  1. The WDF models only the TR1 input stage (one Ebers-Moll BJT) plus a
     fake one-pole IIR transformer LPF (alpha=0.6).  The SPICE reference has
     TR1+TR2+TR3 plus a linear LO1166 transformer.  Transfer functions differ
     by construction.
  2. The WDF has no bias control (no RV1 pot).  It uses a fixed DC operating
     point regardless of the --bias value chosen for the SPICE wet files.
  All metrics are therefore measures of shape similarity after best-fit gain
  normalization, not absolute-accuracy claims.
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

# Stimuli on which the WDF binary crashed (SIGSEGV / non-zero exit).
WDF_CRASHES: list[str] = []

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

SR = 96_000
OUTPUT_REF_V = 64.0  # V – same constant as train/dataset.py

# All sweep levels present in the dataset (dBFS magnitude)
SWEEP_LEVELS = [1, 2, 3, 4, 6, 8, 10, 13, 16, 20, 24]

# For the fake LPF (alpha=0.6) the -3 dB point changes with SR:
# f_c = sr * arccos(1 - alpha^2 / (2*(1-alpha))) / (2*pi)
# At 96k: ~13.9 kHz; at 44.1k: ~6.4 kHz.  Note this in the README.
_LPF_ALPHA = 0.6
_LPF_FC_96K = SR * np.arccos(1.0 - _LPF_ALPHA**2 / (2 * (1 - _LPF_ALPHA))) / (2 * np.pi)

# Welch FFT window for FR analysis (0.042 s = 4096 samples at 96k)
_NPERSEG = 4096


# ---------------------------------------------------------------------------
# Signal utilities
# ---------------------------------------------------------------------------


def load_wav(path: Path) -> np.ndarray:
    """Load a WAV as mono float32, assert 96 kHz."""
    x, sr = sf.read(path, dtype="float32")
    if sr != SR:
        raise ValueError(f"Expected {SR} Hz, got {sr} Hz: {path}")
    if x.ndim > 1:
        x = x[:, 0]
    return np.asarray(x, dtype=np.float32)


def try_run_wdf(dry_path: Path, rgain: float) -> np.ndarray | None:
    """Run the WDF binary, tolerating crashes.

    The latest WDF source segfaults on some inputs (and exhibits non-deterministic
    behaviour across builds — likely uninitialized-memory UB).  On any failure we
    record the stimulus in ``WDF_CRASHES`` and return None so the caller can skip
    it instead of aborting the whole evaluation.
    """
    try:
        out, _ = run_wdf(dry_path, rgain=rgain)
        if not np.all(np.isfinite(out)):
            print(f"  [WDF non-finite output] {dry_path.name}")
            WDF_CRASHES.append(dry_path.name)
            return None
        return out
    except (RuntimeError, Exception) as exc:  # noqa: BLE001 - binary crash is opaque
        print(f"  [WDF CRASH] {dry_path.name}: {str(exc).splitlines()[0]}")
        WDF_CRASHES.append(dry_path.name)
        return None


def find_lag(a: np.ndarray, b: np.ndarray, max_lag_s: float = 0.2) -> int:
    """Return lag d such that a[d:] best aligns with b (d>0: a leads b).

    Uses 2 s of signal around the start to find the delay.
    """
    window = min(len(a), len(b), 2 * SR)
    a_w = a[:window].astype(np.float64)
    b_w = b[:window].astype(np.float64)
    # Zero-mean + unit-std normalisation removes the gain difference.
    a_w = (a_w - a_w.mean()) / (a_w.std() + 1e-12)
    b_w = (b_w - b_w.mean()) / (b_w.std() + 1e-12)
    xcorr = np.correlate(a_w, b_w, mode="full")  # length 2*window-1
    max_lag = int(max_lag_s * SR)
    center = window - 1
    lo, hi = max(0, center - max_lag), min(len(xcorr), center + max_lag + 1)
    rel_peak = np.argmax(xcorr[lo:hi])
    lag = (lo + rel_peak) - center  # positive: a leads b
    return int(lag)


def align_pair(
    wdf_out: np.ndarray, spice_wet: np.ndarray
) -> tuple[np.ndarray, np.ndarray]:
    """Trim wdf_out and spice_wet to common aligned length."""
    d = find_lag(wdf_out, spice_wet)
    if d > 0:
        wdf_a, spice_a = wdf_out[d:], spice_wet
    elif d < 0:
        wdf_a, spice_a = wdf_out, spice_wet[-d:]
    else:
        wdf_a, spice_a = wdf_out, spice_wet
    n = min(len(wdf_a), len(spice_a))
    return wdf_a[:n].copy(), spice_a[:n].copy()


def fit_gain(wdf: np.ndarray, spice: np.ndarray) -> float:
    """Least-squares gain scalar g: min ||g*wdf - spice||^2.

    Returns g in volts/unit  (converts WDF's arbitrary-unit output to volts).
    """
    denom = float(np.dot(wdf.astype(np.float64), wdf.astype(np.float64)))
    numer = float(np.dot(wdf.astype(np.float64), spice.astype(np.float64)))
    if abs(denom) < 1e-30:
        return 1.0
    return numer / denom


def compute_esr(wdf_scaled: np.ndarray, spice: np.ndarray) -> float:
    """ESR = 10*log10(||error||^2 / ||reference||^2) in dB.

    −∞ dB = perfect match, 0 dB = error has same energy as reference.
    """
    error = wdf_scaled.astype(np.float64) - spice.astype(np.float64)
    ref_pow = np.dot(spice.astype(np.float64), spice.astype(np.float64))
    if ref_pow < 1e-30:
        return float("nan")
    esr = np.dot(error, error) / ref_pow
    return float(10.0 * np.log10(esr + 1e-30))


def is_clipping(wdf_out: np.ndarray, threshold: float = 0.99) -> bool:
    """True if any WDF output sample has |value| >= threshold."""
    return bool(np.any(np.abs(wdf_out) >= threshold))


# ---------------------------------------------------------------------------
# Frequency-response analysis
# ---------------------------------------------------------------------------


def compute_fr(
    dry: np.ndarray,
    wet_spice: np.ndarray,
    wet_wdf: np.ndarray,
    nperseg: int = _NPERSEG,
) -> dict:
    """Compute BLA (Best Linear Approximation) transfer functions for both models.

    Uses Welch's cross-spectral density:  H(f) = Cxy(f) / Sxx(f)

    Returns a dict with keys: freq, H_spice, H_wdf,
    mag_spice_db, mag_wdf_db, phase_spice_deg, phase_wdf_deg.
    """
    # Truncate to the shortest signal.
    n = min(len(dry), len(wet_spice), len(wet_wdf))
    x = dry[:n].astype(np.float64)
    y_sp = wet_spice[:n].astype(np.float64)
    y_wdf = wet_wdf[:n].astype(np.float64)

    kw = dict(fs=SR, nperseg=nperseg, noverlap=nperseg // 2)
    freq, Pxx = welch(x, **kw)
    _, Pxy_sp = csd(x, y_sp, **kw)
    _, Pxy_wdf = csd(x, y_wdf, **kw)

    # Regularise by the input PSD to avoid division by noise floor.
    reg = np.max(Pxx) * 1e-6
    H_sp = Pxy_sp / (Pxx + reg)
    H_wdf = Pxy_wdf / (Pxx + reg)

    # Normalise magnitudes to 0 dB at 1 kHz for shape comparison.
    idx_1k = int(np.argmin(np.abs(freq - 1000.0)))
    ref_sp = np.abs(H_sp[idx_1k]) + 1e-12
    ref_wdf = np.abs(H_wdf[idx_1k]) + 1e-12

    mag_sp = 20.0 * np.log10(np.abs(H_sp) / ref_sp + 1e-12)
    mag_wdf = 20.0 * np.log10(np.abs(H_wdf) / ref_wdf + 1e-12)

    return {
        "freq": freq,
        "H_spice": H_sp,
        "H_wdf": H_wdf,
        "mag_spice_db": mag_sp,
        "mag_wdf_db": mag_wdf,
        "phase_spice_deg": np.angle(H_sp, deg=True),
        "phase_wdf_deg": np.angle(H_wdf, deg=True),
    }


# ---------------------------------------------------------------------------
# Bias-tracking gap
# ---------------------------------------------------------------------------


def run_bias_tracking(
    spice_set: Path,
    rgain: float,
    g: float,
    out_dir: Path,
) -> dict:
    """Compare SPICE wet at all 5 biases vs WDF (no-bias) on sweep_m6.

    Returns: {bias: {"spice_rms_v": ..., "esr_db": ...}, "wdf_rms_v": ...}
    """
    dry_path = spice_set / "dry" / "sweep_m6_dry.wav"
    if not dry_path.exists():
        print(f"[bias_tracking] {dry_path} not found, skipping.")
        return {}

    biases = [0.0, 0.25, 0.5, 0.75, 1.0]
    spice_rms: dict[float, float] = {}

    for b in biases:
        wet_path = spice_set / "wet" / f"sweep_m6_bias{b:.2f}.wav"
        if not wet_path.exists():
            continue
        wet = load_wav(wet_path)
        spice_rms[b] = float(np.sqrt(np.mean(wet.astype(np.float64) ** 2)))

    wdf_out = try_run_wdf(dry_path, rgain=rgain)
    if wdf_out is None:
        print("[bias_tracking] WDF crashed on sweep_m6, skipping.")
        return {}
    wdf_scaled = wdf_out * g
    wdf_rms = float(np.sqrt(np.mean(wdf_scaled.astype(np.float64) ** 2)))

    # Align and compute ESR for bias=0.5.
    wet_05 = load_wav(spice_set / "wet" / "sweep_m6_bias0.50.wav")
    wdf_a, _ = align_pair(wdf_out, wet_05)
    esr_per_bias: dict[float, float] = {}
    for b in biases:
        wet_path = spice_set / "wet" / f"sweep_m6_bias{b:.2f}.wav"
        if not wet_path.exists():
            continue
        wet_b = load_wav(wet_path)
        _, sp_a = align_pair(wdf_out, wet_b)
        n = min(len(wdf_a), len(sp_a))
        esr_per_bias[b] = compute_esr(wdf_a[:n] * g, sp_a[:n])

    # Plot
    fig, axes = plt.subplots(1, 2, figsize=(12, 4))
    ax0, ax1 = axes

    bias_vals = sorted(spice_rms.keys())
    ax0.plot(bias_vals, [spice_rms[b] for b in bias_vals], "o-", label="SPICE")
    ax0.axhline(wdf_rms, color="r", linestyle="--", label=f"WDF (no bias, Rgain={rgain:.0f}Ω)")
    ax0.set_xlabel("Bias (RV1 wiper, 0–1)")
    ax0.set_ylabel("Output RMS (V, SPICE) or scaled (WDF)")
    ax0.set_title("Bias-tracking gap — sweep_m6")
    ax0.legend()
    ax0.grid(True, alpha=0.4)

    ax1.plot(
        sorted(esr_per_bias.keys()),
        [esr_per_bias[b] for b in sorted(esr_per_bias.keys())],
        "s-",
        color="darkorange",
    )
    ax1.axhline(0, color="k", linestyle=":", alpha=0.5)
    ax1.set_xlabel("Bias")
    ax1.set_ylabel("ESR (dB)  [lower = better match]")
    ax1.set_title("WDF null depth vs. SPICE @ each bias")
    ax1.grid(True, alpha=0.4)

    plt.tight_layout()
    fig.savefig(out_dir / "bias_tracking.png", dpi=150)
    plt.close(fig)
    print(f"[bias_tracking] saved bias_tracking.png")

    return {
        "spice_rms_per_bias": {str(k): v for k, v in spice_rms.items()},
        "wdf_rms_scaled": wdf_rms,
        "esr_per_bias_db": {str(k): v for k, v in esr_per_bias.items()},
    }


# ---------------------------------------------------------------------------
# Level ladder — sweep_m* at a fixed bias
# ---------------------------------------------------------------------------


def run_level_ladder(
    spice_set: Path,
    bias: float,
    rgain: float,
    g: float,
    out_dir: Path,
) -> dict:
    """Run each sweep level (m1..m24) through WDF and compare to SPICE.

    Produces:
        level_ladder.png   – SPICE RMS and WDF*g RMS vs input level
        esr_vs_level.png   – ESR (dB) vs input level

    Returns dict with per-level metrics.
    """
    bias_str = f"{bias:.2f}"
    results = {}

    for lvl in SWEEP_LEVELS:
        dry_path = spice_set / "dry" / f"sweep_m{lvl}_dry.wav"
        wet_path = spice_set / "wet" / f"sweep_m{lvl}_bias{bias_str}.wav"
        if not dry_path.exists() or not wet_path.exists():
            print(f"[level_ladder] skipping m{lvl} (files missing)")
            continue

        print(f"[level_ladder] processing sweep_m{lvl}...")
        wet = load_wav(wet_path)
        wdf_out = try_run_wdf(dry_path, rgain=rgain)
        if wdf_out is None:
            results[lvl] = {
                "esr_db": None,
                "spice_rms_v": round(float(np.sqrt(np.mean(wet.astype(np.float64) ** 2))), 4),
                "wdf_rms_scaled_v": None,
                "wdf_clipping": None,
                "wdf_crashed": True,
            }
            continue

        clipping = is_clipping(wdf_out)
        wdf_a, spice_a = align_pair(wdf_out, wet)
        wdf_scaled = wdf_a * g
        esr = compute_esr(wdf_scaled, spice_a)

        spice_rms = float(np.sqrt(np.mean(spice_a.astype(np.float64) ** 2)))
        wdf_rms_scaled = float(np.sqrt(np.mean(wdf_scaled.astype(np.float64) ** 2)))

        results[lvl] = {
            "esr_db": round(esr, 2),
            "spice_rms_v": round(spice_rms, 4),
            "wdf_rms_scaled_v": round(wdf_rms_scaled, 4),
            "wdf_clipping": clipping,
        }
        clip_flag = " [CLIPPING]" if clipping else ""
        print(
            f"  m{lvl:2d}: ESR={esr:+.1f} dB  "
            f"spice_rms={spice_rms:.3f}V  wdf_rms={wdf_rms_scaled:.3f}V{clip_flag}"
        )

    if not results:
        return {}

    levels = sorted(results.keys())
    # Levels where the WDF produced usable output (did not crash).
    ok = [l for l in levels if results[l].get("esr_db") is not None]

    # --- Plot 1: RMS vs level ---
    fig, ax = plt.subplots(figsize=(8, 5))
    ax.plot(
        levels,
        [results[l]["spice_rms_v"] for l in levels],
        "o-",
        label="SPICE (raw V)",
    )
    if ok:
        ax.plot(
            ok,
            [results[l]["wdf_rms_scaled_v"] for l in ok],
            "s--",
            label=f"WDF × g (Rgain={rgain:.0f}Ω)",
        )
    for l in levels:
        if results[l].get("wdf_clipping"):
            ax.axvline(l, color="red", alpha=0.3, linewidth=1)
        if results[l].get("wdf_crashed"):
            ax.axvline(l, color="purple", alpha=0.4, linewidth=2, linestyle=":")
    ax.set_xlabel("Input level (dBFS magnitude)")
    ax.set_ylabel("Output RMS (V equivalent)")
    ax.set_title(f"Output level tracking — sweep ladder, bias={bias:.2f}")
    ax.legend()
    ax.grid(True, alpha=0.4)
    ax.invert_xaxis()  # m1 (hot) on left, m24 (quiet) on right
    ax.set_xticks(levels)
    ax.set_xticklabels([f"−{l}" for l in levels])
    note = "Red: WDF clipping  ·  Purple dotted: WDF crashed"
    ax.text(0.99, 0.02, note, transform=ax.transAxes, ha="right", fontsize=8, color="red")
    plt.tight_layout()
    fig.savefig(out_dir / "level_ladder.png", dpi=150)
    plt.close(fig)

    # --- Plot 2: ESR vs level (non-crashed only) ---
    if ok:
        fig, ax = plt.subplots(figsize=(8, 5))
        esrs = [results[l]["esr_db"] for l in ok]
        colors = ["red" if results[l]["wdf_clipping"] else "steelblue" for l in ok]
        ax.bar([str(l) for l in ok], esrs, color=colors)
        ax.axhline(0, color="k", linewidth=1, linestyle=":")
        ax.set_xlabel("Input level (dBFS magnitude)")
        ax.set_ylabel("ESR (dB)  [lower = better match]")
        ax.set_title(f"Null depth (ESR) vs level — WDF vs SPICE, bias={bias:.2f}")
        ax.set_xticks(range(len(ok)))
        ax.set_xticklabels([f"−{l}" for l in ok])
        ax.grid(True, axis="y", alpha=0.4)
        note = "Red bars: WDF clipping"
        ax.text(0.99, 0.01, note, transform=ax.transAxes, ha="right", fontsize=8, color="red")
        plt.tight_layout()
        fig.savefig(out_dir / "esr_vs_level.png", dpi=150)
        plt.close(fig)

    print(f"[level_ladder] saved level_ladder.png and esr_vs_level.png")
    return results


# ---------------------------------------------------------------------------
# Frequency-response plots
# ---------------------------------------------------------------------------


def run_fr_analysis(
    spice_set: Path,
    bias: float,
    rgain: float,
    g: float,
    out_dir: Path,
) -> dict:
    """Compute and plot frequency-response comparison for sweep_m24 (most linear).

    Produces: fr_magnitude.png, fr_phase.png
    Returns dict with max_delta_mag_db and hf_delta_at_15kHz_db.
    """
    # Use the lowest-level sweep for linear FR (m24 = -24 dBFS, peak≈0.032V)
    bias_str = f"{bias:.2f}"
    dry_path = spice_set / "dry" / "sweep_m24_dry.wav"
    wet_sp_path = spice_set / "wet" / f"sweep_m24_bias{bias_str}.wav"

    if not dry_path.exists() or not wet_sp_path.exists():
        print("[fr_analysis] sweep_m24 files missing, trying sweep_m20...")
        dry_path = spice_set / "dry" / "sweep_m20_dry.wav"
        wet_sp_path = spice_set / "wet" / f"sweep_m20_bias{bias_str}.wav"

    if not dry_path.exists() or not wet_sp_path.exists():
        print("[fr_analysis] no suitable low-level sweep found, skipping FR plots.")
        return {}

    print(f"[fr_analysis] loading {dry_path.name} and {wet_sp_path.name}...")
    dry = load_wav(dry_path)
    wet_sp = load_wav(wet_sp_path)

    print("[fr_analysis] running WDF...")
    wdf_out = try_run_wdf(dry_path, rgain=rgain)
    if wdf_out is None:
        print("[fr_analysis] WDF crashed on the FR stimulus, skipping FR plots.")
        return {}

    # Align WDF to match SPICE wet timing.
    wdf_a, spice_a = align_pair(wdf_out, wet_sp)
    n = min(len(dry), len(wdf_a), len(spice_a))
    dry_t = dry[:n]
    wdf_t = wdf_a[:n] * g   # scale to same volt domain as SPICE
    sp_t = spice_a[:n]

    fr = compute_fr(dry_t, sp_t, wdf_t)
    freq = fr["freq"]

    # Restrict to 20 Hz – 22 kHz for plots
    mask = (freq >= 20) & (freq <= 22_000)

    # --- Plot: magnitude ---
    fig, ax = plt.subplots(figsize=(10, 5))
    ax.semilogx(freq[mask], fr["mag_spice_db"][mask], label="SPICE (full chain)", color="steelblue")
    ax.semilogx(freq[mask], fr["mag_wdf_db"][mask], label=f"WDF (TR1 only, Rgain={rgain:.0f}Ω)",
                color="darkorange", linestyle="--")
    ax.axvline(_LPF_FC_96K, color="gray", linestyle=":", alpha=0.7,
               label=f"WDF fake LPF f_c ≈ {_LPF_FC_96K/1000:.1f} kHz @ 96k")
    ax.set_xlabel("Frequency (Hz)")
    ax.set_ylabel("Magnitude (dB, normalised at 1 kHz)")
    ax.set_title(f"Frequency Response — sweep_m24, bias={bias:.2f}")
    ax.legend(fontsize=9)
    ax.grid(True, which="both", alpha=0.4)
    ax.set_xlim(20, 22_000)
    plt.tight_layout()
    fig.savefig(out_dir / "fr_magnitude.png", dpi=150)
    plt.close(fig)

    # --- Plot: ΔMagnitude (shape error) ---
    delta_mag = fr["mag_spice_db"] - fr["mag_wdf_db"]
    fig, ax = plt.subplots(figsize=(10, 4))
    ax.semilogx(freq[mask], delta_mag[mask], color="crimson")
    ax.axhline(0, color="k", linewidth=0.8, linestyle="--")
    ax.axvline(_LPF_FC_96K, color="gray", linestyle=":", alpha=0.7,
               label=f"WDF LPF f_c ≈ {_LPF_FC_96K/1000:.1f} kHz")
    ax.set_xlabel("Frequency (Hz)")
    ax.set_ylabel("SPICE − WDF (dB)")
    ax.set_title("Frequency response shape error (SPICE − WDF)")
    ax.legend(fontsize=9)
    ax.grid(True, which="both", alpha=0.4)
    ax.set_xlim(20, 22_000)
    plt.tight_layout()
    fig.savefig(out_dir / "fr_delta.png", dpi=150)
    plt.close(fig)

    # --- Plot: phase ---
    fig, ax = plt.subplots(figsize=(10, 4))
    ax.semilogx(freq[mask], fr["phase_spice_deg"][mask], label="SPICE", color="steelblue")
    ax.semilogx(freq[mask], fr["phase_wdf_deg"][mask], label=f"WDF",
                color="darkorange", linestyle="--")
    ax.set_xlabel("Frequency (Hz)")
    ax.set_ylabel("Phase (degrees)")
    ax.set_title(f"Phase response — sweep_m24, bias={bias:.2f}")
    ax.legend(fontsize=9)
    ax.grid(True, which="both", alpha=0.4)
    ax.set_xlim(20, 22_000)
    plt.tight_layout()
    fig.savefig(out_dir / "fr_phase.png", dpi=150)
    plt.close(fig)

    print(f"[fr_analysis] saved fr_magnitude.png, fr_delta.png, fr_phase.png")

    # Compute headline numbers
    band = (freq >= 20) & (freq <= 20_000)
    max_delta_db = float(np.max(np.abs(delta_mag[band])))
    idx_15k = int(np.argmin(np.abs(freq - 15_000)))
    delta_at_15k = float(delta_mag[idx_15k])

    return {
        "max_shape_error_db": round(max_delta_db, 2),
        "shape_error_at_15kHz_db": round(delta_at_15k, 2),
        "wdf_fake_lpf_fc_hz": round(_LPF_FC_96K, 0),
    }


# ---------------------------------------------------------------------------
# Waveform overlay
# ---------------------------------------------------------------------------


def plot_waveform_overlay(
    spice_a: np.ndarray,
    wdf_scaled: np.ndarray,
    label: str,
    out_dir: Path,
    segment_s: float = 0.05,
    start_s: float = 2.0,
) -> None:
    """Plot a short segment of spice vs scaled WDF output."""
    start = int(start_s * SR)
    n = int(segment_s * SR)
    t = np.arange(n) / SR * 1000  # ms

    s_seg = spice_a[start : start + n]
    w_seg = wdf_scaled[start : start + n]

    fig, axes = plt.subplots(2, 1, figsize=(10, 6), sharex=True)

    axes[0].plot(t, s_seg, label="SPICE", color="steelblue", linewidth=0.8)
    axes[0].plot(t, w_seg, label="WDF (scaled)", color="darkorange",
                 linestyle="--", linewidth=0.8)
    axes[0].set_ylabel("Amplitude (V equiv.)")
    axes[0].set_title(f"Time-domain overlay — {label}")
    axes[0].legend(fontsize=9)
    axes[0].grid(True, alpha=0.4)

    axes[1].plot(t, s_seg - w_seg, color="crimson", linewidth=0.8)
    axes[1].set_ylabel("Error (V equiv.)")
    axes[1].set_xlabel("Time (ms)")
    axes[1].set_title("Residual (SPICE − WDF)")
    axes[1].grid(True, alpha=0.4)

    plt.tight_layout()
    fname = f"waveform_overlay_{label.replace(' ', '_')}.png"
    fig.savefig(out_dir / fname, dpi=150)
    plt.close(fig)
    print(f"[overlay] saved {fname}")


# ---------------------------------------------------------------------------
# Val-set ESR table
# ---------------------------------------------------------------------------


def run_val_esr(
    spice_set: Path,
    bias: float,
    rgain: float,
    g: float,
    out_dir: Path,
) -> list[dict]:
    """Compute ESR for every val.csv pair at the target bias.

    Returns a list of dicts with per-pair metrics.
    """
    val_csv = spice_set / "val.csv"
    if not val_csv.exists():
        print("[val_esr] val.csv not found, skipping.")
        return []

    df = pd.read_csv(val_csv)
    df = df[np.isclose(df["bias_value"].astype(float), bias, atol=0.01)].copy()
    df["dry_base"] = df["dry_path"].apply(lambda p: Path(p).name)
    df["wet_base"] = df["wet_path"].apply(lambda p: Path(p).name)

    records = []
    for _, row in df.iterrows():
        dry_path = spice_set / "dry" / row["dry_base"]
        wet_path = spice_set / "wet" / row["wet_base"]
        if not dry_path.exists() or not wet_path.exists():
            print(f"[val_esr] missing file(s) for {row['dry_base']}, skipping")
            continue

        print(f"[val_esr] {row['dry_base']} (bias={bias:.2f}, kind={row['stim_kind']})...")
        wet = load_wav(wet_path)
        wdf_out = try_run_wdf(dry_path, rgain=rgain)
        if wdf_out is None:
            records.append({
                "dry": row["dry_base"],
                "wet": row["wet_base"],
                "stim_kind": row["stim_kind"],
                "bias": float(row["bias_value"]),
                "esr_db": None,
                "wdf_clipping": None,
                "wdf_crashed": True,
            })
            continue
        clipping = is_clipping(wdf_out)
        wdf_a, spice_a = align_pair(wdf_out, wet)
        wdf_scaled = wdf_a * g
        esr = compute_esr(wdf_scaled, spice_a)

        record = {
            "dry": row["dry_base"],
            "wet": row["wet_base"],
            "stim_kind": row["stim_kind"],
            "bias": float(row["bias_value"]),
            "esr_db": round(esr, 2),
            "wdf_clipping": clipping,
            "wdf_crashed": False,
        }
        records.append(record)
        print(f"  ESR = {esr:+.1f} dB{'  [CLIPPING]' if clipping else ''}")

    if records:
        # Save table as CSV
        pd.DataFrame(records).to_csv(out_dir / "val_esr_table.csv", index=False)

        # Plot ESR bar chart (non-crashed pairs only)
        ok_records = [r for r in records if not r.get("wdf_crashed")]
        if ok_records:
            fig, ax = plt.subplots(figsize=(max(8, len(ok_records) * 0.6), 5))
            labels = [r["dry"].replace("_dry.wav", "")[:30] for r in ok_records]
            esrs = [r["esr_db"] for r in ok_records]
            cols = ["red" if r["wdf_clipping"] else "steelblue" for r in ok_records]
            ax.bar(range(len(ok_records)), esrs, color=cols)
            ax.set_xticks(range(len(ok_records)))
            ax.set_xticklabels(labels, rotation=45, ha="right", fontsize=7)
            ax.axhline(0, color="k", linestyle=":", linewidth=0.8)
            ax.set_ylabel("ESR (dB)  [lower = better match]")
            ax.set_title(f"Val-set null depth — WDF vs SPICE (bias={bias:.2f})")
            ax.grid(True, axis="y", alpha=0.4)
            plt.tight_layout()
            fig.savefig(out_dir / "val_esr_bar.png", dpi=150)
            plt.close(fig)
        print(f"[val_esr] saved val_esr_table.csv" + ("" if not ok_records else " and val_esr_bar.png"))

    return records


# ---------------------------------------------------------------------------
# Main entry point
# ---------------------------------------------------------------------------


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument(
        "--spice-set",
        type=Path,
        default=Path.home() / "GoogleDrive/ba283_spice",
        help="Root directory of the pre-rendered SPICE dataset (dry/, wet/, val.csv).",
    )
    ap.add_argument("--bias", type=float, default=0.5, help="SPICE bias value to use (default 0.5).")
    ap.add_argument(
        "--rgain",
        type=float,
        default=1200.0,
        help="WDF gain resistor in ohms (default 1200).",
    )
    ap.add_argument(
        "--out",
        type=Path,
        default=Path("eval/wdf_vs_spice/out"),
        help="Output directory for plots and summary.json.",
    )
    args = ap.parse_args()

    args.out.mkdir(parents=True, exist_ok=True)
    build_if_needed()

    print("=" * 60)
    print(f"WDF vs SPICE accuracy evaluation")
    print(f"  SPICE set : {args.spice_set}")
    print(f"  bias      : {args.bias}")
    print(f"  Rgain     : {args.rgain} Ω")
    print(f"  WDF LPF   : {_LPF_FC_96K/1000:.1f} kHz @ 96k SR")
    print("=" * 60)

    # ----------------------------------------------------------------
    # Step 1: Fit the gain scalar on the lowest-level linear sweep.
    # ----------------------------------------------------------------
    bias_str = f"{args.bias:.2f}"
    ref_dry = args.spice_set / "dry" / "sweep_m24_dry.wav"
    ref_wet = args.spice_set / "wet" / f"sweep_m24_bias{bias_str}.wav"

    if not ref_dry.exists() or not ref_wet.exists():
        # Fall back to m20
        ref_dry = args.spice_set / "dry" / "sweep_m20_dry.wav"
        ref_wet = args.spice_set / "wet" / f"sweep_m20_bias{bias_str}.wav"

    print(f"\n--- Gain calibration on {ref_dry.name} ---")
    ref_wet_wav = load_wav(ref_wet)
    wdf_ref = try_run_wdf(ref_dry, rgain=args.rgain)
    if wdf_ref is None:
        raise SystemExit(
            f"FATAL: WDF crashed on the gain-calibration stimulus ({ref_dry.name}). "
            "Cannot compute the gain scalar — the WDF binary is unusable on this input."
        )
    wdf_ref_a, spice_ref_a = align_pair(wdf_ref, ref_wet_wav)
    g = fit_gain(wdf_ref_a, spice_ref_a)
    print(f"  gain scalar g = {g:.4f} V/unit")

    # Sanity: ESR at the calibration pair should be best-case.
    esr_cal = compute_esr(wdf_ref_a * g, spice_ref_a)
    print(f"  ESR at calibration pair = {esr_cal:+.1f} dB")

    # ----------------------------------------------------------------
    # Step 2: Frequency-response analysis.
    # ----------------------------------------------------------------
    print("\n--- Frequency response analysis ---")
    fr_results = run_fr_analysis(args.spice_set, args.bias, args.rgain, g, args.out)

    # ----------------------------------------------------------------
    # Step 3: Level ladder — sweep_m* at the target bias.
    # ----------------------------------------------------------------
    print("\n--- Level ladder ---")
    ladder_results = run_level_ladder(args.spice_set, args.bias, args.rgain, g, args.out)

    # ----------------------------------------------------------------
    # Step 4: Waveform overlay for a mid-level sweep.
    # ----------------------------------------------------------------
    mid_dry = args.spice_set / "dry" / "sweep_m6_dry.wav"
    mid_wet = args.spice_set / "wet" / f"sweep_m6_bias{bias_str}.wav"
    if mid_dry.exists() and mid_wet.exists():
        print("\n--- Waveform overlay (sweep_m6) ---")
        mid_wdf = try_run_wdf(mid_dry, rgain=args.rgain)
        if mid_wdf is not None:
            mid_wdf_a, mid_spice_a = align_pair(mid_wdf, load_wav(mid_wet))
            plot_waveform_overlay(
                mid_spice_a,
                mid_wdf_a * g,
                label="sweep_m6",
                out_dir=args.out,
            )
        else:
            print("[overlay] WDF crashed on sweep_m6, skipping overlay.")

    # ----------------------------------------------------------------
    # Step 5: Bias-tracking gap.
    # ----------------------------------------------------------------
    print("\n--- Bias-tracking gap ---")
    bias_results = run_bias_tracking(args.spice_set, args.rgain, g, args.out)

    # ----------------------------------------------------------------
    # Step 6: Val-set ESR table.
    # ----------------------------------------------------------------
    print("\n--- Val-set ESR ---")
    val_records = run_val_esr(args.spice_set, args.bias, args.rgain, g, args.out)

    # ----------------------------------------------------------------
    # Step 7: Aggregate summary.
    # ----------------------------------------------------------------
    usable = [r for r in val_records if not r.get("wdf_crashed")]
    esr_vals = [r["esr_db"] for r in usable if not r["wdf_clipping"]]
    clip_count = sum(1 for r in usable if r["wdf_clipping"])

    # Crashed stimuli (deduped, across all stages).
    crashed = sorted(set(WDF_CRASHES))
    ladder_crashed = sum(1 for v in ladder_results.values() if v.get("wdf_crashed"))
    val_crashed = sum(1 for r in val_records if r.get("wdf_crashed"))

    summary = {
        "config": {
            "bias": args.bias,
            "rgain_ohm": args.rgain,
            "wdf_fake_lpf_fc_hz": round(_LPF_FC_96K, 0),
        },
        "wdf_stability": {
            "crashed_stimuli": crashed,
            "n_crashed_unique": len(crashed),
            "ladder_levels_crashed": ladder_crashed,
            "val_pairs_crashed": val_crashed,
            "note": "Latest WDF source segfaults (SIGSEGV) on some inputs and is "
            "non-deterministic across builds (likely uninitialized-memory UB).",
        },
        "gain_calibration": {
            "g_volts_per_unit": round(g, 6),
            "esr_at_cal_pair_db": round(esr_cal, 2),
            "calibration_file": ref_dry.name,
        },
        "frequency_response": fr_results,
        "level_ladder": {str(k): v for k, v in ladder_results.items()},
        "val_set_esr": {
            "mean_esr_db": round(float(np.mean(esr_vals)), 2) if esr_vals else None,
            "median_esr_db": round(float(np.median(esr_vals)), 2) if esr_vals else None,
            "min_esr_db": round(float(np.min(esr_vals)), 2) if esr_vals else None,
            "clipping_pairs": clip_count,
            "crashed_pairs": val_crashed,
            "n_pairs": len(val_records),
        },
        "bias_tracking": bias_results,
        "caveats": [
            "WDF models TR1 (1 stage) + fake one-pole LPF transformer. "
            "SPICE has TR1+TR2+TR3 + linear LO1166 transformer. "
            "Transfer functions differ by construction.",
            "WDF has no bias control. SPICE bias dependence is not modeled.",
            f"WDF fake transformer LPF pole shifts with SR: "
            f"~{_LPF_FC_96K/1000:.1f} kHz @ 96k vs ~6.4 kHz @ 44.1k.",
            "Gain scalar g fitted on lowest-level sweep (linear region only).",
        ],
    }

    summary_path = args.out / "summary.json"
    summary_path.write_text(json.dumps(summary, indent=2))
    print(f"\n[summary] saved to {summary_path}")

    print("\n" + "=" * 60)
    print("SUMMARY")
    print("=" * 60)
    print(f"  Gain calibration ESR  : {esr_cal:+.1f} dB")
    if fr_results:
        print(f"  Max FR shape error    : {fr_results.get('max_shape_error_db', '?'):.1f} dB")
        print(f"  FR error @ 15 kHz     : {fr_results.get('shape_error_at_15kHz_db', '?'):.1f} dB")
    if esr_vals:
        print(f"  Val-set ESR (mean)    : {np.mean(esr_vals):+.1f} dB")
        print(f"  Val-set ESR (median)  : {np.median(esr_vals):+.1f} dB")
        print(f"  Clipping pairs        : {clip_count} / {len(val_records)}")
    if crashed:
        print(f"  WDF CRASHES           : {len(crashed)} stimuli "
              f"(ladder={ladder_crashed}, val={val_crashed})")
        for name in crashed:
            print(f"     - {name}")
    if bias_results and "esr_per_bias_db" in bias_results:
        esr_05 = bias_results["esr_per_bias_db"].get("0.5", "?")
        print(f"  WDF vs SPICE@bias=0.5 : {esr_05:+.1f} dB (sweep_m6)")
    print("=" * 60)
    print(f"Plots and table saved to: {args.out}")


if __name__ == "__main__":
    main()
