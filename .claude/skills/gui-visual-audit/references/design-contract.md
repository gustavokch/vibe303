# Design contract

The flat device-chain restyle (`56b383e` and its follow-ups through
`60a1a74`, 2026-10-17) set the editor's current design direction. It is an
improvement, not a finished design. This file records the rules it follows,
so an audit can tell two findings apart: **drift**, where the render breaks a
rule, and a **design weakness**, where a rule, or a place no rule covers,
falls short. Both are findings. A design finding that would change a rule
here names that rule; the user decides whether it changes.

The values live in code: tokens in `plugin303/src/gui/theme.rs`, layout
constants in `gui/mod.rs`, components in `gui/widgets/`. This file states the
rules those values serve and what each rule looks like in a 2x capture. When
the user changes the design, update this file in the same change.

---

## Planes

Depth comes from five neutral planes and nothing else, darkest to lightest:

| Plane | Carries |
|---|---|
| `CHASSIS_BG` | the window, the 6 px gutters between devices |
| `WELL_BG` | inset values: DragValue and ComboBox wells, fader tracks, knob value pills, pitch cells, REST gates, the step LED at rest, separators |
| `PANEL_BG` | the device body |
| `PANEL_HEADER_BG` | the device header strip, the app bar, the current-step plate while the sequencer runs |
| `SURFACE_ELEVATED` | a raised control at rest: buttons, tiles, unselected segments, the knob body |

Hover lifts a raised control to `SURFACE_HOVER`; press drops it to
`SURFACE_PRESSED`. A hovered well darkens to `SURFACE_PRESSED`.

- **No outlines, no shadows, no gradients.** Every widget state has
  `bg_stroke = NONE`. The only line in the UI is the separator: 1 pt
  `WELL_BG`.
- **Plane steps are quiet on purpose**: body on chassis 1.21:1, header strip
  on body 1.13:1, raised control on body 1.33:1. A low plane step is not
  drift. It is a design weakness only where a user cannot tell a control or a
  group from its surroundings: show that, then propose a fix. A stronger
  plane step stays in the direction; an outline leaves it.
- **Values sit in wells, actions are raised.** `theme::field_style` turns
  DragValues and ComboBoxes into `WELL_BG` wells; buttons and tiles are
  `SURFACE_ELEVATED`. A raised value field or an inset action is drift.
- A plane out of order (a well lighter than the body, a raised control
  darker than its parent) is drift.

## Colour roles

| Colour | Role, and only this role |
|---|---|
| `ACCENT_ORANGE` | live state: value arcs, fader fills, a toggle's LED when on, the selected segment, lit actions (ENABLE FUN MODE, CALIBRATE when a rebuild is due), a lit ACC step, the playhead LED. As text only in the CALIBRATE status line when a rebuild is due (`tweaks_panel.rs`) |
| `ACCENT_ORANGE_HOVER` | a hovered lit tile or toggle LED |
| `ACCENT_BLUE` | a lit SLD step. Nothing else |
| `GATE_NOTE_FILL` > `GATE_TIE_FILL` > `WELL_BG` | NOTE > TIE > REST gate brightness, neutral on purpose so orange and blue stay free |
| `TEXT_ON_ACCENT` | text on every light fill: orange, blue, NOTE |
| `TEXT_PRIMARY` | values, readouts, the knob pointer, the playing step number, the label of an on or hovered toggle |
| `TEXT_SECONDARY` | captions, labels, the text on an unlit tile, an off toggle's label at rest |
| `TEXT_MUTED` | disabled text and marks, REST text |

A third hue, orange that means something other than "live", or blue anywhere
but SLD is drift.

## Contrast

Ratios computed from the tokens:

| Foreground | `WELL_BG` | `PANEL_BG` | `PANEL_HEADER_BG` | `SURFACE_PRESSED` | `SURFACE_ELEVATED` | `SURFACE_HOVER` |
|---|---|---|---|---|---|---|
| `TEXT_PRIMARY` | 14.73 | 12.81 | 11.33 | 11.17 | 9.63 | 7.62 |
| `TEXT_SECONDARY` | 8.77 | 7.63 | 6.75 | 6.65 | 5.73 | 4.54 |
| `TEXT_MUTED` | 5.38 | 4.68 | 4.14 | 4.08 | 3.52 | 2.78 |
| `ACCENT_ORANGE` | 5.67 | 4.93 | 4.36 | 4.30 | 3.70 | 2.93 |

Light fills: `TEXT_ON_ACCENT` on orange 6.00, on blue 7.27, on NOTE 7.75;
`TEXT_PRIMARY` on TIE 5.41.

- Enabled text ≥ 4.5:1. Non-text marks (arcs, fills, pointers, arrows)
  ≥ 3:1.
- Disabled text ≥ 3:1. The widgets themselves pair `TEXT_MUTED` with
  `PANEL_BG` (4.68:1) for a disabled, unselected segment or toggle.
- Below 3:1 is a hard defect in every state. The usual cause is a plain
  egui widget (Label, DragValue, ComboBox) inside `add_enabled_ui(false)`:
  egui's fade dims it on top of its token.
  `plugin303/tests/gui_text_contrast.rs` fails on any text outside the text
  tokens.
- `TEXT_MUTED` on `SURFACE_ELEVATED` or `SURFACE_HOVER` as enabled text is
  drift. `theme.rs` names `CHASSIS_BG`, `WELL_BG` and `PANEL_BG` as its safe
  planes.

## Dimming: one mechanism

A disabled control shows itself through tokens, never through egui's fade.
The custom widgets clear the fade (`set_fade_to_color(None)`) and paint:

| Widget | Disabled paint |
|---|---|
| Segment: selected / on | `SURFACE_HOVER` fill, `TEXT_SECONDARY` text. Keeps its position, never orange |
| Segment: unselected / off | `PANEL_BG` fill, `TEXT_MUTED` text |
| Toggle: on | `PANEL_BG` tile, `TEXT_MUTED` LED and text |
| Toggle: off | `PANEL_BG` tile, `WELL_BG` LED, `TEXT_MUTED` text |
| Knob | arc, pointer and label `TEXT_MUTED`; the value stays `TEXT_PRIMARY` |
| Fader | fill `TEXT_MUTED`, value `TEXT_SECONDARY` |
| DevicePanel `.dimmed(true)` | title `TEXT_SECONDARY` |
| Readout (`widgets/readout.rs`) | always `WELL_BG` well, `TEXT_SECONDARY` 9 pt monospace; a disabled group's labels turn `TEXT_MUTED` |

Inside a disabled group, a built-in egui widget (Label, DragValue,
ComboBox, Button) is drift: egui fades it. Values become `Readout`s;
labels are painted with the fade cleared; an action the mode cannot use is
hidden.

A disabled control says why on hover with `on_disabled_hover_text`;
`on_hover_text` never shows on a disabled widget.
`plugin303/tests/gui_tooltips.rs` checks QUALITY and ANALOG TWEAKS in
ACCURATE, both ADSR banks, and the OUTPUT CLIPPER while it is off.

Disabled groups (`add_enabled_ui(false, …)`): QUALITY in ACCURATE, both ADSR
banks (MEG and VEG) unless ADV ENV is on in FUN, the ADV ENV tile in
ACCURATE, the OUTPUT CLIPPER body and header switches when it is off, and
the ANALOG TWEAKS body in ACCURATE: its fields become Readouts, its status
reads FUN MODE ONLY, and its header hides CALIBRATE. GENERATION SEED is a
Readout in every mode. In ACCURATE the SOUPED-UP ENGINE body is replaced by
a banner that says why, with an ENABLE FUN MODE action.

- Orange on a disabled control is a defect: orange claims "live".
- A disabled selection must still show which option is selected.
- Accent that disappears in a disabled state is correct. Do not report it.

## Components

| Component | Source | Contract |
|---|---|---|
| Device card | `widgets/panel.rs` `DevicePanel` | `PANEL_BG` body under a 22 pt `PANEL_HEADER_BG` strip. Title 9 pt proportional on the left; that device's own mode switches sit right-aligned in the strip (the first one added sits flush right). Body inset 8 pt x, 4 pt y |
| Segmented control | `widgets/switch.rs` `SlideSwitch`, `BoolSwitch` | one flat bar of 18 pt segments split by 1 pt panel-colour hairlines; only the outer ends are rounded. Selected = orange fill + `TEXT_ON_ACCENT`. Caption 8.5 pt, left of the bar in header strips, above it in knob columns. WAVE is the vertical variant |
| Boolean | `widgets/toggle.rs` `ToggleButton` | every on/off (RUN, SYNC, MIDI, SYNTH, JITTER, ADV ENV, LINK, ON, SLD CHAIN) is a raised tile with a 3 × 10 pt LED left of a state-free label: `WELL_BG` slot off, orange on (`ACCENT_ORANGE_HOVER` on hover). The tile never fills orange. 18 pt tall (28 pt in TRANSPORT). An egui checkbox is drift; a label that spells its state (`MIDI: OFF`) is drift |
| Knob | `widgets/knob.rs` `Knob303` | two tiers only: LARGE r 21 pt (VOICE, DRIFT, RECAL), MEDIUM r 12 pt (SOUPED-UP ENGINE, OUTPUT CLIPPER). `KNOB_TRACK` ring over the whole -135°..+135° sweep; orange arc from the minimum, or from 0 when the range's plain minimum is below 0 (TUNING, VOLUME, PITCH1/2, IN, OUT, TILT); `KNOB_BODY` disc at 0.68 r; `TEXT_PRIMARY` pointer; label 8.5 pt caps; value 8 pt monospace in an 11 pt `WELL_BG` pill, 52 / 44 pt wide by tier. The value text is the parameter's own text (`value_to_string`, the string the host shows). A knob whose meaning depends on another parameter passes `value_text()` computed from the voice's mapping (`value_text::cutoff`, `value_text::glide`) |
| Fader | `widgets/fader.rs` `MiniFader` | 12 pt `WELL_BG` track filled from the bottom in the accent; no thumb; label 8.5 pt above, value 7.5 pt monospace below |
| Value field | `theme::field_style` | DragValue: 9 pt monospace in a well. ComboBox: a well with a ▼; the TRANSPORT combos keep ▼ even when the popup opens upward. A value that cannot be edited is a `Readout`. The ANALOG TWEAKS fields share one width, `FIELD_W` = 80 pt |
| Action | egui `Button` under `theme::apply` | raised tile, 8.5 pt label. A lit action is an orange fill with `TEXT_ON_ACCENT`, and only as the call to action of a state: ENABLE FUN MODE while ACCURATE bypasses the engine, CALIBRATE when a rebuild is due. Any other action is never lit; GENERATE is raised. A destructive action takes two clicks: CLEAR relabels to CONFIRM CLEAR on `SURFACE_HOVER` for 3 s; a second click within that window acts |
| Step column | `widgets/step_button.rs` | top down: 4 pt LED bar, 11 pt step number (8.5 pt monospace), ACC \| SLD pills, gate pill, pitch well, ▲ \| ▼. Rows on a 21 pt rhythm, pills 19 pt. REST shows no pitch text and mutes ▲▼; an octave arrow at its limit is muted |
| Playhead | `step_button.rs` | while the sequencer runs, the playing step's column gets a `PANEL_HEADER_BG` plate, its 4 pt LED turns orange and its number `TEXT_PRIMARY`. Stopped: no step is marked. No frame, no outline |
| Tooltip | every control | Every control in SOUPED-UP ENGINE (in ACCURATE, the ENABLE FUN MODE action), OUTPUT CLIPPER and ANALOG TWEAKS, and every abbreviated generator field, has a hover tooltip (`plugin303/tests/gui_tooltips.rs`). A disabled control uses on_disabled_hover_text to say why |

## Type

Words are proportional; every numeric readout is monospace.

| Use | Size | Face |
|---|---|---|
| App bar identity (BASS SYNTH, MONO), ACCURATE banner title | 9.5 pt | proportional |
| Device title | 9 pt | proportional |
| Captions, labels, tile and button text | 8.5 pt | proportional |
| TRANSPORT field labels (CH:, BPM:, LEN:) | 8 pt | proportional |
| Default Body and Button text (combo values, plain labels) | 9.5 pt | proportional |
| DragValue readouts | 9 pt | monospace |
| Knob value | 8 pt | monospace |
| Fader value | 7.5 pt | monospace |
| Step number, pitch | 8.5 pt | monospace |

At 2x an 8.5 pt caption measures h = 12 px with `text --box`. A widget at
egui's 14 pt default (about 20 px) never got the theme font.

## Layout

- Window 1216 × 640 pt, 19:10 (a const assert in `gui/mod.rs`); a capture
  is 2432 × 1280 px.
- Window margin 4 pt. One gutter, 3 pt, horizontal and vertical. Every gap
  is an explicit `PANEL_GAP`; `item_spacing.y` is zeroed.
- App bar 24 pt: identity on the left, engine-wide modes on the right
  (TOPOLOGY, QUALITY); the empty middle is by design.
- Below the app bar, four rows split at one seam: left
  `round((1208 - 3) × 2/3)` = 803 pt, right 402 pt. Device edges line up down
  the whole window.
- Row heights 100, 163 (80 + 3 + 80), 102 and 231 pt. With the margins,
  the app bar and the gutters they sum to exactly 640: no overflow and no
  dead band under the last row (`plugin303/tests/gui_layout_test.rs`).
- Inside a body, banks spread across the full width, and mixed-height
  children align to the top.

Landmarks in a 2x offscreen render. In a window capture, add the title bar
height to every y. `gui_qa_measure.py panels` is the source of truth; this
table names the cards it finds.

| Device | x0, y0, x1, y1 (px) | Source (`plugin303/src/gui/`) |
|---|---|---|
| App bar | 8, 8, 2424, 56 | `panels/header_bar.rs` |
| VOICE | 8, 62, 1614, 262 | `panels/top_controls.rs` `render_voice_panel` |
| ENVELOPES | 1620, 62, 2424, 262 | `panels/top_controls.rs` `render_envelope_panel` |
| SOUPED-UP ENGINE | 8, 268, 1614, 428 | `panels/souped_panel.rs` |
| OUTPUT CLIPPER | 8, 434, 1614, 594 | `panels/output_clipper.rs` |
| ANALOG TWEAKS | 1620, 268, 2424, 594 | `panels/tweaks_panel.rs` |
| ACID EUCLIDEAN GENERATOR | 8, 600, 1614, 804 | `panels/generator_panel.rs` `render_generator_panel` |
| GENERATION | 1620, 600, 2424, 804 | `panels/generator_panel.rs` `render_generation_panel` |
| SEQUENCER | 8, 810, 1614, 1272 | `panels/sequencer_panel.rs` `render_sequencer_grid` |
| TRANSPORT & PATTERN | 1620, 810, 2424, 1272 | `panels/sequencer_panel.rs` `render_transport_panel` |

Every card's header strip is its top 44 px; its body starts 8 px below that
and 16 px in from each side.

## Deliberate in the current direction

Do not report these as drift. Critique them as design only with evidence of a
problem a user meets.

- No border or outline anywhere.
- Quiet plane steps between chassis, body, header strip and controls.
- The empty middle of the app bar.
- No accent on a disabled control.
- No pitch text on a REST step.
- No arc on a knob at its zero-length fill (a unipolar knob at its minimum,
  a bipolar knob on its centre detent).
