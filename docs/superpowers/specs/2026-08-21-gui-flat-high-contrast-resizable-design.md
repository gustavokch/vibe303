# Flat, High-Contrast, Resizable GUI Design Specification (`nih_plug_egui`)

**Date:** 2026-08-21  
**Status:** Approved  
**Topic:** Modern, non-skeuomorphic, high-contrast, resizable GUI for `plugin303`  
**Target:** VST3 / CLAP / Standalone  
**Reference Base File:** `plugin303/src/gui/`  

---

## 1. Overview & Objectives

The `plugin303` graphical user interface transitions from a fixed-size, skeuomorphic hardware imitation (brushed-aluminum triangle-fan meshes, specular bevel arcs, pseudo-plastic knurling, bloom blur halos) to a **modern, precision studio vector interface**.

### Key Design Pillars
1. **Wide Proportional Vector Canvas:** Base canvas of `1280px` × `720px` (16:9 widescreen) with smooth proportional scaling across all DAW host window sizes and high-DPI displays.
2. **Industrial Dark High-Contrast Palette:** Strict token system anchored by **Deep Charcoal** (`#131315`), **Safety Orange** (`#FF7A00`) for primary core controls and active states, and **Tech Cyan** (`#00F0FF`) for pitch/slide/secondary modulation.
3. **Flat, Non-Skeuomorphic Affordances:** Zero pseudo-3D bevels, faux drop-shadows, or artificial bloom glows. Every visual element represents a true parameter value, modulation state, or signal metric.
4. **Acid Euclidean Generator Integration:** Consolidated 4-column flat grid displaying pattern structure, octave behavior, note event probabilities, and variation actions.
5. **Pattern Shift Controls:** Dedicated `< SHIFT` and `> SHIFT` cyclic rotation buttons in the sequencer transport console.
6. **Zero-Lock & Zero-Allocation Safety:** All rendering executes on the UI thread via `egui::Painter` vector primitives with zero locks on the real-time audio thread.

---

## 2. Canvas Architecture & Dynamic Vector Resizing

### 2.1 Base Canvas Geometry
- **Native Resolution:** `1280px` width × `720px` height.
- **Aspect Ratio:** Fixed 16:9 vector grid.
- **Layout Budget:**
  - Sequencer step columns: `56.0px` width per step × 16 steps = `896px` step grid.
  - Sequencer transport & utility column: `352px` width.
  - Total sequencer width: `1248px` (inside `16px` outer margins).

### 2.2 Vector Scaling Engine
Dynamic scaling factor calculated on every host resize event:
$$\text{scale} = \min\left(\frac{W_{\text{host}}}{1280}, \frac{H_{\text{host}}}{720}\right)$$
The computed scale factor is applied via `egui_ctx.set_zoom_factor(scale * host_dpi_factor)`. All layout measurements remain deterministic integer/half-integer units in the base coordinate space while rendering with sub-pixel sharpness on standard, 1440p, and 4K Retina displays.

### 2.3 Manual Zoom Preset Menu
The Header Bar includes a discrete zoom dropdown menu:
- `75%` (`960px` × `540px`)
- `100%` (`1280px` × `720px` — Default)
- `125%` (`1600px` × `900px`)
- `150%` (`1920px` × `1080px`)
- `200%` (`2560px` × `1440px`)

Zoom preferences are persisted in plugin GUI state across DAW session reloads.

---

## 3. Color Hierarchy & Design Tokens

### 3.1 Base Surface Tokens
```
Base: Deep Charcoal (#131315)
Primary Accent: Safety Orange (#FF7A00)
Secondary Accent: Tech Cyan (#00F0FF)
```

| Token | Hex Color | Description |
| :--- | :--- | :--- |
| `CHASSIS_BG` | `#131315` | Master plugin frame & window margin |
| `PANEL_BG` | `#1A1A1E` | Main panel backgrounds |
| `WELL_BG` | `#161619` | Inset control wells (ADSR banks, step slots) |
| `SURFACE_ELEVATED` | `#222228` | Buttons, popovers, active controls |
| `SURFACE_HOVER` | `#2A2A33` | Interactive hover highlight |

### 3.2 Hairline Border Tokens (1.0px Crisp Strokes)
| Token | Hex Color | Description |
| :--- | :--- | :--- |
| `BORDER_DEFAULT` | `#2B2B34` | Panel outlines & module frames |
| `BORDER_SUBTLE` | `#1E1E24` | Section dividers & grid separators |
| `BORDER_FOCUS` | `#404050` | Control hover/focus ring |
| `BORDER_ACCENT` | `#FF7A00` | Active selection & engaged state |

### 3.3 Typography & Contrast Tokens
| Token | Hex Color | Description |
| :--- | :--- | :--- |
| `TEXT_PRIMARY` | `#FFFFFF` | Parameter values, section headers, active steps |
| `TEXT_SECONDARY` | `#C4C4D0` | Parameter labels, detent names |
| `TEXT_MUTED` | `#6E6E80` | Inactive step numbers, units, captions |
| `TEXT_ON_ACCENT` | `#0B0B0D` | Dark text on bright Orange/Cyan pills |

### 3.4 Functional Semantic Accents
| Token | Hex Color | Description |
| :--- | :--- | :--- |
| `ACCENT_ORANGE` | `#FF7A00` | Safety Orange: 303 Synth Core, Filter Mod, Playhead, Active |
| `ACCENT_CYAN` | `#00F0FF` | Tech Cyan: Pitch Glide, Slide, Waveform selector |
| `ACCENT_GOLD` | `#FFC83B` | Tech Gold: Sequencer Tie state |
| `ACCENT_SLATE` | `#2C2D36` | Neutral Slate: Sequencer Rest state |
| `ACCENT_RUBY` | `#FF3B56` | Signal Ruby: Clipper Saturation & Peak Warning |

---

## 4. Flat Widget Suite Specification

### 4.1 `FlatKnob` (Rotary Parameter Dial)
- **Base Structure:** Solid circular base disc (`#161619`) with 1.0px perimeter stroke (`#2B2B34`).
- **Radial Indicator:**
  - 270° sweep angle (135° start to 405° end).
  - Background track: 2.0px stroke (`#222228`).
  - Active arc: 2.5px solid stroke in `ACCENT_ORANGE` (`#FF7A00`) or `ACCENT_CYAN` (`#00F0FF`).
- **Pointer:** Crisp 1.5px solid radial line extending to inner perimeter.
- **Value Badge:** Compact dark pill tag beneath knob with crisp monospace text (`TEXT_PRIMARY` `#FFFFFF`).
- **Interaction:** Vertical drag (fine adjustment with `Shift`), double-click reset to parameter default.

### 4.2 `FlatSwitch` (Segmented Selector)
- **Base Structure:** Recessed horizontal or vertical segmented tab bar (`#161619`) with 1.0px border (`#2B2B34`).
- **Active State:** Solid pill surface (`#262630`) with vibrant indicator stripe in `ACCENT_ORANGE` (`#FF7A00`).
- **Labels:** Active detent in `TEXT_PRIMARY` (`#FFFFFF`), inactive detents in `TEXT_MUTED` (`#6E6E80`).

### 4.3 `FlatFader` (ADSR Mini-Fader)
- **Base Structure:** 6.0px wide vertical recessed slot (`#161619`).
- **Level Indicator:** Solid vertical fill bar in `ACCENT_ORANGE` (`#FF7A00`), height proportional to normalized value.
- **Thumb:** Flat 2.0px horizontal indicator bar with zero drop-shadow.

### 4.4 `StepCard` (16-Step Sequencer Column — 56.0px Width)
- **Base Card:** Rounded rectangular container (`#1A1A1E`), 1.0px border (`#2B2B34`). Active playhead highlights border to `#FF7A00`.
- **Playhead LED Bar:** Solid 4.0px flat LED strip. Unlit state: `#222228`; active running state: solid `#FF7A00` (no bloom blur).
- **Step Number:** Monospace `01`–`16` (`TEXT_MUTED`, active step `TEXT_PRIMARY`).
- **Modifier Toggles:**
  - `ACC`: Flat button. Active state fills `ACCENT_ORANGE` (`#FF7A00`) with dark text (`#0B0B0D`).
  - `SLD`: Flat button. Active state fills `ACCENT_CYAN` (`#00F0FF`) with dark text (`#0B0B0D`).
- **Gate Tile (`NOTE` / `TIE` / `REST`):**
  - `NOTE`: High-contrast `ACCENT_CYAN` pill (`#00F0FF`) with dark text.
  - `TIE`: Solid `ACCENT_GOLD` pill (`#FFC83B`) with dark text.
  - `REST`: Deep muted slate (`#262630`) with `TEXT_MUTED`.
- **Pitch Button & Octave Shift:** Displays formatted note (e.g. `C 3`, `F# 2`). Clicking opens `PianoRollPopup`. Up/Down buttons nudge octave (`-2` to `+2`).

### 4.5 Anti-Skeuomorphic Audit
The following legacy rendering artifacts are completely removed:
- `gradient_circle()` triangle-fan aluminum simulations.
- `bevel_ring()` specular highlight lines and shadow arcs.
- Multi-pass radial alpha bloom halos on LEDs.
- Faux plastic ribbing and knurled carriage lines.
- Faux 3D drop-shadows and chassis vent slits.

---

## 5. Panel Hierarchy & Layout Architecture

```
+-------------------------------------------------------------------------------------------------------------------------------+
|  TECHNEO 303    [TOPOLOGY: ACCURATE / FUN]    [FIDELITY: LOW / HIGH / ULTRA]                             [ZOOM: 100% v]       |
+-------------------------------------------------------------------------------------------------------------------------------+
| [WAVEFORM] |  (TUNING)  (CUTOFF)  (RESO)  (ENV MOD)  (DECAY)  (ACCENT)  (VOLUME)   |   [MEG ADSR]          [VEG ADSR]         |
| [  SAW   ] |                                                                       |   [A] [D] [S] [R]     [A] [D] [S] [R]    |
| [  SQR   ] |                                                                       |                                          |
| [  SWP   ] |                                                                       |                                          |
+-------------------------------------------------------------------------------------------------------------------------------+
| SOUPED-UP ENGINE & ANALOG DRAWER (Accurate Mode: 35% Opacity Dimmed Bed with Flat Non-Interactive Badge)                      |
| [VCF TAPS: 2P/3P/4P] [VCA: GATE/ENV] | (DRIVE) (SUB) (GLIDE) (PITCH1) (PITCH2) (MIX2) (PW) (SWEEP) (R-COMP)                   |
+-------------------------------------------------------------------------------------------------------------------------------+
| OUTPUT CLIPPER & ANALOG TWEAKS                                                                                                |
| [BYPASS] [OS: 2X/4X] (DRIVE) (THRESH) (BASS) (TONE) (IN) (OUT) (TILT) (DYN) (MIX) | (TOLERANCE) (AGE) (NOISE) (SEED) (DRIFT) |
+-------------------------------------------------------------------------------------------------------------------------------+
| ACID EUCLIDEAN GENERATOR                                                                         [SEED: 000000000000002A]     |
| [1. PATTERN STRUCTURE]          [2. OCTAVE BEHAVIOR]          [3. EVENT PROBABILITIES]        [4. ACTIONS & VARIATION]        |
| ROOT: [C v]  SCALE: [Penta v]   OCT MIN: [-1]  OCT MAX: [+1]  REST %:   [ 20% ]  ACC %: [ 35% ]  MUTATION %: [ 15% ]          |
| PULSES (k): [ 5]  STEPS (n):[16] OCT UP %: [ 20% ]           SLIDE %:  [ 25% ]  BIAS:  [ 50% ]  [GENERATE]  [MUTATE]         |
| SHIFT:      [ 0]  PITCH LEN:[16] OCT DN %: [ 10% ]           [x] SLD CHAIN                   [NEW SEED]                      |
+-------------------------------------------------------------------------------------------------------------------------------+
| 16-STEP SEQUENCER CONSOLE (16 x 56.0px Step Cards)                              | TRANSPORT & PATTERN UTILITIES               |
| [01] [02] [03] [04] [05] [06] [07] [08] [09] [10] [11] [12] [13] [14] [15] [16]  | [RUN / STOP]    [SYNC: HOST / FREE]         |
| (Playhead LED, ACC, SLD, Gate Pill, Pitch Readout, Octave Nudges)                | [< SHIFT]       [> SHIFT]                   |
|                                                                                 | [RANDOM PATTERN][CLEAR ALL]                 |
|                                                                                 | PATTERN: [PRESET SELECTOR v]                |
+-------------------------------------------------------------------------------------------------------------------------------+
```

### 5.1 Section Dimensions & Heights
1. **Header Bar (32.0px):** Minimal status line with vector logotype, topology switch, fidelity selector, and zoom menu.
2. **Classic Panel (144.0px):** Vertical waveform switch on the left (`64px`), 7 vintage rotary dials (`72px` spacing), and dual ADSR mini-fader banks on the right.
3. **Souped-Up Drawer (116.0px):** Full-width drawer containing filter slope selector, VCA envelope mode switch, and 9 extended sound-design dials.
4. **Output Clipper & Tweaks (108.0px):** Combined full-width subpanel housing 10 clipper controls and 5 analog component tweak dials side-by-side.
5. **Acid Euclidean Generator (110.0px):** 4-column structured parameter grid with clear action buttons.
6. **16-Step Hardware Sequencer & Transport (178.0px):** 16 step columns (`56.0px` each) paired with transport controls and pattern shift buttons.

---

## 6. Acid Euclidean Generator Specification

### 6.1 Parameter Structure
| Category | Parameter | Range / Options | Control Type | Description |
| :--- | :--- | :--- | :--- | :--- |
| **Pattern Structure** | `ROOT` | C, C#, D, D#, E, F, F#, G, G#, A, A#, B | Dropdown | Fundamental root note for pitch quantizer |
| | `SCALE` | Pentatonic, Dorian, Phrygian, Aeolian, Harmonic Minor, Chromatic | Dropdown | Musical scale for step pitch quantization |
| | `PULSES (k)` | 1 to 16 | Monospace DragValue | Euclidean active pulses distributed across steps |
| | `STEPS (n)` | 1 to 16 | Monospace DragValue | Sequence length in 16th steps |
| | `SHIFT` | 0 to 15 | Monospace DragValue | Step rotation offset for rhythmic pattern |
| | `PITCH LEN` | 1 to 16 | Monospace DragValue | Loop length for pitch sequence generation |
| **Octave Behavior** | `OCT MIN` | -2 to 0 | Monospace DragValue | Lowest allowable octave offset |
| | `OCT MAX` | 0 to +2 | Monospace DragValue | Highest allowable octave offset |
| | `OCT UP %` | 0.00 to 1.00 (0% to 100%) | Monospace DragValue | Probability of upward octave transposition |
| | `OCT DN %` | 0.00 to 1.00 (0% to 100%) | Monospace DragValue | Probability of downward octave transposition |
| **Event Probabilities** | `REST %` | 0.00 to 1.00 (0% to 100%) | Monospace DragValue | Probability an active pulse becomes a rest |
| | `ACCENT %` | 0.00 to 1.00 (0% to 100%) | Monospace DragValue | Probability a step receives an accent |
| | `SLIDE %` | 0.00 to 1.00 (0% to 100%) | Monospace DragValue | Probability a step receives a slide/glide |
| | `ACC PITCH BIAS %` | 0.00 to 1.00 (0% to 100%) | Monospace DragValue | Correlation between accents and pitch jumps |
| | `SLD CHAIN` | Off / On | Flat Checkbox | Allows consecutive slide events across steps |
| **Variation & Actions** | `MUTATION %` | 0.00 to 1.00 (0% to 100%) | Monospace DragValue | Step mutation rate during mutate passes |
| | `GENERATE` | — | Solid Safety Orange Button | Rebuilds sequence pattern from current seed |
| | `MUTATE` | — | Solid Tech Cyan Button | Varies active pattern according to mutation rate |
| | `NEW SEED` | — | Flat Surface Button | Generates new random 64-bit seed value |
| | `SEED` | 16-hex characters | Monospace Pill Readout | Read-only pattern seed display in panel header |

---

## 7. Sequencer Transport & Pattern Shift Specification

### 7.1 Pattern Shift Operations (`< SHIFT` and `> SHIFT`)
- `< SHIFT` button: Shifts all 16 sequencer steps cyclically left by 1 step ($S_0 \leftarrow S_1, S_1 \leftarrow S_2, \dots, S_{15} \leftarrow S_0$).
- `> SHIFT` button: Shifts all 16 sequencer steps cyclically right by 1 step ($S_1 \leftarrow S_0, S_2 \leftarrow S_1, \dots, S_0 \leftarrow S_{15}$).
- Both operations perform wait-free updates to `params.pattern` within a single write lock acquisition, ensuring glitch-free audio playback.

### 7.2 Transport & Clock Modes
- `RUN / STOP`: Toggles `params.seq_run`. Active running state highlights in solid `ACCENT_ORANGE` (`#FF7A00`).
- `SYNC: HOST / FREE`: Selects DAW transport sync vs internal free-running clock.
- `RANDOM PATTERN`: Generates randomized acid pattern with musical weighting.
- `CLEAR ALL`: Resets all 16 steps to baseline root note, no accent, no slide, note gate.

---

## 8. State Management & Real-Time Sync

### 8.1 Circuit Topology Modes
- `params.topology == CircuitTopology::Accurate`:
  - `SoupedUpPanel` renders with uniform `35%` alpha dimming.
  - Centered non-interactive badge displays: `ACCURATE MODE ACTIVE` / `Souped-up engine is bypassed` with a one-click `[ENABLE FUN MODE]` switch.
  - Mouse interaction with dimmed controls is disabled.
- `params.topology == CircuitTopology::Fun`:
  - `SoupedUpPanel` renders with full vibrancy, high contrast, and active interactivity.

### 8.2 Real-Time Playhead Tracking
- Sample-accurate step position is reported from the audio engine via `Arc<AtomicU8>`.
- The GUI reads the atomic step index once per frame with `Ordering::Relaxed` to update step card border highlights and solid playhead LEDs without UI thread stalls.

---

## 9. Verification & Visual QA Plan

### 9.1 Headless & Unit Test Verification
1. `cargo test -p plugin303 --no-default-features`: Headless DSP tests pass without GUI dependencies.
2. `cargo test -p plugin303 --all-features`: GUI modules compile and pass test suites.

### 9.2 Visual QA & Layout Gates
1. **DPI & Scaling Invariance:** Verify rendering across 75%, 100%, 125%, 150%, and 200% zoom levels with zero text clipping or border misalignment.
2. **Contrast & Readability Audit:** Verify text contrast ratios exceed WCAG AA standards (`> 4.5:1`) across all background surfaces.
3. **Anti-Skeuomorphic Compliance:** Confirm complete removal of triangle-fan meshes, specular bevels, and artificial bloom halos.
4. **Pattern Shift Verification:** Verify that `< SHIFT` and `> SHIFT` rotate all 16 steps cyclically without audio glitching.
