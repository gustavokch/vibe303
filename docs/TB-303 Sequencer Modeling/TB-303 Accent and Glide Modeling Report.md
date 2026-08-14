# Accurately Modeling the TB-303 Accent and Glide: Circuit-Level Analysis and the jc303/Open303 Reference Implementation

## Executive Summary

This report consolidates three evidence streams — the Roland TB-303 service-notes schematic, the published circuit analyses of Robin Whittle (developer of the Devil Fish modification) and the wider DIY community, and a line-by-line reading of the open-source **jc303** plugin (midilab's JUCE port of Robin Schmidt's **Open303** DSP engine) [^43^][^26^] — into a single modeling specification for the **Accent** and **Glide (Slide)** subsystems, aimed at component-level emulation.

The headline findings are:

- **Accent is not a simple "louder + brighter" control.** It is a switched reconfiguration of the synth: on accented notes an IC12 (4066) analog switch (a) shorts the ENV.DECAY pot so the Main Envelope Generator (MEG) falls to its minimum (~200 ms) decay, (b) injects the MEG into the BA662A VCA control node through a 47 kΩ + 0.033 µF softening network, and (c) injects the MEG into the filter's control-current summing node through the unique **Accent Sweep Circuit** (diode D24, R46 47 kΩ, C13 1 µF, and the second gang of the Resonance pot) [^8^][^4^].
- The Accent Sweep capacitor C13 gives the filter sweep **analog memory**: fast charge through the diode, slow discharge through the wiper/mixing resistance, so successive accents in quick succession produce rising filter peaks — the famous "wow" staircase. This is the single most-missed behavior in casual emulations [^4^][^5^].
- **Glide is a constant-time exponential lag on the pitch CV**, implemented by switching C35 (0.22 µF) out of a low-impedance path so it must charge through roughly 100 kΩ — a first-order RC with **τ ≈ 22 ms** (small-signal cutoff ≈ 7.2 Hz). It is a CV-domain (semitone-domain) slew, not a frequency-domain slew [^33^][^32^].
- The slide switch is active **during the destination note** (no anticipation), and the gate is held through a slide so **envelopes are not re-triggered** — two timing facts that matter as much as the RC value [^25^].
- **jc303 captures the macro-structure well** (decay switching, VCA injection, swept second filter envelope, slew-limited pitch, legato handling) but deviates in measurable ways: it slews pitch in the **Hz domain** rather than the CV domain, uses τ = 12 ms against the hardware's ~22 ms, models the sweep circuit as a **single symmetric leaky integrator** (τ = 15 ms) with no charge/discharge asymmetry and therefore much weaker sweep memory, and omits the Resonance-knob coupling of the sweep depth. Section 7 quantifies every gap; Section 8 gives concrete equations, discretizations and parameter values to close them in a component model.

---

## 1. Research Basis and Method

Three primary evidence streams were used. First, the **TB-303 service notes** (schematic), from which all component designators and values cited here were read directly: the accent-relevant parts cluster around the VCA (IC15, BA662A), the ENV block, the IC12 (4066) quad analog switch, and the VR3–VR7 pot row; the glide-relevant parts cluster around the 6-bit DAC (IC9 4174 + IC10 4050 + R-2R ladder R74–R90), IC12, Q29/Q30, R93 and C35 [^8^]. Second, the **published analyses of Robin Whittle**, whose Devil Fish modification required exact knowledge of both subsystems: his 1999 essay isolates the Accent Sweep Circuit, and his slide treatise documents the sequencer's clock/gate/slide timing from direct investigation [^6^][^25^]. Third, the **jc303 source tree** (cloned from `github.com/midilab/jc303`, v0.12.x), whose `src/dsp/open303` engine is Robin Schmidt's Open303 code — itself the product of a 15-year community reverse-engineering effort that began with a 2009 KVR call for collaborators [^43^][^46^].

Where the three streams disagree, this report flags the disagreement explicitly rather than silently picking a winner. The most visible case is Whittle's recollection of the sweep pot as "a 100k pot" where the service notes show VR4 as a **50 kΩ dual-gang (B-taper)** part; another is the VEG (volume envelope) decay, described as "rather long" / "approximately 3–4 seconds" in Open303's header comments while the Open303 constructor programs 1230 ms. Both cases are discussed in context below, because a component model must ultimately be tuned against measurements (the Din Sync reference recordings being the canonical public dataset [^5^]), not against any single document.

## 2. Control Architecture Relevant to Accent and Glide

### 2.1 Two envelope generators and one accent switch

The TB-303 contains exactly two envelope generators, which Whittle names the **MEG (Main Envelope Generator)** and the **VEG (Volume Envelope Generator)**. The VEG has a sharp attack and a long, fixed exponential decay and drives only the VCA (a BA662A OTA, IC15). The MEG also has a sharp attack (~3 ms on the original machine) and an exponential decay; it primarily modulates the filter cutoff via the ENV.MOD pot, but on accented notes it is re-routed to do two additional jobs [^4^][^21^]. Figure 1 shows the full control-signal flow with component references.

![TB-303 Accent & Glide control-signal flow](report_assets/fig1_accent_flow.png)

The architectural point that most "303-in-an-hour" emulations miss is that **accent is a switch state, not a knob offset**. The sequencer's per-note accent flag energizes sections of a 4066 quad bilateral switch (IC12) for the duration of the accented note, and those switch sections re-wire three things at once: the MEG's decay resistance (shorted to minimum), the MEG→VCA path (connected through a softening RC), and the MEG→filter path (connected through the Accent Sweep network) [^8^][^4^]. The front-panel ACCENT knob (VR7, 50 kΩ B-taper) sits *after* the switch and scales the two injected paths. This is why accented notes are shorter, louder *and* hollower-sounding simultaneously, and why simply adding "velocity to cutoff and VCA" never quite sounds like a 303 [^1^].

### 2.2 Sequencer timing fundamentals

Both subsystems are slaves of the sequencer's timing, so it must be modeled first. Each 16th-note step spans **six clock pulses**. A normal note's gate goes high on the positive edge of pulse 0 and low on the negative (half) edge of pulse 3 — **3.5 pulses on, 2.5 off** (a 58.3% duty cycle) — which is the origin of the 303's staccato feel [^25^]. The accent gate is in time with the note gate, while the slide switch follows its own rule described in Section 4.2 [^5^].

Two consequences follow for a component model. First, envelope re-triggering happens on gate edges, so the gate duty cycle — not the step length — determines how much of each envelope's decay is audible before the next note. Second, because the accent state persists for the whole note (it is a level, not a trigger), the 4066-based reconfiguration persists too: the MEG decay stays short and the injection paths stay connected for the entire accented note, including any slide-tail that extends beyond the gate, which is why an accented slid note still "rings" softly after the VEG has closed [^25^].

## 3. The Accent System from the Schematics

### 3.1 MEG decay shortening on accented notes

The MEG's decay time is set by the discharge resistance around its timing capacitor, of which the front-panel DECAY pot (VR6, **1 MΩ A-taper**) forms the variable part, giving the documented **200 ms – 2 s** range for normal notes. On accented notes, a section of IC12 shorts the variable resistance, forcing the MEG to the same fast decay it would have with the DECAY pot fully counter-clockwise — approximately **200 ms**, fixed, regardless of the panel setting [^4^][^26^]. Whittle's Devil Fish famously replaces this with independent Normal-Decay and Accent-Decay pots (30 ms – 3 s each), which is the same circuit fact seen from the modifier's side [^21^].

For modeling, the important subtlety is that this is a **resistor switch across a continuously running RC decay**, not a parameter that only matters at trigger time. If an accent gate ends mid-note (or, in a Devil-Fish-style extended model, changes state mid-note), the decay time constant changes *from the current capacitor voltage*, with no discontinuity in the envelope output itself. A component model that implements the MEG as an explicit capacitor node with a switched conductance gets this for free; a "phase-machine" ADSR that recomputes targets at segment boundaries needs care to preserve continuity.

### 3.2 The VCA injection path

On accented notes the (now fast) MEG is added to the VCA's control current **through an RC softening network of 47 kΩ and 0.033 µF** — R119 and C36 on the schematic, feeding the BA662A's control node via R162 (2.2 kΩ) [^8^]. Whittle identifies this as "the primary reason why accented notes are louder": the VEG's own long decay is still present underneath, so the VCA control becomes a **multi-segment envelope** — fast attack, fast partial decay (the ~200 ms MEG), then the slow VEG tail — with a higher peak than a normal note [^4^].

The softening network's time constant (47 kΩ × 0.033 µF ≈ **1.55 ms**) is small but audible: it rounds the leading edge of the amplitude boost just enough to prevent a hard click and gives the accent its characteristic "thwap" rather than a digital-sounding step. Figure 3 (right panel) shows the composite VCA control envelope built from a simulated VEG plus the softened MEG injection; the resemblance to Whittle's verbal "instant attack / fast decay / slow decay" description is exact [^1^]. In jc303, this whole path is approximated by adding `0.45·MEG + accentGain·4.0·MEG` to the amp envelope while the gate is on, and then low-passing the sum with a fixed 12 dB/oct "de-clicker" at 200 Hz (`rosic_Open303.h`, lines 378–382) — a reasonable but clearly idealized stand-in for the physical R119/C36 network, as discussed in Section 7.

![Envelope generators and composite VCA envelope](report_assets/fig3_envelopes.png)

### 3.3 The Accent Sweep Circuit — the 303's signature

The third accent path is the one Whittle credits with the 303's "living" quality. From the service notes: the switched, accent-pot-scaled MEG drives **diode D24 in series with R46 (47 kΩ)**, into a network built around **C13 (1 µF/50 V electrolytic)** and the **second gang of the RESONANCE pot (VR4, 50 kΩ dual B-taper)**; the wiper feeds one of the filter's two control-current summing nodes through a mixing resistor [^8^]. Whittle's circuit description (diode + 47 kΩ, 1 µF cap, 100 kΩ mixing resistor, pot between) matches the schematic topology exactly, though he recalled the pot value as 100 kΩ where the service notes specify 50 kΩ [^4^].

The behavior follows from three facts. **(a) Asymmetric slew:** the diode makes charging fast (through R46 plus the CW-side pot resistance) while discharge must leak out through the wiper and the ~100 kΩ mixing network — charge τ on the order of 47–147 ms depending on wiper position, discharge τ longer still. **(b) Memory:** because discharge is slow relative to a 16th step (≈107–125 ms at typical acid tempos), C13 retains charge between successive accented notes, so each subsequent accent's filter peak starts from a higher floor and reaches a **higher peak** — Whittle's rising "wow" staircase, which he half-jokingly analyses as mimicking a distress cry. **(c) Resonance coupling:** because the sweep level pot *is* the Resonance pot's second gang, the accent's filter-sweep depth grows as the performer turns Resonance up — the harder you push the resonance, the more violently the accents sweep the cutoff [^4^][^5^]. Figure 4 simulates this against jc303's approximation, and the difference — continued build-up over 4–6 accents versus a plateau after the first — is the audible essence of "303 accent" that a component model must reproduce.

![Accent sweep memory vs jc303 approximation](report_assets/fig4_sweep_memory.png)

### 3.4 What the ACCENT knob itself does — and does not do

The ACCENT knob (VR7, 50 kΩ B-taper) attenuates the switched MEG signal feeding **both** the VCA injection and the sweep injection; it is a depth control for the whole accent complex, not a "filter accent" control [^4^]. With the knob at zero, accented notes are still *shorter* (the MEG decay shorting is upstream of the pot, driven by the accent gate directly) but not louder or brighter — a detail worth honoring in a model, because it means the decay-shortening and the depth scaling live at different points in the signal chain.

Two boundary conditions complete the picture. First, the ENV.MOD circuit does not reach zero modulation even fully counter-clockwise, so the MEG always nudges the cutoff somewhat, accented or not [^17^]. Second, accent has **no effect on the pitch CV path at all**: Roland's own manual notes that accent is absent from the CV/GATE outputs, which only carry pitch (with slide) and gate [^3^]. Accent is purely an envelope-routing phenomenon — pitch is the glide subsystem's exclusive territory.

## 4. The Glide (Slide) System from the Schematics

### 4.1 Circuit: a switched first-order RC lag on the pitch CV

Pitch is generated digitally: the CPU and IC9 (4174 hex latch) hold the note bits, buffered by IC10 (4050) into a 1%-matched R-2R ladder (R74–R90) that forms a 6-bit DAC — the same DAC documented by Whittle for the Devil Fish's MIDI-out pitch sensing [^8^][^20^]. The slide circuit sits between the DAC and the VCO's exponential converter: C35 (**0.22 µF**) is the lag capacitor, and Q29/Q30 with the IC12 (4066) switch sections reconfigure its charging path. Community analysis of the schematic describes the switching precisely: with the slide signal off, Q29 is off, Q30 and IC12 pin 12 are driven on, and the top of C35 is switched to the op-amp (DAC buffer) output — a low-impedance drive, so the CV follows note changes essentially instantly; with slide on, that switch opens and C35 must charge/discharge through a resistance of about **100 kΩ** [^34^].

The resulting time constant is **τ ≈ 100 kΩ × 0.22 µF ≈ 22 ms**, an equivalent first-order low-pass cutoff of ≈ **7.2 Hz**, which is exactly what independent measurement-oriented analysis of the slide arrives at (7.234 Hz idealized, described there as "behaves like typical portamento in analog synth") [^33^]. This is a **constant-time** portamento: whatever the interval, the CV traverses the same exponential approach with the same τ — a defining 303 trait explicitly noted in reference documentation, and the opposite of constant-rate glide [^12^]. The widely quoted "60 ms slide time" (used by Open303/jc303 and by the Devil Fish's 5×-longer 300 ms extension [^21^]) corresponds to the perceptual settling duration — roughly 2.7τ, ~93% of the way — rather than to τ itself; Section 8 recommends parameterizing on τ to avoid this ambiguity.

### 4.2 The slide timing algorithm: no anticipation

Whittle's timing investigation settles a persistent rumor: the 303 does **not** look ahead. The slide switch goes active **one 16th after the start of the note carrying the slide flag** — i.e., at the start of the *destination* note — and remains active until the end of that destination note, however long either note is [^25^]. The CV therefore sits exactly on the first note's pitch for its full duration and only begins its exponential approach when the second note's step begins. Figure 2 renders the complete timing: clock, gate, slide window, and the DAC-vs-VCO CV traces.

![Sequencer timing with slide](report_assets/fig2_timing.png)

Modeling implications are direct. The "slide" signal in your sequencer model is a **state flag with a one-step delay**, not an attribute consumed at the flagged note's trigger. Chained slides (slide flags on consecutive notes) simply keep the lag switch open across multiple steps, and the CV chases each new DAC value in turn, restarting its exponential approach from wherever it currently is — the classic acid "rubber ball" pitch motion. A tied note (slide between two notes of equal pitch) engages the same machinery but produces no audible glide, only the gate extension below [^3^].

### 4.3 Gate behavior during slides

The second function of a slide flag is that the flagged note's gate **does not close**: it extends through the destination note's gate interval, so neither the MEG nor the VEG re-triggers at the note boundary [^25^][^5^]. Legato pitch change without envelope re-trigger is what makes a 303 slide sound like one continuous gesture rather than two notes.

The interaction with accent deserves care in a component model. If the *destination* note of a slide is accented, the accent switch closes at the destination note's start — reconfiguring MEG decay and opening the injection paths — even though the envelopes were not re-triggered. On the real machine the audible effect depends on how much envelope level remains from the previous note plus the changed decay constant and the sweep circuit's stored charge. Open303 handles the analogous case by calling `slideToNote()`, which updates the decay constants and `accentGain` without touching envelope states (`rosic_Open303.cpp`, lines 248–265) — a defensible macro-level equivalent, provided the sweep-capacitor state (Section 3.3) is a free-running node that is *not* reset on note events.

## 5. How jc303/Open303 Models the Accent

### 5.1 Architecture and parameter mapping

jc303 is a JUCE wrapper around the Open303 engine; the accent-relevant state lives in `rosic_Open303.{h,cpp}` and the wrapper mapping in `src/JC303.cpp` [^43^]. The front-panel **Accent** parameter (0–1) maps linearly to 0–100% and is stored as `accent` (0–1) via `setAccent()`. On every note event the engine decides `hasAccent` (sequencer flag, or MIDI velocity ≥ 100 in the MIDI fallback mode) and executes either `triggerNote()` or `slideToNote()`; in both, an accented note sets `accentGain = accent`, switches the MEG decay to `accentDecay` (default **200 ms**, vs `normalDecay` default 1000 ms, panel range 200–2000 ms), and switches the amp envelope release to `accentAmpRelease` (50 ms vs 1 ms) — directly mirroring the hardware's 4066 reconfiguration [^43^].

The MEG itself is `DecayEnvelope`: a pure one-pole exponential `y *= c` with `c = exp(-1/(fs·τ))`, triggered to a normalized peak — the right skeleton for a hardware RC envelope. The VEG is `AnalogEnvelope` (attack/decay/sustain/release) with attack 0 ms, decay 1230 ms, sustain 0 — note the tension with the header comment "fixed to approximately 3-4 seconds" on the real machine, one of the places where a component model should trust measurement over either number [^26^]. The Devil-Fish-inspired MOD section (toggled by `switchModState`) widens the ranges: decay 30–3000 ms, accent-decay 30–3000 ms, soft attack 0.3 ms up, slide time 2–360 ms — ranges taken from the Devil Fish specification [^41^][^21^].

### 5.2 The per-sample accent math

The filter-side accent is computed in `Open303::getSample()` (`rosic_Open303.h`, lines 365–376). The MEG output feeds `rc1` (a `LeakyIntegrator`, τ = normal attack ~3 ms) for the normal ENV.MOD path, scaled/offset into octaves by a *measured* mapping (`calculateEnvModScalerAndOffset()` — constants fitted from hardware measurements, nominal cutoff range c0 ≈ 314 Hz to c1 ≈ 2394 Hz). For the accent path, the raw MEG is gated by `accentGain`, smoothed by `rc2` (τ = **15 ms**), scaled by `accentGain`, and added **in the log2 (octave) domain**: `instCutoff = cutoff · 2^(tmp1+tmp2)`. This is structurally faithful — a smoothed copy of the shortened MEG added to filter cutoff on accented notes, scaled by the Accent knob — and it correctly operates in the exponential (current-summing) domain of the real filter control node.

The amp side adds `0.45·MEG + accentGain·4.0·MEG` to the VEG output while the gate is on, then passes the sum through `ampDeClicker`, a 12 dB/oct low-pass at 200 Hz with −3 dB gain — standing in for the R119/C36 softener and general envelope click suppression. The engineering is tidy, but three idealizations relative to the schematic matter for a component-level model and are carried into the gap table in Section 7: the sweep path is a **symmetric** RC (no diode, no charge/discharge asymmetry, τ=15 ms ≪ the hardware's effective discharge), the sweep has **no dependence on the Resonance knob** (the VR4-B coupling), and the VCA injection is a fixed linear mix rather than a distinct 1.55 ms-softened control-current injection into an OTA gain cell.

## 6. How jc303/Open303 Models the Glide

### 6.1 The slew limiter and its calibration

Glide lives in one line of the engine: `double instFreq = pitchSlewLimiter.getSample(oscFreq);` (`rosic_Open303.h`, line 361), where `pitchSlewLimiter` is a `LeakyIntegrator` — the one-pole `y = in + coeff·(y − in)`, `coeff = exp(−1/(fs·τ))`. The calibration is in `setSlideTime()`: **τ = 0.2 × slideTime**, with `slideTime` defaulting to **60 ms**, giving **τ = 12 ms**, flagged in the source itself with a `\todo: tweak the scaling constant` comment [^43^]. The MOD-mode Slide Time knob maps linearly to 2–360 ms of `slideTime`, echoing the Devil Fish's internal-sequencer range of 60–360 ms [^41^].

Two deviations from hardware follow, both visible in Figure 5. First, the slew is applied to **frequency in Hz**, not to the pitch CV: the hardware lags the *voltage* feeding an exponential (1 V/oct) VCO, so semitones follow the pure RC exponential; jc303 lags Hz, so semitones follow a log-warped curve that rises too fast early in upward slides (and falls too slowly early in downward ones). For small intervals the curves are nearly indistinguishable; for octave-plus slides the difference is clearly audible. Second, τ = 12 ms is about half the hardware's ~22 ms, so even at equal curve shape jc303's glide settles roughly twice as fast [^33^].

![Slide domain and constant-time comparison](report_assets/fig5_slide_domains.png)

### 6.2 Legato, gate, and slide-flag handling

The legato semantics are handled well. In MIDI mode, a note-on arriving while another note is held routes to `slideToNote()` (no envelope re-trigger, no `pitchSlewLimiter` reset — the slew state just chases the new `oscFreq`), and releasing back to a held note slides back; `triggerNote()` alone calls `pitchSlewLimiter.setState(oscFreq)` to snap the CV when starting from silence [^43^]. In the internal sequencer mode, a slide flag on the current note with a gated next note sets `noteOffCountDown = INT_MAX` (gate held) and `slideToNextNote = true`, so the next step slides instead of re-triggering; otherwise the gate closes after `getStepLengthInSamples()`.

Two fidelity notes remain. The engine's gate length is `stepLength = 0.5` of a step (`rosic_AcidPattern.cpp`), versus the hardware's 3.5/6 ≈ **0.583** — a ~17% shorter gate that slightly over-staccatos every note [^25^]. And the slide *switch window* is implicit in the slew-state machine rather than the hardware's explicit "lag engaged only during the destination note": because the destination step in Open303 either slides (flag on previous note) or snaps (state set on trigger), a non-slide note correctly cannot glide, but chained-slide edge cases — e.g., slide flag on the destination note followed by a rest — are governed by different logic than the 4066/IC13 hardware described in Section 4.2. For a component model, implementing the explicit slide-window flag is both simpler and more correct.

## 7. Gap Analysis: Hardware vs jc303

The table below consolidates every accent/glide behavior, its hardware ground truth, jc303's implementation, and a severity rating for audible impact in a component-level model. "Ground truth" values come from the service notes schematic [^8^], Whittle's analyses [^4^][^25^], and community measurements [^33^]; jc303 values come from the v0.12.x source [^43^].

| # | Behavior | TB-303 hardware | jc303 / Open303 | Gap severity |
|---|----------|-----------------|------------------|--------------|
| G1 | MEG decay on accent | 4066 shorts ENV.DECAY pot → fixed ~200 ms, continuous RC switch [^8^][^4^] | Decay constant swapped to `accentDecay` = 200 ms at note event | **Low** — equivalent at note boundaries; hardware also switches mid-note correctly, which the swapped constant handles only at events |
| G2 | Accent depth knob | VR7 scales VCA + sweep injection after the switch [^4^] | `accentGain = accent` scales both filter term and amp term | **Low** — correct structure |
| G3 | VCA injection shape | 47 kΩ × 0.033 µF ≈ 1.55 ms softening into BA662A control current [^8^] | Linear mix `accentGain·4.0·MEG` + 200 Hz/12 dB de-clicker | **Low–Med** — softening exists but is not the physical network; de-clicker also processes the VEG itself |
| G4 | Sweep charge/discharge asymmetry | Diode D24: fast charge, slow discharge through wiper + ~100 kΩ [^4^] | Single symmetric leaky integrator rc2, τ = 15 ms | **High** — wrong curve shape; 15 ms loses the slow-tail "wow" |
| G5 | Sweep memory across accents | C13 retains charge → rising peaks over 4–6 accents [^4^][^5^] | rc2 state persists but decays ~99% in one 16th step (τ=15 ms) | **High** — signature staircase largely absent (Fig. 4) |
| G6 | Sweep depth ∝ Resonance knob | Sweep pot is VR4-B, the Resonance pot's second gang [^8^][^4^] | Not modeled | **Med–High** — accent/resonance interaction is core 303 behavior |
| G7 | ENV.MOD never reaches zero | Circuit floor below pot minimum [^17^] | Captured via measured offset `envOffset` ≥ ~0.29 | **Low** — handled, via fitted mapping |
| G8 | Glide curve domain | RC lag on CV → exponential in semitones, τ ≈ 22 ms [^33^][^32^] | RC lag on Hz, τ = 12 ms (0.2 × 60 ms) | **Med–High** — wrong domain + ~2× too fast (Fig. 5) |
| G9 | Constant-time property | Interval-independent duration [^12^] | Preserved (any RC lag is constant-time) | **Low** — preserved |
| G10 | Slide window timing | Lag engaged during destination note only; no anticipation [^25^] | Implicit in trigger/slide state machine | **Low–Med** — edge cases (chains, rests) differ; recommend explicit flag |
| G11 | No re-trigger on slide | Gate held; MEG/VEG continue [^25^] | `slideToNote()` leaves envelope states untouched | **Low** — correct |
| G12 | Gate duty cycle | 3.5/6 pulses = 58.3% of step [^25^] | `stepLength` = 0.5 | **Med** — 17% shorter gate tightens every note |
| G13 | VEG decay | Fixed, long (~3–4 s per Open303 docs) [^26^] | 1230 ms programmed | **Med** — unresolved; tune against recordings |
| G14 | Accent at slide destination | Accent switch reconfigures paths without re-trigger [^25^] | `slideToNote()` updates decay + accentGain | **Low** — correct, provided sweep cap is free-running |

Three of these gaps (G4, G5, G6) concentrate in the Accent Sweep Circuit, which is precisely the subsystem Whittle identifies as the 303's irreducible core — and precisely the one most simplified in jc303. The glide gaps (G8, G10, G12) are individually modest but compound: a Hz-domain slew at half the hardware τ, gated 17% shorter, on slides that don't retrigger, shifts the perceived "liquidity" of every acid line. A component model that closes G4–G6 and G8 will sound categorically closer than one that closes everything else.

## 8. Modeling Specification and Recommendations

### 8.1 Recommended accent model

Implement the MEG as an explicit capacitor node `V_meg` with an attack conductance (τ_a ≈ 0.9–3 ms [^21^]) and a **switched decay conductance**: `G_decay = G_min + (1−A)·g(VR6)`, where `A ∈ {0,1}` is the accent switch state and `G_min` corresponds to ~200 ms. This reproduces G1 exactly, including mid-note switching continuity. Drive three consumers from `V_meg`: the ENV.MOD path (with a small floor so modulation never reaches zero, per G7), the VCA injection, and the sweep circuit.

The VCA injection should be its own two-element network: `V_inj = RC(V_meg · A · k_acc; τ=1.55 ms)` summed into the VCA control current alongside the VEG, with `k_acc` the Accent knob. For the **sweep circuit**, model C13 explicitly with diode asymmetry — the one-state, two-time-constant system below reproduces G4–G6:

```
if (V_meg·A·k_acc·r(V_res) > V_c13):        # charge through D24 + R46 (+CW pot segment)
    V_c13 += (1 - exp(-dt/tau_chg)) * (src - V_c13)   # tau_chg ≈ 47–147 ms, r(V_res)-dependent
else:                                       # discharge through wiper + ~100k mixing R
    V_c13 += (1 - exp(-dt/tau_dis)) * (0   - V_c13)   # tau_dis ≈ 100–150 ms
cutoff_current += V_c13 * k_mix             # into the filter summing node (log domain)
```

with `r(V_res)` the Resonance-knob-dependent scaling from VR4-B (increasing sweep depth with resonance), `τ_dis > τ_chg` always, and **no reset of `V_c13` on note events** — the free-running state is the memory (G5). If your component model solves the filter's control node in the current/exponential domain (as Open303 correctly does with `cutoff·2^(…)`), sum `V_c13` there, not in the linear-cutoff domain. Suggested starting values from the schematic: charge through 47 kΩ + wiper-dependent segment into 1 µF; discharge through the ~50–100 kΩ wiper/mixing path; then trim ±30% against the Din Sync reference recordings [^8^][^5^].

### 8.2 Recommended glide model

Replace any Hz-domain slew with a **CV-domain lag**: maintain `V_cv` (semitones or volts), update it with `V_cv += (1 − exp(−dt/τ))·(V_dac − V_cv)` only while the **slide window flag** is open, and snap `V_cv ← V_dac` at hard triggers; convert to frequency afterward via `f = f0·2^(V_cv/12)`. Set **τ = 22 ms** as the calibrated center (τ range 2–110 ms if you offer a Devil-Fish-style Slide Time control — the DF reaches "five times longer than normal" [^21^]), and expose the "~60 ms slide time" only as UI semantics (≈2.7τ settling) so the DSP constant stays honest. This closes G8 and G9.

Implement the slide window exactly as the hardware does (G10): the slide flag on note *N* opens the lag from the start of note *N+1* to its end; hold the gate open across the boundary so no envelope re-triggers (G11); and set the gate duty to **3.5/6 ≈ 0.583** of a step rather than 0.5 (G12). If the destination note is accented, close the accent switch at its start without re-triggering (G14), letting your free-running MEG/VEG/C13 nodes respond through the reconfigured conductances — which, in a true component model, is precisely what "accent on a slid note" means physically.

### 8.3 Verification plan

Calibrate and verify in layers, cheapest signal first. **(1) CV domain:** probe your model's pitch CV and check τ = 22 ms ±10% and constant-time invariance across 2-, 7-, and 12-semitone slides; the hardware behavior is well documented and needs no audio [^12^][^33^]. **(2) Envelope domain:** check accented MEG decay ≈ 200 ms independent of the Decay knob, and the multi-segment VCA envelope's two-slope shape (Fig. 3). **(3) Behavioral domain:** program 6–8 successive accented 16ths at 120–140 BPM with high resonance and confirm rising filter peaks (Fig. 4) — this single test exercises G4, G5 and G6 together and is the most discriminating accent test available.

**(4) Reference domain:** A/B against the Din Sync systematic recordings (a C note mapped across knob positions) and a known musical line such as *Da Funk*, the evaluation methodology recommended in the modular-emulation literature [^5^]. Keep in mind that unit-to-unit component tolerances (the R-2R ladder is factory-selected to 1%; electrolytics like C13/C35 have wide tolerances) mean "the" 303 response is a family of curves; matching the *structure* — asymmetry, memory, coupling, constant-time glide — is what reads as authentic, not chasing any single unit's numbers [^8^].

## 9. Quick-Reference Parameter Tables

**Accent subsystem — component values and model constants:**

| Element | Schematic ref. | Value | Model constant | Source |
|---------|---------------|-------|----------------|--------|
| Accent switch | IC12 (4066) sections | — | `A ∈ {0,1}`, held for note duration | [^8^] |
| MEG attack | — | ~3 ms (typ. 4 ms delay + 3 ms attack) | τ_a ≈ 0.9–3 ms | [^21^][^43^] |
| MEG decay, normal | VR6 (1 MΩ A-taper) + network | 200–2000 ms | g(VR6) | [^26^] |
| MEG decay, accented | VR6 shorted by IC12 | ~200 ms fixed | G_min | [^4^] |
| VEG decay | C37 network at IC15 | long, fixed (~3–4 s claimed; 1230 ms in Open303) | τ_veg, calibrate | [^26^] |
| Accent knob | VR7, 50 kΩ B | 0–100% depth | k_acc | [^8^] |
| VCA injection | R119 47 kΩ + C36 0.033 µF, R162 2.2 kΩ | τ ≈ 1.55 ms | injection softening | [^8^] |
| Sweep network | D24, R46 47 kΩ, C13 1 µF, VR4-B (50 kΩ), ~100 kΩ mix | τ_chg 47–147 ms, τ_dis ≳100 ms, depth ∝ resonance | asymmetric one-state RC | [^8^][^4^] |

**Glide subsystem — component values and model constants:**

| Element | Schematic ref. | Value | Model constant | Source |
|---------|---------------|-------|----------------|--------|
| Pitch source | IC9 4174 + IC10 4050 + R74–R90 R-2R | 6-bit DAC, 1 V/oct | V_dac | [^8^] |
| Lag capacitor | C35 | 0.22 µF | — | [^8^][^34^] |
| Charge resistance | ~100 kΩ path (R93 / switch network) | τ ≈ 22 ms (≈7.2 Hz) | CV-domain τ | [^33^][^32^] |
| Slide window | IC13 14013 + Q35/Q29/Q30 + IC12 | active during destination note | slide flag, 1-step delay | [^25^][^34^] |
| Gate duty | sequencer firmware | 3.5/6 pulses = 58.3% | 0.583 × step | [^25^] |
| "Slide time" convention | ~60 ms perceived (≈2.7τ); DF extends ×5 | UI mapping only | 60–300 ms ↔ τ 22–110 ms | [^21^][^43^] |
| jc303 reference | `pitchSlewLimiter`, τ = 0.2×slideTime | τ = 12 ms at default, Hz domain | replace with CV domain | [^43^] |

---

*Prepared from the Roland TB-303 service notes, Robin Whittle's published circuit analyses, community measurements, and a source-level reading of midilab/jc303 (Open303 engine). Component values read from the service-notes schematic are nominal Roland design values; simulation figures are first-order model predictions from those values, intended to illustrate curve shapes for calibration, not to replace measurements of physical units.*

[^1^]: https://www.patreon.com/LearningModular/posts/patch-ideas-i-tb-41917616
[^3^]: http://cdn.roland.com/assets/media/pdf/TB-303_OM.pdf
[^4^]: https://tinyloops.com/tb303/sound_robin_whittle.htm
[^5^]: https://olney.ai/ct-modular-book/tb-303.html
[^6^]: https://www.firstpr.com.au/rwi/dfish/303-unique.html
[^8^]: https://experimentalistsanonymous.com/diy/Schematics/Full%20Synths%20Drum%20Synths%20and%20Misc%20Synth/Roland%20TB-303.pdf
[^12^]: http://www.synth-db.com/synths/Roland/TB-303/TB-303.php
[^16^]: https://www.timstinchcombe.co.uk/index.php?pge=diode2
[^17^]: https://music.stackexchange.com/questions/72307/what-kind-of-envelope-has-the-roland-tr-03-and-what-does-the-parameter-do
[^20^]: https://www.firstpr.com.au/rwi/dfish/
[^21^]: http://www.synthdiy.com/show/?id=415
[^25^]: https://www.firstpr.com.au/rwi/dfish/303-slide.html
[^26^]: https://github.com/RobinSchmidt/Open303
[^29^]: https://gearspace.com/board/electronic-music-instruments-and-electronic-music-production/1286361-behringer-td-3-303-clone-281.html
[^32^]: https://gearspace.com/threads/behringer-td3-diy-mods.1289500/page-2
[^33^]: https://gearspace.com/threads/303-floaty-slide-sound.1045691/post-11499652
[^34^]: https://www.reddit.com/r/synthdiy/comments/d2ff82/midi_to_cv_portamento/
[^41^]: https://github.com/midilab/jc303/releases
[^43^]: https://github.com/midilab/jc303
[^46^]: https://www.kvraudio.com/forum/viewtopic.php?t=262829&start=1140
