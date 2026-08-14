# vibe303 GUI Frontend Specification (`nih_plug_egui`)

**Date:** 2026-08-13  
**Status:** Approved  
**Topic:** Graphical User Interface implementation for `plugin303` using `nih_plug_egui`  
**Target:** VST3 / CLAP / Standalone  

---

## 1. Overview & Goals

The `vibe303` plugin requires a rich, hardware-inspired graphical user interface that exposes all synthesis and sequencing controls across a unified, responsive console. The GUI is built using `nih_plug_egui` with custom procedural vector rendering (knobs, sliders, backlit buttons, LEDs, and metallic panels) to ensure zero external image asset dependencies, crisp HiDPI rendering, and responsive DAW automation bindings.

### Key Objectives
1. **Classic 303 Front Panel:** 7 vintage brushed-aluminum rotary dials (Tuning, Cutoff, Resonance, Env Mod, Decay, Accent, Volume), Waveform slide switch (Saw, Square, Sweep), and Fidelity Tier selector (Low, High, Ultra).
2. **Souped-Up Engine Drawer:** Extended dual oscillator controls, pulse width, filter slope taps (2-Pole, 3-Pole, 4-Pole), drive, glide, analog wear/drift dials, and MEG/VEG ADSR faders.
3. **Accurate vs. Fun Topology Interactivity:** Visually dimmed/ghosted (~30% opacity with non-interactive overlay badge) when in `Accurate` mode; fully vibrant and interactive in `Fun` mode.
4. **16-Step Hardware Sequencer Console:** Interactive step strips supporting note/tie/rest cycling, pitch transposing, mini keyboard popover, octave shifts, dedicated accent/slide toggles, and live playhead tracking.
5. **Zero-Lock & Zero-Allocation Safety:** GUI runs entirely on the UI thread without blocking locks or audio thread allocations.

---

## 2. Architecture & Crate Structure

### 2.1 Cargo Dependencies & Feature Gating
In `plugin303/Cargo.toml`:
```toml
[dependencies]
nih_plug = { git = "https://github.com/robbert-vdh/nih-plug.git", rev = "f36931f7af4646065488a9845d8f8c2f95252c23", features = ["vst3"] }
nih_plug_egui = { git = "https://github.com/robbert-vdh/nih-plug.git", rev = "f36931f7af4646065488a9845d8f8c2f95252c23", optional = true }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0.150"

[features]
default = ["gui", "standalone"]
standalone = ["nih_plug/standalone"]
gui = ["dep:nih_plug_egui"]
```

### 2.2 Module Layout
```
plugin303/src/
  gui/
    mod.rs               # Editor creation, window sizing, master layout grid
    theme.rs             # Color tokens, fonts, strokes, procedural gradients
    widgets/
      mod.rs
      knob.rs            # Knob303: procedural rotary dial with DAW automation
      switch.rs          # Multi-position slider & toggle switches
      step_button.rs     # Sequencer step column with modifiers, gate, and pitch
      led.rs             # Glowing LED indicators with bloom halos
      mini_keyboard.rs   # 13-key popover chromatic keyboard
    panels/
      mod.rs
      header_bar.rs      # Top branding, topology switch, master volume
      classic_panel.rs   # Left 7-knob vintage 303 controls & tier selector
      souped_panel.rs    # Right extended drawer with accurate-mode dimming
      sequencer_panel.rs # Bottom 16-step grid & transport controls
```

---

## 3. UI Components & Interaction Specification

### 3.1 `Knob303` (Procedural Rotary Dial)
- **Geometry & Rendering:**
  - Outer bezel ring with radial tick marks spanning a 270° sweep (from 135° to 405° / 7 o'clock to 5 o'clock).
  - Cylindrical brushed-aluminum knob disc with top bevel and an engraved pointer line.
  - Parameter label above (bold uppercase) and live formatted value readout below.
- **Gesture Handling & DAW Automation:**
  - Vertical drag gesture with fine-tune modifier (`Shift` key reduces speed 5×).
  - Double-click snaps back to default parameter value.
  - Automatically wraps parameter modifications in `setter.begin_set_parameter()` and `setter.end_set_parameter()` for DAW automation consistency.

### 3.2 `SlideSwitch` & `MiniFader`
- **Slide Switches:**
  - Used for `Waveform` (`Saw`, `Square`, `Sweep`), `Tier` (`Low`, `High`, `Ultra`), `VcfTaps` (`2-Pole`, `3-Pole`, `4-Pole`), `VcaEnvMode` (`Gate`, `Env`), and `Topology` (`Accurate`, `Fun`).
  - Rendered as a recessed dark track with a knurled silver sliding carriage that snaps to distinct position detents.
- **Vertical Mini-Faders:**
  - Vertical tracks with glowing orange fill bars for `MEG` (Attack, Decay, Sustain, Release) and `VEG` (Attack, Decay, Sustain, Release).

### 3.3 `StepButton` & 16-Step Sequencer Strip
- Each of the 16 step columns contains:
  1. **Playhead Indicator:** Top LED bar that illuminates in neon coral/red when the step is currently playing.
  2. **Accent Toggle (`ACC`):** Button with amber LED dot toggling step accent.
  3. **Slide Toggle (`SLD`):** Button with cyan LED dot toggling step pitch glide.
  4. **Gate Tile (`NOTE` / `TIE` / `REST`):**
     - Clicking cycles through `Note` (Cyan/Emerald `#00e5ff`), `Tie` (Gold `#ffd700`), and `Rest` (Dim Slate `#333a42`).
  5. **Pitch Readout & Nudge:**
     - Displays formatted note name (e.g. `C 3`, `F# 2`).
     - Clicking note opens `MiniKeyboard` popover for single-click pitch assignment.
     - Up/Down arrows (`▲` / `▼`) nudge octave up or down (`-2` to `+2`).

### 3.4 `PianoRollPopup` (Multi-Octave Note Selection Dialog)
- Renders an anchored, foreground overlay popover (`egui::Area` on `Order::Foreground`) above or below the selected sequencer step.
- Displays a 61-row vertical scrollable keyboard spanning 5 octaves (C1 through C6 / octaves -2 to +2).
- Features auto-centering scroll initialization on the current note, distinct white/black key styling with tactile borders, and single-click note + octave assignment with immediate dismissal on click-away or `Escape`.

---

## 4. State Management & Real-Time Sync

### 4.1 Topology Switching (`Accurate` vs `Fun`)
- When `params.topology` is `CircuitTopology::Accurate`:
  - `SoupedUpPanel` is rendered at 30% opacity with input interactions disabled.
  - An overlay banner indicates *"Accurate Mode Active (Souped-up Engine Bypassed)"* with a quick-switch button to enable `Fun` mode.
- When `params.topology` is `CircuitTopology::Fun`:
  - `SoupedUpPanel` is rendered with full vibrancy and interactivity.

### 4.2 Real-Time Playhead Tracking
- An `Arc<AtomicU8>` representing `current_playing_step` (0..15, or 255 if stopped) is maintained on `Plugin303`.
- `Sequencer::process_block()` updates the atomic step index sample-accurately using `Ordering::Relaxed`.
- The GUI reads `current_playing_step.load(Ordering::Relaxed)` every frame to drive the playhead LED and border pulse.

### 4.3 Pattern Synchronization
- Pattern edits in the GUI acquire the `RwLock::write()` on `params.pattern`.
- The audio engine continues using its wait-free `try_read()` cache in `process()`, ensuring zero lock contention or audio glitches.

---

## 5. Visual Palette & Sizing

- **Window Dimensions:** `1080px` width × `640px` height (fixed aspect ratio, vector auto-scaled with DPI).
- **Color Tokens:**
  - **Chassis Background:** `#16181b` (Dark industrial brushed steel)
  - **Panel Borders & Insets:** `#2a2e33` (Bevelled metallic lines)
  - **Classic Knobs:** Brushed aluminum (`#d0d4d8` gradient to `#888e96`)
  - **Accent Amber Glow:** `#ffaa00`
  - **Slide Cyan Glow:** `#00e5ff`
  - **Playhead Neon Coral:** `#ff3355`
  - **Souped-Up Orange:** `#ff8800`

---

## 6. Verification Plan

1. **Cargo Build Matrix:**
   - `cargo test -p plugin303 --no-default-features`: Headless DSP tests pass without GUI dependencies.
   - `cargo test -p plugin303 --all-features`: GUI modules compile and pass tests.
2. **Audio Path Allocation Gate:**
   - `cargo test -p plugin303 --test no_alloc`: Verifies audio loop remains 100% allocation-free with GUI enabled.
3. **Interactive & DAW Smoke-Test:**
   - Run `cargo xtask bundle plugin303 --release` and launch the VST3/CLAP in a DAW host.
   - Test parameter dragging, fine-tuning (`Shift`), double-click resets, pattern step editing, and playhead LED animation.
