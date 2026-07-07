# Analog Tier Voicing Evaluation Harness Design

## Overview
A unified testing and calibration harness to systematically analyze and quantify voicing differences between the different analog quality tiers of the Vibe303 plugin. This harness serves a dual purpose: an interactive web UI for real-time tweaking and visual debugging, and an offline CLI mode for automated grid evaluation.

The primary goal is to provide the metrics and visual feedback necessary to align the pitch, volume, envelope shape, and spectral signature of the lower tiers to match the "Ultra" tier, which serves as the gold reference.

## Architecture
- **Language/Framework:** Python, utilizing Streamlit (or Dash) for the interactive web dashboard.
- **Backend:** Calls the compiled Rust `render303` binary via subprocess to generate audio files under specific configurations.
- **Modes:**
  - **Interactive Mode:** A local web dashboard with sidebar controls for adjusting `comp_age`, `tolerance`, and `thermal_noise`. Changing parameters triggers a background render and instantly updates the UI plots.
  - **Offline CLI Mode:** Runs the core metric evaluations across a full parameter grid and outputs structured reports (JSON/Markdown) and heatmaps, extending the existing `measure_tiers.py` workflow.

## Component Analysis Modules

### 1. VCO Analysis
- **Goal:** Align pitch tracking, volume, and spectral signature across tiers.
- **Baseline Constraint:** Pitch tracking error measurements will enforce `comp_age = 0`, `tolerance = 0`, and `thermal_noise = 0` by default.
- **Metrics:**
  - Pitch tracking error (deviation in cents from the ideal note target across the MIDI range).
  - Volume tracking (RMS dBFS delta vs. Ultra tier).
  - Phase and period jitter variance (measuring thermal noise impact).
- **Visualizations:**
  - Overlay plots of Pitch Deviation (cents) vs. Note.
  - 3-Octave continuous sweep spectrum overlays (comparing Low, High, Ultra tiers).

### 2. VCF Analysis
- **Goal:** Align filter cutoff tracking and resonance behavior across tiers.
- **Metrics:**
  - Spectral match (LSD and Centroid error) against the Ultra tier reference.
  - Cutoff tracking error (Hz deviation from the expected cutoff frequency).
- **Visualizations:**
  - dB spectrum plots showing steady-state frequency response overlays.
  - Time-domain waveform overlays for high-resonance/self-oscillating states (to inspect phase alignment, amplitude, and ringing).

### 3. Envelope Analysis
- **Goal:** Ensure transient impact, punch, and timing match across tiers.
- **Metrics:**
  - T60 decay time variance.
  - Attack peak amplitude jitter.
  - Shape curvature metrics (log vs linear fit).
- **Visualizations:**
  - Raw amplitude curve overlays mapped over time (Attack, Decay, Release trajectories).
  - Spectral analysis of the transients (FFT of the attack phase) to guarantee the spectral footprint of the "click" matches.

### 4. DC Offset & Drift
- **Goal:** Prevent unwanted asymmetric clipping or baseline shift due to thermal drift and analog wear.
- **Metrics:**
  - Maximum DC deviation (mV) from zero over a sustained duration (e.g., 2-5 seconds).
- **Visualizations:**
  - Low-pass filtered DC component plotted over time, showing the drift trajectory.
