# Design Specification: Voicing Calibration and CV/Pitch Tracking Parity

This document outlines the design for calibrating the VCO pitch tracking and VCF voicing (cutoff/resonance) of the **Low** (TPT ladder) and **High** (2x oversampled nodal ladder) fidelity tiers to match the reference behavior of the **Ultra** (nodal VCO + 4x oversampled active Q9 nodal ladder) tier.

---

## 1. VCO Pitch Tracking Parity

### Objective
Ensure that switching or crossfading between tiers does not introduce audible pitch shifts or phase drift. The Ultra tier's circuit VCO (`Vco303`) has slight non-linearities and interpolation tracking errors (up to ~8–10 cents) due to linear CV interpolation between its 25 LUT anchors. The polyBLEP oscillators in Low and High tiers must track these exact pitch deviations.

### Offline Characterization
We will write a characterization script `eval/303/calibrate_vco_pitch.py` that:
1.  Instantiates a raw `Vco303` model at `sr = 48000 Hz` in its nominal state (20°C, zero age/tolerance, zero drift).
2.  Sweeps MIDI notes $p_{\text{nominal}}$ from $24$ to $72$ in steps of 1.
3.  For each note, sets target frequency $f_{\text{nominal}} = 440 \cdot 2^{(p_{\text{nominal}} - 69)/12}$ on the VCO, renders 0.3 seconds of raw oscillator output, and computes the realized fundamental frequency $f_{\text{realized}}$ using sub-sample zero-crossing interpolation.
4.  Computes the pitch error in semitones:
    $$\Delta p = 12 \log_2(f_{\text{realized}} / f_{\text{nominal}})$$
5.  Fits a low-order polynomial mapping $p_{\text{nominal}}$ to the error:
    $$\Delta p(p_{\text{nominal}}) = a_3 \cdot p_{\text{nominal}}^3 + a_2 \cdot p_{\text{nominal}}^2 + a_1 \cdot p_{\text{nominal}} + a_0$$
6.  Prints the coefficients $[a_3, a_2, a_1, a_0]$.

### Runtime DSP Integration
In `Voice::tick` (inside `plugin303/src/dsp/voice.rs`), before setting oscillator frequencies for Low and High:
1.  Compute the target nominal pitch:
    $$p_{\text{nominal}} = \text{current\_pitch\_st} - 69.0 + \text{osc1\_pitch\_st} + \text{tuning\_st} + \text{pitch\_drift\_st} + \text{pitch\_thermal\_st} + 69.0$$
2.  Evaluate the fitted polynomial to get the pitch correction $\Delta p(p_{\text{nominal}})$.
3.  Compute the corrected pitch:
    $$p_{\text{corrected}} = p_{\text{nominal}} + \Delta p(p_{\text{nominal}})$$
4.  Compute the corrected oscillator frequency:
    $$osc1\_f = 440.0 \cdot 2^{(p_{\text{corrected}} - 69.0) / 12.0}$$
5.  Apply $osc1\_f$ to the oscillators (`self.osc`, `self.osc_hi`, etc.). 

> [!NOTE]
> The Ultra VCO is set using the original nominal frequency $osc1\_f$ (without the polynomial correction), as it is the target reference.

---

## 2. VCF Voicing Parity (LSD Optimization)

### Objective
Align the VCF cutoff and active resonance curves of the Low and High filters to match the Ultra VCF's active diode-ladder response across dynamic sweeps, minimizing Log Spectral Distance (LSD).

### Offline Optimization
To prevent numerical scaling issues, all optimization and fitting are performed in the normalized control space $[0, 1]$:
*   Cutoff control: $c \in [0, 1] \implies fc = 200 \cdot 20^c\text{ Hz}$
*   Resonance control: $r \in [0, 1] \implies \text{reso} = r$

We will write a calibration script `eval/303/calibrate_vcf_voicing.py` that:
1.  Defines a grid in control space:
    *   $c_{\text{nominal}} \in [0.0, 0.15, 0.3, 0.45, 0.6, 0.75, 0.9, 1.0]$ (8 points)
    *   $r_{\text{nominal}} \in [0.0, 0.2, 0.4, 0.6, 0.8, 0.85]$ (6 points)
2.  For each grid point $(c_{\text{nominal}}, r_{\text{nominal}})$:
    *   Renders the reference signal in the **Ultra** tier (note 57, 0.2s duration, Saw wave) with parameters $fc_{\text{nominal}} = 200 \cdot 20^{c_{\text{nominal}}}$ and $\text{reso}_{\text{nominal}} = r_{\text{nominal}}$ via the `render303` binary.
    *   Uses Nelder-Mead optimization (`scipy.optimize.minimize`) to search for the candidate parameters $(c_{\text{opt}}, r_{\text{opt}})$ that minimize the Welch LSD (computed using `spectral_match` in `eval/303/metrics.py`) of the candidate tier's render against the Ultra reference render.
    *   Does this optimization separately for **Low** (tier 0) and **High** (tier 1).
3.  Fits 2D polynomial surfaces mapping nominal controls to optimized controls for each tier:
    $$c_{\text{opt}} = P_c(c_{\text{nominal}}, r_{\text{nominal}})$$
    $$r_{\text{opt}} = P_r(c_{\text{nominal}}, r_{\text{nominal}})$$
    using polynomial terms up to degree 3:
    $$P(c, r) = \sum_{0 \le i+j \le 3} k_{ij} \cdot c^i \cdot r^j$$
4.  Prints the coefficient matrices for Low and High.

### Runtime DSP Implementation
In `Voice::tick` (inside `plugin303/src/dsp/voice.rs`):
1.  Obtain the nominal filter control values $c_{\text{nominal}}$ and $r_{\text{nominal}}$ (after applying drift, thermal, and age scaling).
2.  For **Low** and **High** tiers, evaluate the respective 2D polynomials to compute the optimized controls $c_{\text{opt}}$ and $r_{\text{opt}}$.
3.  Map the optimized controls to physical parameters:
    $$fc_{\text{opt}} = (200.0 \cdot 20.0^{c_{\text{opt}}}).clamp(20.0, sr \cdot 0.45)$$
    $$reso_{\text{opt}} = r_{\text{opt}}.clamp(0.0, 1.0)$$
4.  Apply $fc_{\text{opt}}$ and $reso_{\text{opt}}$ to the active filter (`self.filter` for Low, `self.ladder_hi` for High).

> [!NOTE]
> The Ultra tier bypasses this polynomial mapping and applies $fc_{\text{nominal}}$ and $reso_{\text{nominal}}$ directly.

---

## 3. Verification Gates

We will create integration tests in `tests/tier_calibration.rs`:
*   **VCO Pitch Parity Gate:** Verifies that the fundamental frequency of Low and High matches Ultra within $\pm 0.1\%$ (approx. 1.7 cents) across MIDI notes 24 to 72.
*   **VCF Voicing Parity Gate:** Verifies the Welch LSD and correlation under dynamic sweeps:
    *   High vs Ultra: Welch LSD $\le 2.0\text{ dB}$, Correlation $\ge 0.98$.
    *   Low vs Ultra: Welch LSD $\le 3.5\text{ dB}$, Correlation $\ge 0.95$.
