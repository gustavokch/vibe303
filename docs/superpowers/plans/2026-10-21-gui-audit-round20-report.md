# TECHNEO 303 — GUI Visual Audit, Round 20

Date: 2026-10-21
Captures: `target/gui-qa-offscreen` (`latest`, `accurate`, `advanced`, `running`,
`running_note`, `knobs`, `quality_low`, `generator`, `zoom75`), all 2432x1280,
offscreen render (`scripts/render_gui_offscreen.sh`).
Baseline: commit `509db21` "Merge pull request #77 from gustavokch/qa/gui-audit-flat-restyle" on `main`.

This is the first round against the flat device-chain restyle (`56b383e`) and
its design contract (`.claude/skills/gui-visual-audit/references/design-contract.md`).

## Capture set

| File | State |
|---|---|
| `latest.png` | defaults: FUN, QUALITY DRAFT, stopped |
| `accurate.png` | ACCURATE: SOUPED-UP banner, ANALOG TWEAKS and QUALITY disabled |
| `advanced.png` | ADV ENV on: MEG bank enabled |
| `running.png` | running, playhead on step 04 (REST) |
| `running_note.png` | running, every step NOTE |
| `knobs.png` | TUNING +25 ct, CUTOFF 0.75 |
| `quality_low.png` | QUALITY LOW |
| `generator.png` | every generator field off its default, SLD CHAIN on |
| `zoom75.png` | egui zoom 0.75 in the fixed window |

Pixel diffs confirm the states are distinct and correctly scoped:
`accurate` differs only in the app bar (y 14-49, x 1521-2201), the SOUPED-UP
body and both header strips of row 2 (y 272-307), and ANALOG TWEAKS
(y 324-566); `advanced` only in ENVELOPES (y 66-248, x 1637-2407);
`generator` only in the generator body and the GENERATION SEED value
(y 663-797); `knobs` only in TUNING and CUTOFF; `quality_low` only in the
QUALITY bar (x 1863-2031); `running` in SEQUENCER steps 01-04 and the RUN tile.
The step 01-03 changes in `running` are the current-step plate moving from
step 01 (stopped) to step 04: `latest` (975, 100) is `#2F2F2F`, `running` is `#262626`.

## Previous round (19)

- **1 SEED read-only value 2.84:1: FIXED as text.** `0042` now measures
  `TEXT_MUTED` 4.66:1 (`text --box 1950,655,2020,685`). Its well is still
  egui-faded to `#202020`, which no token names: carried into issue 1.
- **2 TRANSPORT row 5 breaks the grid: FIXED.** A column scan at x 2300 gives
  six rows, every one 56 px (28 pt), on 13 px gaps.
- **3 Four checkboxes, three on-states: SUPERSEDED by `56b383e`.** Every boolean
  is a `ToggleButton` tile. A new legibility problem with those tiles is issue 5.
- **4 ANALOG panel unequal columns: SUPERSEDED by `56b383e`.** The separators
  and columns are gone. The ragged field column that remains is issue 7.
- **5 ACCURATE dims ANALOG with no explanation: OPEN.** Issue 3.
- **6 Gutters `[10, 0, 51, …]`: FIXED.** `panels`: 10 cards, every gap 6 px,
  seam x 1614 | 1620, in all eight 100% states.
- **7 Field fill 1.43:1: SUPERSEDED by `56b383e`.** Values sit in `WELL_BG` wells
  by design; no border.
- **8 Sequencer bottom edge missing: SUPERSEDED by `56b383e`.** No borders exist.
  The last row ends at y 1272: an 8 px (4 pt) margin, the contract's `MARGIN`.
- **9 Playhead is a white frame: SUPERSEDED by `56b383e`.** The frame is gone. The
  plate that replaced it does not read: issue 6.
- **Flagged: `zoom75` renders the 100% layout: FIXED.** `zoom75` now lays out on a
  1621 × 853 pt canvas. That layout has its own defect: issue 2.

## Geometry

Cards match the contract's landmark table exactly in every 100% state.
`knobs`: 27 knobs (9 LARGE, 18 MEDIUM); no arc/pointer mismatch (largest
delta 2.3° / 0.9 px, PW); TUNING +25 ct fills from centre (needle 34.0°, 0.626),
CUTOFF 0.75 from min (needle 67.9°, 0.752). All 14 `arc=NONE` knobs at
defaults sit at a zero-length fill (unipolar at 0 %, bipolar on +0.0).

---

## HIGH

### 1. Disabled ANALOG TWEAKS labels fall below 3:1 in ACCURATE

**Kind.** Defect. Breaks Contrast ("below 3:1 is a hard defect in every
state") and Dimming ("a disabled control shows itself through tokens, never
through egui's fade").

**Symptom.** `accurate.png`, ANALOG TWEAKS body. `text --box`:

| Text | `latest` | `accurate` |
|---|---|---|
| TOL label | `#B5B5B5` 7.38:1 | `#6D6D6D` **2.92:1** |
| AGE label | — | `#6E6E6E` **2.97:1** |
| NOISE label | — | `#6D6D6D` **2.92:1** |
| CMP SEED label | — | `#6D6D6D` **2.92:1** |
| CALIBRATED status | `#B4B4B4` 7.30:1 | `#6D6D6D` **2.92:1** |

The labels are `TEXT_SECONDARY` (`tweaks_panel.rs:90`, status at `:236-243`)
inside `add_enabled_ui(!is_accurate, …)` (`tweaks_panel.rs:59`), so egui's fade
dims them on top of their token. The same fade paints the wells `#202020`
instead of `WELL_BG #1A1A1A`: the AGE, NOISE and CMP SEED wells in
`accurate`, and the read-only GENERATION SEED well in every state
(`0042`, bg `#202020`). The knob captions in the same body are correct
(DRIFT `TEXT_MUTED` 4.38:1): the custom widget clears the fade.

`colors` does not flag this: `#6C6C6C` snaps to `GATE_TIE_FILL`, a surface
token, and is hidden. Found with `text --box`.

**Required fix.** Every text run in a disabled group is ≥ 3:1 and uses a
token: labels `TEXT_MUTED` (4.68:1 on `PANEL_BG`), the CALIBRATED line
`TEXT_MUTED`. Wells stay `WELL_BG` when disabled. Clear the fade the way the
custom widgets do (`set_fade_to_color(None)`) and paint the tokens. Must NOT
change: the enabled palette in `latest`, DRIFT/RECAL disabled paint, the
CALIBRATE orange status when a rebuild is due.

### 2. ZOOM 75% leaves a quarter of the window empty

**Kind.** Defect (dead space, checklist B). No contract rule covers zoom; the
layout rule "no dead band under the last row" holds only at 100%.

**Symptom.** `zoom75.png`: the canvas is 1621 × 853 pt, but the last row ends
at y 954 px. The band y 954-1280 (326 px, 217 pt, 25% of the window) is bare
chassis. Widths stretch to the canvas (cards span x 6-2426), heights do not:
the row heights are constants (`gui/mod.rs:34` `VOICE_ROW_H`, `:37`
`BANK_ROW_H`, `:40` `TWEAKS_H`). `ZOOM_PRESETS` (`header_bar.rs:12`) offers
75% in a fixed 1216 × 640 pt window, and nothing resizes the window. The net
effect of 75% is smaller text (an 8.5 pt caption is 9 px tall at 1.5 px/pt)
and a black strip.

**Required fix.** At every ZOOM preset the layout fills the window with no
dead band, or the preset is removed. Two options, user's choice: resize the
editor window with the zoom factor, so 75% gives a 912 × 480 pt window; or
drop sub-100% presets. Must NOT change: the 100% layout and
`gui_layout_test.rs`.

---

## MEDIUM

### 3. ACCURATE disables ANALOG TWEAKS without saying why (carried from round 19 #5)

**Kind.** Defect (checklist D: "a bypassed or inert device that does not say
so").

**Symptom.** `accurate.png`: SOUPED-UP ENGINE says `ACCURATE MODE ACTIVE /
Souped-up engine is bypassed / ENABLE FUN MODE`. ANALOG TWEAKS only dims its
title (`TEXT_PRIMARY` 11.03:1 → `TEXT_SECONDARY` 6.60:1) and greys its body.
Its tooltips (`tweaks_panel.rs:49, 171, 192, 215`) use `on_hover_text`,
which egui does not show on a disabled widget. QUALITY does the same thing
(`header_bar.rs:109`).

**Required fix.** In ACCURATE the card states why it is inert and how to
re-enable it, in the card itself or in a disabled-hover tooltip
(`on_disabled_hover_text`) on the header and body. Must NOT change: the
SOUPED-UP banner.

### 4. Static orange outweighs live feedback

**Kind.** Design. Would change the Colour roles row that lists GENERATE as a
lit action, and adds a size rule the contract does not have.

**Symptom.** Orange pixels, `latest` vs `running`:

| Area | `latest` | `running` |
|---|---|---|
| Whole window | 119,491 | 141,755 |
| GENERATE bar (804 × 25 pt, always lit) | 31,953 (27%) | 31,953 |
| TRANSPORT (SYNC: HOST, SYNTH: ON; + STOP when running) | 41,839 (35%) | 62,977 |
| Playhead LED | 0 | **1,126 (0.8%)** |

At 1x (`latest` downscaled to 1216 × 640) the eye lands on the bottom-right
orange block first, not on the VOICE knobs or the step grid. GENERATE is lit
whether or not anything is pending, so "orange = live" reads as "orange =
big button". The live-state mark (the playhead) is the smallest orange
element in the window.

**Proposal.** Stays in the direction: draw GENERATE as a raised action like
MUTATE and NEW SEED (remove it from the lit-action list), and bring the
SYNC and SYNTH booleans down to standard 18 pt tiles, so lit area tracks
what is live. Must NOT change: the selected-segment and lit-toggle rules.

### 5. An off toggle and an action look the same

**Kind.** Design. Would change the Boolean and Action rows of Components.

**Symptom.** On the same `SURFACE_ELEVATED` tile, an off toggle (JITTER,
RUN, MIDI: OFF, ADV ENV, SLD CHAIN) differs from an action (CLEAR, < SHIFT,
NEW SEED, CALIBRATE) only by text tone: `TEXT_SECONDARY` 5.49:1 against
`TEXT_PRIMARY` 9.28:1. Same size, same fill, same radius. In TRANSPORT the
JITTER toggle and the CLEAR action sit side by side and read as a pair.
The labels add a second inconsistency: MIDI: OFF, SYNTH: ON and SYNC: HOST
spell their state; JITTER, LINK, ON, ADV ENV and SLD CHAIN do not.

**Proposal.** One naming rule for booleans (state-free noun, the fill shows
the state) and one shape cue that marks a tile as a toggle. A cue such as a
small `WELL_BG` indicator slot in the off tile stays in the direction; an
outline would leave it. User decides.

### 6. The playhead plate does not read

**Kind.** Design. Would change the Playhead row of Components.

**Symptom.** `running.png` / `running_note.png`, step 04. The plate is
`PANEL_HEADER_BG` on `PANEL_BG`, 1.13:1, and shows only in the 4-6 px
seams between the step's tiles. In `running_note`, where every gate is
NOTE, the playing column differs from its neighbours by the LED
(`strokes`: one 6 px orange bar, 3 pt; 3 px at 1x) and the number tone
(7.63:1 → 11.33:1). The plate also marks step 01 while stopped, where nothing
else marks it, and there it cannot be seen at all.

**Proposal.** In the direction: drop the plate, or raise it to a plane that
reads against the body (≥ 1.3:1) while staying below `SURFACE_ELEVATED`
(a new token between `PANEL_HEADER_BG` and `SURFACE_ELEVATED`). Make the
LED the full step-column width and 4 pt tall. Must NOT change: no frame,
no outline.

---

## LOW

7. **ANALOG TWEAKS field column is ragged.** Drift (checklist G: rows that
   do not share an edge). `edges --box 1760,330,2000,530` right edges: TOL
   1915, AGE 1842, NOISE 1809, CMP SEED 1831 px: four widths in one column.
8. **The ACCURATE banner title is 10.5 pt.** Drift (Type: no row matches).
   `souped_panel.rs:43`; `text --box` h = 15 px against 12 px for a 9 pt
   device title. Either add a banner row to the type table or use 9.5 pt.
9. **CLEAR reads like a benign action.** Design (checklist I). It resets all
   16 steps (`sequencer_panel.rs:576`) and looks exactly like < SHIFT and
   JITTER beside it.
10. **ADV ENV does not say what it enables.** Design. The MEG bank greys out
    with no link to the tile in the header strip; no disabled-hover tooltip
    on the bank (`top_controls.rs:156`).
11. **Abbreviated controls without tooltips.** Design (checklist I).
    `knob.rs` has no `on_hover_text`, so MIX2, R-COMP, PW, SWEEP, DYN, TILT,
    RECAL have none; `generator_panel.rs` has none for PULSES (k), STEPS (n),
    ACC BIAS, SLD CHAIN; DIODE SI | GE has none.
12. **Words in a monospace well.** Drift (Type: "a word in monospace"). OCT
    RANGE shows `MIN -1` and `MAX +1` in the 9 pt monospace DragValue face.

---

## Flagged, intent unclear — not asserted as defects

- **Knob value text is derived from the label, not the parameter.**
  `knob.rs:194-250` picks a formatter by substring (`contains("IN")`,
  `contains("MIX")`, …). CUTOFF shows `300 · 50^f` Hz (`2.1 kHz` at 0.5,
  `5.6 kHz` at 0.75), but `lib.rs:295` passes the raw 0-1 value to the voice
  and the parameter has no `value_to_string`, so the host shows a different
  string. Confirm the Hz mapping matches the DSP; if not, the readout is a
  value/position defect.
- **STOP is lit while running.** The label names the action, the fill names
  the state. Consistent with orange = live; confirm the intent.
- **TRANSPORT labels CH:, BPM:, LEN: measure `#A6A6A6` / `#ADADAD`
  (6.2-6.7:1).** The 8 pt glyph cores never reach `TEXT_SECONDARY`; not
  drift.
- **`zoom75` SEQUENCER greys at 2.76-2.96:1 in `colors`.** Antialiased edges.
  The glyphs read REST 4.72:1, the muted arrow 3.52:1, step numbers 7.63:1.

## Capture gaps for the next round

- Hover, press, keyboard focus; disabled-hover tooltips (would prove issues 3
  and 10).
- Open ComboBox and piano-roll popups.
- SQR and SWP waves; TIE under the playhead.
- CALIBRATE when a rebuild is due (the only orange text).
- SYNTH MUTE, MIDI ON, LINK off, OUTPUT CLIPPER off (its disabled header and
  body never render).
- QUALITY HIGH and BEST; ADV ENV in ACCURATE.
- A window capture on a 1x (non-Retina) display: 7.5 pt fader values and
  8 pt knob values are 7.5-8 px tall there.
- A window capture at ZOOM 75%, to check the combo label (offscreen reads
  `100%` by harness design).

## Most urgent

1. **#1**: the only text below 3:1 in the app, in a shipped mode.
2. **#2**: a user-selectable setting that blanks a quarter of the window.
3. **#3**: carried for a second round; ACCURATE users cannot tell why the card is dead.
4. **#4**: orange no longer signals live state, and the playhead loses the fight.
5. **#6**: the current step is unmarked while stopped.
