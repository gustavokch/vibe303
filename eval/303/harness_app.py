import streamlit as st
import subprocess
import numpy as np
import matplotlib.pyplot as plt
from metrics import pitch_tracking_error_cents, welch_pow

st.title("Analog Tier Voicing Harness")
tier = st.sidebar.selectbox("Tier", ["Low", "High", "Ultra"])
st.write(f"Ready to evaluate {tier} tier.")

st.header("1. VCO Analysis")
if st.button("Run VCO Evaluation"):
    # Enforce 0 age, 0 tol for baseline
    st.write("Running baseline pitch measurement...")
    # Trigger subprocess to render303 for MIDI notes 36 to 72
    # Compute pitch deviation using pitch_tracking_error_cents
    # Plot cents deviation using matplotlib/streamlit
    fig, ax = plt.subplots()
    ax.set_title(f"{tier} - Pitch Tracking Error (cents)")
    ax.set_xlabel("MIDI Note")
    ax.set_ylabel("Cents")
    st.pyplot(fig)
    
    st.write("Running 3-Octave Sweep...")
    # Trigger render303 --sweep
    # Compute and plot spectral overlays
    fig, ax = plt.subplots()
    ax.set_title(f"{tier} - 3-Octave Sweep Spectrum")
    st.pyplot(fig)
