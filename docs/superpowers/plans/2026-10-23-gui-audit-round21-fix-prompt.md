# TECHNEO 303 — GUI remediation prompt, round 21

Source audit: `docs/superpowers/plans/2026-10-23-gui-audit-round21-report.md`.
Issue numbers below match that report exactly. Keep them stable in your
confirmation.

The user selected all four design findings (#1, #4, #8, #9) for this
round. Each one changes a rule in the design contract. Update
`.claude/skills/gui-visual-audit/references/design-contract.md` in the same
change.

---

## Context

The editor is an egui UI (nih_plug_egui) in `plugin303/src/gui/`. Tokens are in
`theme.rs`, layout in `mod.rs`, components in `widgets/`, and devices in
`panels/`. The window is 1216 x 640 pt. The captures are 2432 x 1280 physical
px (2x), from `target/gui-qa-offscreen` (`bash scripts/render_gui_offscreen.sh`).
All coordinates in this prompt are physical pixels in those captures. The
baseline is commit `da9e6ed` on `fix/gui-audit-round20`.

### Devices, top to bottom (left | right)

1. **App bar** (x 8-2424, y 8-56): identity on the left; TOPOLOGY
   (ACCURATE | FUN) and QUALITY (DRAFT | LOW | HIGH | BEST) on the right.
   Source: `panels/header_bar.rs`.
2. **VOICE** (x 8-1614, y 62-262): WAVE switch and seven LARGE knobs.
   **ENVELOPES** (x 1620-2424, y 62-262): ADV ENV toggle in its header and two
   ADSR fader banks. Source: `panels/top_controls.rs`.
3. **SOUPED-UP ENGINE** (x 8-1614, y 268-428): VCF TAPS and VCA ENV switches in
   its header and nine MEDIUM knobs (DRIVE … R-COMP). In ACCURATE the body is
   a banner. Source: `panels/souped_panel.rs`.
   **OUTPUT CLIPPER** (x 8-1614, y 434-594): LINK, DIODE, OS and ON in its
   header and nine MEDIUM knobs (DRIVE … MIX). Source:
   `panels/output_clipper.rs`.
   **ANALOG TWEAKS** (x 1620-2424, y 268-594): CALIBRATE, the TOL, AGE, NOISE
   and CMP SEED fields, and the DRIFT and RECAL knobs. Source:
   `panels/tweaks_panel.rs`.
4. **ACID EUCLIDEAN GENERATOR** (x 8-1614, y 600-804): NEW SEED in its header,
   and three groups (PATTERN STRUCTURE, OCTAVE BEHAVIOR, PROBABILITIES) split
   by separators at x 545 and about 1080. **GENERATION** (x 1620-2424,
   y 600-804): MUTATION and the SEED readout in row 1, then GENERATE and
   MUTATE. Source: `panels/generator_panel.rs` (`render_generator_panel`,
   `render_generation_panel`).
5. **SEQUENCER** (x 8-1614, y 810-1272): 16 step columns.
   **TRANSPORT & PATTERN** (x 1620-2424, y 810-1272): six 28 pt rows in two
   slots:
   - RUN | SYNC
   - MIDI, CH: | SYNTH
   - < SHIFT | > SHIFT
   - BPM: | LEN:
   - the timebase combo | the mode combo
   - JITTER | CLEAR

   Source: `panels/sequencer_panel.rs`.

---

## Design contract

The flat device-chain design is the current direction, not a finished design.
Restore its rules where the render drifted. Change a rule only where a design
issue below says so (the user selected #1, #4, #8 and #9). Update the
contract in the same change. These are the rules that apply to these fixes:

- **Contrast:** enabled text ≥ 4.5:1. Disabled text ≥ 3:1. Non-text marks
  ≥ 3:1.
- **Planes:** depth comes from the neutral planes only (`CHASSIS_BG`,
  `WELL_BG`, `PANEL_BG`, `PANEL_HEADER_BG`, `SURFACE_ELEVATED`). Values sit in
  wells. Actions are raised (`SURFACE_ELEVATED`).
- **No outlines:** no stroke, outline, shadow or gradient. Every widget state
  has `bg_stroke = NONE`. The only line is the 1 pt `WELL_BG` separator.
- **Dimming:** a disabled control shows itself through tokens, never through
  egui's fade. The selected option of a disabled control must still read.
  Orange never appears on a disabled control.
- **Type:** words are proportional, and every numeric readout is monospace.
  - DragValue readouts: 9 pt monospace.
  - Combo values: 9.5 pt proportional.
  - Captions and button text: 8.5 pt.
  - TRANSPORT field labels: 8 pt.
  - Knob values: 8 pt monospace in a `WELL_BG` pill.
- **Knob:** the value text is the parameter's own text (`value_to_string`) or
  `value_text()` for CUTOFF and GLIDE.
- **Action:** a raised tile with an 8.5 pt label. The only lit actions are
  ENABLE FUN MODE in ACCURATE and CALIBRATE when a rebuild is due.
- **Layout:** banks spread across the full body width. Mixed-height children
  align to the top.
- No new hue and no new token, unless an issue asks for one. Reuse the
  existing widgets and tokens.

## Guardrails

- **Presentation layer only.** Do not change parameter IDs, ranges,
  defaults, `value_to_string` output, or persistence. A control that reads
  `X` must still read `X`. There is one exception: issue #10 changes how the
  OCT RANGE wells format zero.
- **Behaviour stays the same.** NEW SEED keeps its exact effect
  (`gen_params.seed.wrapping_add(0x9E37_79B9_7F4A_7C15)`). The two TRANSPORT
  combos keep their options and their option text.
- Do not add, remove or reorder controls, except where an issue asks for it:
  - #1 adds two captions.
  - #8 moves NEW SEED.
  - #9 renames one knob label.
- Do not regress any earlier fix. Each of these must still hold:
  - No text in any state is below 3:1, and `tests/gui_text_contrast.rs`
    passes.
  - Every boolean is a `ToggleButton` with an LED slot and a state-free
    label. No tile ever fills orange.
  - GENERATE and MUTATE are raised and not orange.
  - The playhead plate shows only while running. The playhead LED is one
    8 px orange bar across the full width of the playing column.
  - CLEAR acts only on a second click within 3 s
    (`tests/gui_clear_confirm.rs`).
  - Knob readouts are the parameter's own text.
  - The ANALOG TWEAKS fields share one width (`FIELD_W` = 80 pt), and their
    right edges sit at x 1915.
  - The app bar has no ZOOM control, and keyboard zoom stays off
    (`gui/mod.rs:171-172`).
  - Every header-strip control is 36 px tall and centred in the 44 px strip.
  - `tests/gui_tooltips.rs` passes. Update it where #1 and #8 change the
    controls it lists, but do not delete any case.
- Prefer fixing the shared widget or token over patching each call site,
  except where an issue names specific call sites.
- Keep `cargo fmt` and `cargo clippy` clean. `cargo test -p plugin303 --release`
  must pass. This includes `tests/gui_layout_test.rs` (window filled, no
  overflow, transport seam) and the theme and widget unit tests. Run the GUI
  tests with `--features gui`.

---

## MEDIUM

### 1. Two TRANSPORT combos have no caption and no tooltip (design, selected)

**Symptom.** TRANSPORT row 5 (`latest.png`, y 1140-1190) shows two full-slot
wells, `16th ▼` (x ≈1636-2017) and `Direct ▼` (x ≈2026-2405). They have no
caption. Every other TRANSPORT field has one (CH:, BPM:, LEN:). They also
have no hover text (`sequencer_panel.rs:418` `seq_tb_combo`, `:471`
`seq_mode_combo`). These are what the controls set:

- **Timebase** (`params.seq_timebase`): `16th` is straight 16th steps.
  `12th` is triplet 12th steps (`Timebase` in `params.rs:92`).
- **Mode** (`params.seq_mode`):
  - `Direct`: each step plays the pitch stored on that step.
  - `Decoupled`: the TB-303 pitch/time split. The NOTE steps take the stored
    pitches in order, so a TIE or a REST does not use a pitch
    (`SequencerPattern::compile`, `params.rs:195-235`).

**Required fix.**

- Each combo has a caption on its left, in the TRANSPORT label style (8 pt
  proportional, the same tone and colon form as `BPM:`). The suggested
  captions are `RATE:` for the timebase and `MODE:` for the mode. Any short
  caption that names the setting is acceptable.
- Each combo has a hover tooltip that names both options and what they do,
  as described above.
- Inside each slot, caption plus combo fill the slot, as BPM: and LEN: fill
  theirs.
- Must NOT change:
  - the row height (28 pt);
  - the two-slot grid and the 3 pt slot gap;
  - the option text (`16th` / `12th`, `Direct` / `Decoupled`);
  - the ▼ icon painter.

**Contract update.**

- Components, Tooltip row: add the two TRANSPORT combos to the list.
- Type table: add the new captions to the TRANSPORT field labels row.

**Acceptance criteria.**

- In `latest.png`, both captions read at ≥ 4.5:1 (`text --box`). They share
  the left edges of BPM: (x 1637) and LEN: (x 2025) to within 2 px.
- `gui_tooltips.rs` has a case that hovers each combo and finds its tooltip.

### 2. MEDIUM knob pills are narrower than their longest values (defect)

**Symptom.** The MEDIUM pill is `KNOB_PILL_WIDTH_MEDIUM` = 38 pt (76 px;
`theme.rs:61`). The value is 8 pt monospace and is not clipped to the pill
(`knob.rs:214-230`). These are widths from egui's font layout over each
parameter's full range:

| Knob | Widest value | Width | Pill |
|---|---|---|---|
| PITCH1, PITCH2 (SOUPED-UP) | `-24.0 st` | 39.7 pt | 38 pt |
| IN, OUT (OUTPUT CLIPPER) | `-18.0 dB` | 39.7 pt | 38 pt |
| TILT, DRIVE (engine) | `-6.0 dB`, `10.0 dB` | 34.7 pt | 38 pt |
| TUNING (LARGE) | `-100.0 ct` | 44.7 pt | 52 pt |

Every 8-character value is wider than its pill: on PITCH1/2 that is any value
of ±10 st or more, on IN/OUT any value of ±10 dB or more. In the capture,
`+25.0 ct` inks 77 px.

**Required fix.**

- Every value of every MEDIUM knob fits inside its pill with at least 1 pt
  of padding on each side. Do this by widening the pill (≥ 42 pt), not by
  shortening the value text.
- The pill stays inside the knob's allocated rect. If `Knob303::size` must
  grow, the 9-knob rows must still spread evenly.
- Must NOT change:
  - the two knob tiers;
  - the 8 pt monospace face;
  - the LARGE pill (52 pt);
  - the value strings;
  - the knob centres in both MEDIUM rows: x 110.9, 285.8, 460.7, 635.6,
    810.4, 985.3, 1160.2, 1335.1, 1509.9, each within ±1 px.

**Contract update.** Components, Knob row: set the pill width for MEDIUM to
the new value.

**Acceptance criteria.**

- In the new `knob_extremes` capture (see Verification), every MEDIUM value
  inks at least 2 px inside each end of its pill.
- `knobs` reports 27 knobs and no arc/pointer mismatch.

### 3. The ROOT/SCALE row is off the column grid and moves with ROOT (drift)

**Symptom.** In PATTERN STRUCTURE (`generator_panel.rs:85-146`):

| Element | `latest` (ROOT C, Pentatonic) | `generator` (ROOT D#, Phrygian) | Rows below |
|---|---|---|---|
| ROOT well | x 125-206 | x 125-215 | PULSES / SHIFT wells x 125-190 |
| SCALE: caption left | x 217 | x 226 | STEPS (n): x 201, PITCH LEN: x 202 |
| SCALE well | x 317-473 | x 326-465 | STEPS / PITCH LEN wells from x 301 |

`ComboBox::width(42.0)` and `.width(66.0)` act as minimums: the well grows
with the selected text. ROOT has 12 values (C … B, with five sharps).
SCALE has six: Pentatonic, Dorian, Phrygian, Aeolian, Harmonic, Chromatic.

**Required fix.**

- ROOT and SCALE have fixed widths that hold every value without
  truncation.
- The ROOT row lines up with the rows below it:
  - The ROOT well's right edge is the right edge of the PULSES and SHIFT
    wells.
  - The SCALE: caption starts on the STEPS (n): / PITCH LEN: column.
  - The SCALE well starts on the STEPS / PITCH LEN well column.
- Must NOT change:
  - the three-group layout and the separator at x 545 (the SCALE well
    must end left of it, with at least 4 pt to spare);
  - the 9.5 pt combo face;
  - the ▼ icons;
  - the option lists.

**Acceptance criteria.**

- In `latest.png` and `generator.png`, every one of these x positions is the
  same (0 px difference): the ROOT well, the SCALE: caption and the SCALE
  well.
- Each one matches its column partner in the rows below within 2 px.
- A throwaway check, or a unit test, shows that the widest ROOT value (`C#`)
  and the widest scale name fit with the ▼ in their fixed widths.

---

## LOW

4. **A disabled segmented bar (and a disabled toggle) disappears into a header
   strip (design, selected).** Disabled unselected segments are
   `SURFACE_PRESSED` `#303030` (`switch.rs:83`). The disabled toggle tile has
   the same fill (`toggle.rs:62`). Disabled segments and toggles only appear
   in header strips and the app bar, where the plane is `PANEL_HEADER_BG`
   `#2F2F2F`: 1.01:1. The same cause affects these controls:

   | Control | State | Measured |
   |---|---|---|
   | QUALITY LOW / HIGH / BEST | `accurate` | tile `#303030` on app bar `#2F2F2F` |
   | ADV ENV tile | `accurate` | tile `#303030` on header `#2F2F2F` |
   | LINK, DIODE, OS | clipper off | not captured (see Verification) |

   **Required outcome.** The disabled unselected segment and the disabled
   toggle tile are filled `PANEL_BG`. That is 1.13:1 against
   `PANEL_HEADER_BG`, the same step as body against header strip. The 1 pt
   slot gaps between segments stay visible as header-plane hairlines. These
   stay as they are:
   - the disabled selected segment (`SURFACE_HOVER`, `TEXT_SECONDARY`);
   - the `TEXT_MUTED` labels (≥ 3:1 on `PANEL_BG`: 4.68:1);
   - the off LED slot (`WELL_BG`);
   - every enabled state.

   No stroke. Update the Dimming table rows "Segment: unselected / off",
   "Toggle: on" and "Toggle: off" in the contract.

   **Acceptance.** In `accurate.png`, `PANEL_BG` fills the LOW, HIGH and BEST
   segments and the ADV ENV tile. In the new `clipper_off` capture, the same
   holds for LINK and for the unselected DIODE and OS segments, and the
   selected DIODE and OS segments read as `SURFACE_HOVER` with no orange.
5. **Combo values in three treatments (drift, Type).**
   - TOL shows a number in 8.5 pt proportional, left-aligned
     (`tweaks_panel.rs:151-156`), in a column of centred 9 pt monospace
     DragValues. In ACCURATE it becomes a centred monospace Readout.
   - The selected text of the TRANSPORT combos uses 8.5 pt
     (`sequencer_panel.rs:425`, `:480`).
   - ROOT and SCALE use the contract's 9.5 pt.

   **Required outcome.**
   - The TOL selected value is 9 pt monospace, centred in its 80 pt well. Its
     x matches the ACCURATE Readout within 2 px, and the ▼ stays.
   - The TRANSPORT combo values are 9.5 pt proportional, like ROOT and SCALE.
   - Popup item text may stay as it is.
6. **The GENERATE and MUTATE labels are left-aligned (drift, Action).** The
   GENERATE text starts at x 1653, 17 px into its tile (x 1636-2408)
   (`generator_panel.rs:386-391`, `:427`). **Required outcome.** Both labels
   are centred: label centre within 2 px of tile centre, as for CLEAR, < SHIFT
   and NEW SEED. Must NOT change: the tile height (22 pt), the raised fill and
   the full width.
7. **The contract gives no token for action labels (contract gap).** The 11
   unlit actions set `TEXT_PRIMARY` (for example `generator_panel.rs:389`,
   `sequencer_panel.rs:556`, and CALIBRATE at rest at `tweaks_panel.rs:46-50`).
   **Required outcome.** Contract text only, no code change:
   - Colour roles, `TEXT_PRIMARY` row: add "the label of an action at rest".
   - `TEXT_SECONDARY` row: change "the text on an unlit tile" to "the text on
     an unlit step pill".
8. **NEW SEED and the SEED it changes are in different cards (design,
   selected).** NEW SEED is in the ACID EUCLIDEAN GENERATOR header
   (`generator_panel.rs:57-65`, x 1504-1580). The SEED Readout is in
   GENERATION row 1 (`:376-378`, x 1990-2032). **Required outcome.**
   - NEW SEED is a raised action in GENERATION row 1, directly right of the
     SEED Readout, `G_ROW_H` tall, with a centred 8.5 pt `TEXT_PRIMARY`
     label.
   - The ACID EUCLIDEAN GENERATOR header strip holds no controls.
   - The behaviour is unchanged (see Guardrails).
   - Must NOT change:
     - the MUTATION and SEED positions in row 1 (x within 2 px);
     - GENERATE and MUTATE;
     - the SEED Readout (it stays a Readout).
   - Move the NEW SEED case in `gui_tooltips.rs:244` to the GENERATION
     layout list.
9. **Two knobs are both labelled DRIVE (design, selected).** The SOUPED-UP
   DRIVE (`souped_panel.rs:136`, `0.0 dB`, tooltip "VCF input drive (dB)")
   and the OUTPUT CLIPPER DRIVE (`output_clipper.rs:93`, `50%`) sit at the
   same x, one above the other. **Required outcome.** The SOUPED-UP knob reads
   `VCF DRIVE`. The OUTPUT CLIPPER knob keeps `DRIVE`, because its card names
   it. The label is centred on the knob and does not overlap its neighbours'
   labels (SUB is 87.5 pt away). Both tooltips stay, and the parameter name
   is unchanged.
10. **The sign on zero is not consistent (drift, Type).** `fmt_octave`
    (`generator_panel.rs:43-50`) turns 0 into `0`, while +1 is `+1`, and the
    knobs show zero as `+0.0` (`value_text::signed`). **Required outcome.**
    The OCT RANGE wells show zero as `+0`. Negative values stay `-n` and
    positive values stay `+n`. Only the display string changes; the stored
    value and the DragValue range stay the same.

---

## Verification

Re-render the full set with `bash scripts/render_gui_offscreen.sh` and
confirm that every capture is 2432 px wide.

**States to capture:**

1. `latest`, `accurate`, `advanced`, `running`, `running_note`, `knobs`,
   `quality_low`, `generator`: the baseline set.
2. **New: `knob_extremes`.** Set these values: PITCH1 -24.0 st, PITCH2
   +24.0 st, IN -18.0 dB, OUT +18.0 dB, TILT -6.0 dB, TUNING -100.0 ct.
   Without this state, #2 cannot be checked.
3. **New: `clipper_off`.** Set `clipper_bypass` to true. This disables the
   OUTPUT CLIPPER header (LINK, DIODE, OS) and its body. Without this state,
   #4 cannot be checked on the clipper bar.

Add each new state in all of these places, and keep them in sync:
- `QaState` and `QaState::parse` (`plugin303/src/params.rs:422-449`), plus
  its defaults (`params.rs:476-505`);
- the `STATES` table in `tools/gui_render/src/main.rs:29`;
- `scripts/capture_standalone_gui.sh`;
- `scripts/crop_gui_sections.py`;
- `tests/test_crop_gui_sections.py`;
- a defaults test in `plugin303/tests/gui_build.rs`, like
  `test_qa_knobs_state`;
- the capture table in `.claude/skills/gui-visual-audit/SKILL.md`;
- the expected-diff table in `references/measurement-recipes.md`:
  - `knob_extremes` changes only those six knobs;
  - `clipper_off` changes only the OUTPUT CLIPPER card.

**Per-issue confirmation.** Report each issue number with what you changed and
the measurement that proves it. If you did not fix an issue, say so and give
the reason:

- **1**: the caption x and contrast in `latest.png`, and the
  `gui_tooltips.rs` case names.
- **2**: in `knob_extremes.png`, the ink extent of `-24.0 st`, `+24.0 st`,
  `-18.0 dB` and `+18.0 dB` against their pill extents (≥ 2 px inside each
  end), and the MEDIUM knob centres (±1 px of the list in #2).
- **3**: in `latest.png` and `generator.png`, the x of the ROOT well right
  edge, the SCALE: caption and the SCALE well, against PULSES, STEPS (n): and
  the STEPS well.
- **4**: in `accurate.png` and `clipper_off.png`, the fill under LOW, HIGH,
  BEST, ADV ENV, LINK and the unselected DIODE and OS segments (`#262626`);
  the label contrast (≥ 3:1); and an accent count of 0 for the disabled
  controls.
- **5**: TOL `text --box` (monospace, centred) in `latest` and `accurate`,
  and the glyph height of the TRANSPORT combo text against `Pentatonic`
  (h = 13 px).
- **6**: the GENERATE and MUTATE label centres against their tile centres.
- **7**: the contract diff.
- **8**: the NEW SEED position in `latest.png`, an empty ACID header strip,
  and an unchanged `gen_params.seed` update.
- **9**: the `VCF DRIVE` label in `latest.png`, centred on x 110.9 within
  2 px.
- **10**: the OCT RANGE MIN well in `generator.png` (MIN 0) reads `+0`.

**Regression checks.** In the new captures, confirm these:
- the design contract and every guardrail invariant above still hold;
- `python3 scripts/gui_qa_measure.py report target/gui-qa-offscreen` shows 10
  cards with 6 px gutters and the seam at 1614 | 1620 in every state;
- 27 knobs in FUN and 18 in ACCURATE, with no arc/pointer mismatch;
- no text below 3:1;
- the scoped diffs of the existing states are unchanged, except where #1,
  #3, #4, #5, #6, #8, #9 and #10 change pixels on purpose.
