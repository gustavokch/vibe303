import streamlit as st
import subprocess
import tempfile
import sys
import numpy as np
import matplotlib.pyplot as plt
from pathlib import Path
from scipy.signal import hilbert

# Setup sys.path for local imports
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import voice_ref_spice as vr
from metrics import (
    pitch_tracking_error_cents,
    welch_pow,
    t60_decay_time,
    dc_offset_drift
)

# Setup path to Rust renderer
ROOT = Path(__file__).resolve().parent.parent.parent
RENDER303_REL = ROOT / "target" / "release" / "render303"
RENDER303_DBG = ROOT / "target" / "debug" / "render303"
RENDER303 = RENDER303_REL if RENDER303_REL.exists() else RENDER303_DBG

def render_voice(note: int, cutoff_hz: float, reso_knob: float, wave_idx: int, tier: str,
                 sr: float, secs: float) -> np.ndarray:
    with tempfile.TemporaryDirectory() as d:
        out = Path(d) / "r.f32"
        subprocess.run(
            [
                str(RENDER303),
                "--note", str(note),
                "--sr", str(sr),
                "--secs", str(secs),
                "--cutoff", str(cutoff_hz),
                "--reso", str(reso_knob * 100.0),
                "--wave", str(wave_idx),
                "--tier", tier.lower(),
                "--out", str(out)
            ],
            check=True, capture_output=True
        )
        if not out.exists():
            return np.zeros(int(secs * sr))
        return np.fromfile(out, dtype="<f4").astype(np.float64)

st.title("Analog Tier Voicing Harness")
st.sidebar.markdown("### Settings")
tier = st.sidebar.selectbox("Selected Tier", ["Draft", "Low", "High", "Best"])
selected_tier = tier.lower()

# Sidebar checkbox to toggle slow SPICE transient reference runs
show_spice = st.sidebar.checkbox("Show SPICE Reference", value=False)

st.header("1. VCO Analysis")
if st.button("Run VCO Evaluation"):
    st.write("Running baseline pitch tracking error measurement...")
    
    notes = list(range(36, 73, 6))
    sr = 44100.0
    secs = 0.2
    
    errors_selected = []
    errors_best = []
    errors_spice = []
    
    progress = st.progress(0.0)
    for idx, note in enumerate(notes):
        # selected tier
        sig_selected = render_voice(note, 1000.0, 0.0, 0, selected_tier, sr, secs)
        # reference (Best)
        sig_best = render_voice(note, 1000.0, 0.0, 0, "best", sr, secs)
        
        # Ideal freq
        ref_hz = 440.0 * (2.0 ** ((note - 69) / 12.0))
        
        err_sel = pitch_tracking_error_cents(ref_hz, sig_selected, sr)
        err_best = pitch_tracking_error_cents(ref_hz, sig_best, sr)
        
        errors_selected.append(err_sel)
        errors_best.append(err_best)
        
        if show_spice:
            # Measure SPICE VCO uncalibrated tracking error (no solve_cv, just raw circuit)
            cv_sp = vr.cv_for_freq(ref_hz)
            sig_spice = vr.render_vco(cv_sp, sr, secs)
            err_sp = pitch_tracking_error_cents(ref_hz, sig_spice, sr)
            errors_spice.append(err_sp)
            
        progress.progress((idx + 1) / len(notes))
        
    fig, ax = plt.subplots(figsize=(8, 4))
    ax.plot(notes, errors_selected, 'o-', label=f"{tier} Tier")
    ax.plot(notes, errors_best, 'x--', label="Best Tier (Reference)")
    if show_spice:
        ax.plot(notes, errors_spice, 's-.', label="SPICE VCO Reference (Uncalibrated)")
    ax.set_title("VCO Pitch Tracking Error (cents)")
    ax.set_xlabel("MIDI Note")
    ax.set_ylabel("Cents Deviation")
    ax.grid(True)
    ax.legend()
    st.pyplot(fig)
    
    st.write("Running 3-Octave Sweep (notes 36, 48, 60, 72) Spectrum Overlay...")
    fig, axes = plt.subplots(2, 2, figsize=(10, 8))
    axes = axes.flatten()
    sweep_notes = [36, 48, 60, 72]
    
    for i, note in enumerate(sweep_notes):
        sig_sel = render_voice(note, 1000.0, 0.0, 0, selected_tier, sr, secs)
        sig_best = render_voice(note, 1000.0, 0.0, 0, "best", sr, secs)
        
        p_sel = welch_pow(sig_sel)
        p_best = welch_pow(sig_best)
        
        freqs = np.fft.rfftfreq(1024, 1 / sr)[:512]
        
        axes[i].plot(freqs, 10 * np.log10(p_sel + 1e-30), label=f"{tier}")
        axes[i].plot(freqs, 10 * np.log10(p_best + 1e-30), '--', label="Best")
        
        if show_spice:
            ref_hz = 440.0 * (2.0 ** ((note - 69) / 12.0))
            cv_sp = vr.solve_cv(ref_hz, sr, secs=0.05, tol=0.01, iters=3)
            sig_sp = vr.voice_reference(note, cutoff_itail=32e-6, rreso=1e12, sr=sr, secs=secs, cv=cv_sp)
            p_sp = welch_pow(sig_sp)
            axes[i].plot(freqs, 10 * np.log10(p_sp + 1e-30), ':', label="SPICE Reference")
            
        axes[i].set_title(f"Note {note}")
        axes[i].set_xlabel("Frequency (Hz)")
        axes[i].set_ylabel("Power (dB)")
        axes[i].set_xscale('log')
        axes[i].set_xlim(20, sr/2)
        axes[i].grid(True)
        axes[i].legend()
        
    plt.tight_layout()
    st.pyplot(fig)
    
    st.write("Waveform Plot Comparison (MIDI note 60, first 10ms)...")
    sig_sel = render_voice(60, 1000.0, 0.0, 0, selected_tier, sr, secs)
    sig_best = render_voice(60, 1000.0, 0.0, 0, "best", sr, secs)
    
    samples_10ms = int(0.01 * sr)
    t = np.arange(samples_10ms) / sr * 1000.0
    
    fig, ax = plt.subplots(figsize=(8, 3))
    ax.plot(t, sig_sel[:samples_10ms], label=f"{tier}")
    ax.plot(t, sig_best[:samples_10ms], '--', label="Best")
    
    if show_spice:
        ref_hz = 440.0 * (2.0 ** ((60 - 69) / 12.0))
        cv_sp = vr.solve_cv(ref_hz, sr, secs=0.05, tol=0.01, iters=3)
        sig_sp = vr.voice_reference(60, cutoff_itail=32e-6, rreso=1e12, sr=sr, secs=secs, cv=cv_sp)
        ax.plot(t, sig_sp[:samples_10ms], ':', label="SPICE Reference")
        
    ax.set_title("Waveform Comparison")
    ax.set_xlabel("Time (ms)")
    ax.set_ylabel("Amplitude")
    ax.grid(True)
    ax.legend()
    st.pyplot(fig)

st.header("2. VCF & Envelope Analysis")
if st.button("Run Filter/Env Evaluation"):
    st.write("Evaluating Envelope Linearity and T60...")
    sr = 44100.0
    secs = 1.0 # 1.0s to measure decay
    
    # We trigger note 60 with a decay envelope
    sig_sel = render_voice(60, 2000.0, 0.5, 0, selected_tier, sr, secs)
    sig_best = render_voice(60, 2000.0, 0.5, 0, "best", sr, secs)
    
    env_sel = np.abs(hilbert(sig_sel))
    env_best = np.abs(hilbert(sig_best))
    
    t60_sel = t60_decay_time(env_sel, sr)
    t60_best = t60_decay_time(env_best, sr)
    
    st.write(f"**T60 Decay Time ({tier}):** {t60_sel:.3f} s")
    st.write(f"**T60 Decay Time (Best):** {t60_best:.3f} s")
    
    if show_spice:
        ref_hz = 440.0 * (2.0 ** ((60 - 69) / 12.0))
        cv_sp = vr.solve_cv(ref_hz, sr, secs=0.05, tol=0.01, iters=3)
        cutoff_itail = 32e-6 * (2000.0 / 1002.0) ** (1 / 0.78)
        sig_sp = vr.voice_reference(60, cutoff_itail=cutoff_itail, rreso=100.8e3, sr=sr, secs=secs, cv=cv_sp)
        env_sp = np.abs(hilbert(sig_sp))
        t60_sp = t60_decay_time(env_sp, sr)
        st.write(f"**T60 Decay Time (SPICE Reference):** {t60_sp:.3f} s")
        
    t = np.arange(len(env_sel)) / sr
    fig, ax = plt.subplots(figsize=(8, 4))
    ax.plot(t, env_sel, label=f"{tier} Envelope")
    ax.plot(t, env_best, '--', label="Best Envelope")
    if show_spice:
        ax.plot(t, env_sp, ':', label="SPICE Envelope")
    ax.set_title("Amplitude Decay Envelope")
    ax.set_xlabel("Time (s)")
    ax.set_ylabel("Magnitude")
    ax.grid(True)
    ax.legend()
    st.pyplot(fig)
    
    st.write("Measuring DC Offset Drift...")
    dc_sel = dc_offset_drift(sig_sel, sr)
    dc_best = dc_offset_drift(sig_best, sr)
    if show_spice:
        dc_sp = dc_offset_drift(sig_sp, sr)
    
    fig, ax = plt.subplots(figsize=(8, 3))
    ax.plot(t, dc_sel, label=f"{tier} DC Drift")
    ax.plot(t, dc_best, '--', label="Best DC Drift")
    if show_spice:
        ax.plot(t, dc_sp, ':', label="SPICE DC Drift")
    ax.set_title("DC Offset Drift (5Hz Lowpass)")
    ax.set_xlabel("Time (s)")
    ax.set_ylabel("Voltage / Amplitude")
    ax.grid(True)
    ax.legend()
    st.pyplot(fig)
