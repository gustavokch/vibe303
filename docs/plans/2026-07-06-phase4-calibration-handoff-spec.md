# Phase 4: Voicing Calibration and CV/Pitch Tracking Parity Spec

This spec details the design for aligning the voicing (VCF cutoff and resonance curves) and pitch tracking (VCO frequency deviation) of the **Low** and **High** fidelity tiers to match the **Ultra** tier. 

---

## 1. Objectives

*   **VCO Pitch Tracking Parity:** The Ultra tier's circuit VCO (`Vco303`) has slight non-linearities and temperature/drift offsets. The Low and High tiers (using polyBLEP oscillators) must match these exact pitch deviations so that switching between tiers doesn't introduce tuning shifts.
*   **VCF Voicing Parity:** Align the VCF cutoff and active resonance curves of the Low (TPT ladder) and High (nodal ladder at 2x oversampling) filters to match the Ultra tier's diode ladder response across all dynamic envelope sweeps and resonance settings.
*   **Methodology:** Use offline grid-sweep optimization to characterize the deviations of Ultra from nominal values, fit low-order polynomial correction curves, and apply these correction formulas in the runtime DSP loop of the Low and High tiers.

---

## 2. Technical Design

### Phase A: Offline Characterization & Curve Fitting
1.  **Grid Sweep:** Write a Python sweep script (e.g., under `eval/303/` using `compare_voice.py` or a dedicated tool) that renders the Ultra tier under various configurations:
    *   MIDI Notes: `24` to `72` (the active range of the VCO).
    *   Cutoff Frequencies: `200 Hz` to `4000 Hz`.
    *   Resonance values: `0.0` to `0.9`.
2.  **Measurements:**
    *   **VCO Pitch:** Measure the actual fundamental frequency ($f_{ultra}$) produced by the Ultra VCO solver at each MIDI note. Compute the pitch deviation in semitones: $\Delta p = 12 \log_2(f_{ultra} / f_{nominal})$.
    *   **VCF Cutoff/Resonance:** Measure the filter's frequency response (peak frequency and gain) for each cutoff and resonance setting in Ultra.
3.  **Curve Fitting:**
    *   Use `numpy.polyfit` to fit low-order polynomials (e.g., 2nd or 3rd order) modeling the pitch deviation as a function of nominal pitch:
        $$\Delta p(p_{nominal}) = a_2 \cdot p_{nominal}^2 + a_1 \cdot p_{nominal} + a_0$$
    *   Fit VCF corner frequency scaling curves to align the Low/High cutoff mapping to match Ultra's actual peak frequency response.

### Phase B: Runtime DSP Correction in `Voice`
1.  **Pitch Correction:** Apply the polynomial correction curve to the polyBLEP oscillators (`self.osc`, `self.osc2`, `self.sub_osc` in Low/High tiers):
    $$p_{corrected} = p_{nominal} + \Delta p(p_{nominal})$$
    Compute oscillator frequencies using $p_{corrected}$ so they track Ultra exactly.
2.  **VCF Cutoff Correction:** Apply the VCF correction polynomial to the cutoff calculation of `self.filter` (Low) and `self.ladder_hi` (High) in `Voice::tick` to align their cutoff points dynamically to Ultra's corner response.

### Phase C: Verification Gates
Create a test suite `tests/tier_calibration.rs` that validates:
*   **Pitch Parity:** Low and High tier pitch tracking matches Ultra's fundamental frequency within $\pm 0.1\%$ across the entire MIDI range ($24$-$72$).
*   **Voicing Parity:** Spectral match tests (Welch LSD and correlation) verify that Low/High match Ultra under dynamic sweeps:
    *   High vs Ultra: Welch LSD $\le 2.0\text{ dB}$, Correlation $\ge 0.98$.
    *   Low vs Ultra: Welch LSD $\le 3.5\text{ dB}$, Correlation $\ge 0.95$.
