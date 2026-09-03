# TECHNEO 303 — GUI remediation prompt, round 12

Source audit: `docs/superpowers/plans/2026-09-02-gui-audit-round12-report.md`.
Issue numbers below match that report exactly. Keep them stable in your confirmation.

---

## Context

TECHNEO 303 is a Rust VST3/CLAP acid bass synth (`plugin303/`) whose editor is built with
`nih_plug_egui`. The window captures at 2732x1984 physical pixels (2x DPI, so 1366x992 logical).
All coordinates in this prompt are **physical** pixels from the captures in `target/gui-qa/`.

### UI regions, top to bottom

1. **Window title bar** — macOS chrome, title "TECHNEO 303".
2. **Header bar** (y 84-171) — app title, `BASS SYNTH` / `MONO` tags, TOPOLOGY segmented control,
   QUALITY segmented control, ZOOM combo box. Source: `plugin303/src/gui/panels/header_bar.rs`.
3. **Oscillator panel** (y 179-380) — WAVE selector (SAW/SQR/SWP), seven large knobs
   (TUNING, CUTOFF, RESONANCE, ENV MOD, DECAY, ACCENT, VOLUME), and the ENVELOPES box
   (x 1461-1962) holding the ADV ENV checkbox plus the MEG and VEG ADSR fader groups.
   Source: `plugin303/src/gui/panels/top_controls.rs`, `plugin303/src/gui/widgets/fader.rs`.
4. **Souped-up engine & analog drawer** (y ~381-679) — VCF TAPS (2/3/4-POLE) and VCA ENV
   (GATE/ENV) toggle groups on one row, then nine knobs (DRIVE, SUB, GLIDE, PITCH1, PITCH2,
   MIX2, PW, SWEEP, R-COMP). Replaced by an "ACCURATE MODE ACTIVE" placeholder card in ACCURATE
   topology. Source: `plugin303/src/gui/panels/souped_panel.rs`.
5. **Output clipper** (y 687-870) — BYPASS checkbox + OS (2X/4X) group at left, nine knobs
   (DRIVE, THRESH, BASS, TONE, IN, OUT, TILT, DYN, MIX), LINK checkbox + DIODE (SI/GE) group at
   right. Source: `plugin303/src/gui/panels/output_clipper.rs`.
6. **Analog character & tweaks** (y 883-1109) — four divider-separated columns: TOL/AGE/NOISE,
   DRIFT knob, CMP SEED + RECAL knob, CALIBRATE button + CALIBRATED status.
   Source: `plugin303/src/gui/panels/tweaks_panel.rs`.
7. **Acid Euclidean generator** (y 1117-1357) — GEN SEED + NEW SEED at top right, then four
   numbered columns: 1. PATTERN STRUCTURE, 2. OCTAVE BEHAVIOR, 3. PROBABILITIES, 4. GENERATION.
   Source: `plugin303/src/gui/panels/generator_panel.rs`.
8. **Sequencer** (y 1365-1968) — sixteen step columns in two rows of eight (grid x 38-2085,
   column pitch 247 px, gutter 10 px) and the TRANSPORT & PATTERN column (x 2110-2690).
   Source: `plugin303/src/gui/panels/sequencer_panel.rs`,
   `plugin303/src/gui/widgets/step_button.rs`.

Shared tokens live in `plugin303/src/gui/theme.rs`.

---

## Guardrails

- **Presentation layer only.** Do not change DSP, parameter ranges, parameter defaults, value
  formatting, persistence, or state handling. A knob that reads `50%` must still read `50%`.
- Do not add, remove, or reorder controls except where an issue explicitly asks for it
  (issues 5, 6, 7 permit relayout; issue 15 adds a heading).
- Do not regress any round-5 through round-11 fix. In particular: knob needle and arc must stay
  in agreement; ENVELOPES must stay inside the oscillator panel; the header's right group must
  stay flush right; zoom presets must stay at or below 100 %; `TEXT_MUTED` must stay at or above
  4.5:1; per-frame heap allocations must not reappear in paint code.
- Prefer fixing the shared widget or token over patching each call site, except where the issue
  names specific call sites.
- Keep `cargo fmt` and `cargo clippy` clean. Existing tests must pass.

---

## HIGH

### 1. Knob arc polarity is inferred from the default value

**Symptom.** `plugin303/src/gui/widgets/knob.rs:141` computes
`let is_bipolar = (default_norm - 0.5).abs() < 0.01;`. Any unipolar 0-100 % parameter that happens
to default to 50 % is therefore filled outward from the 12 o'clock detent instead of from the
minimum. In `latest.png` only 3 of 27 knobs draw a value arc at all — GLIDE, DRIFT and RECAL —
because RESONANCE, ENV MOD, ACCENT, PW and the clipper DRIVE / THRESH / BASS / TONE knobs all sit
at 50 %, giving a zero-length "bipolar" arc. CUTOFF (2.1 kHz) and DECAY (1.1 s) are misclassified
the same way and will fill leftward from 12 o'clock as soon as they go below their default.

**Required fix.** Give `Knob303` an explicit polarity, set by the caller — for example a
`.bipolar(bool)` builder, or a `KnobPolarity` enum — and remove the default-value inference. Then
declare polarity at every call site:

- **Bipolar** (fill from the centre detent): TUNING, PITCH1, PITCH2, VOLUME, and the clipper
  IN / OUT / TILT dB trims.
- **Unipolar** (fill from the minimum): everything else, including RESONANCE, ENV MOD, ACCENT, PW,
  CUTOFF, DECAY, MIX2, SUB, SWEEP, R-COMP, GLIDE, DRIFT, RECAL, and the clipper
  DRIVE / THRESH / BASS / TONE / DYN / MIX.

**Acceptance criteria.** With every knob at its default, each unipolar knob shows an arc running
from the -135° minimum to its needle, and each bipolar knob at centre shows no arc. The needle
angle must stay unchanged for every knob — this issue changes only the arc, never the needle.
Arc end angle and needle angle must agree within 3° for every knob at every value.

### 2. ACCURATE mode hides which QUALITY tier is selected

**Symptom.** `accurate.png` contains zero accent-orange pixels in the QUALITY group; `latest.png`
has 380. `plugin303/src/gui/panels/header_bar.rs:144` wraps the group in
`ui.add_enabled_ui(!is_accurate, ...)`, and egui's disabled visuals strip the selected segment's
orange stroke. Disabling the group is intentional — the tooltip at line 155 explains that quality
applies to FUN mode — but the active tier becomes invisible.

**Required fix.** A disabled segmented control must still mark its selected segment, using a dimmed
accent (for example the accent colour at reduced alpha, or a muted stroke) that is visually distinct
from the unselected segments.

**Acceptance criteria.** In an ACCURATE-topology capture, exactly one QUALITY segment is visibly
marked as selected, and it is the same segment that was selected in FUN. The marking is dimmer than
the FUN-mode selection so the group still reads as disabled.

### 3. Disabled text drops to 2.3:1 contrast

**Symptom.** In `accurate.png` the disabled QUALITY options measure `#53535E` on `#1A1A1E`
(**2.29:1**) and the disabled "ANALOG CHARACTER & TWEAKS" heading measures `#54545F` (**2.32:1**).
The panel's entire contents — heading, labels, fields, DRIFT and RECAL knobs — render at this level.

**Required fix.** Define an explicit disabled text token in `theme.rs` and apply it wherever egui's
default disabled visuals are currently in play. It must reach at least **3:1** against `PANEL_BG`.

**Acceptance criteria.** No disabled text anywhere in the UI measures below 3:1 against its own
background. Disabled content still reads as clearly weaker than enabled content.

### 4. Playhead renders as three unaligned white elements

**Symptom.** In `running.png`, step 04 carries three separate white marks that merge into one thick,
uneven top edge:

- the LED bar (`step_button.rs`, "1. Playhead LED bar") filled `TEXT_PRIMARY` — a solid white band
  at y 1375-1384;
- the step-number pill's white stroke — reading as a 2 px rule at y 1413-1414;
- the column outline (`step_button.rs`, "Playhead outline") — 2 px at x 809-810 / 1056-1057 with its
  bottom edge at y 1637-1638.

The column outline reuses `strip_rect`, the full cell rect, and the step's child pills are laid out
with zero horizontal inset, so the outline sits directly on the ACC / SLD / REST / note / arrow
button borders. Every other step keeps a visible gap there.

**Required fix.** Make the playhead one coherent indicator:

- give the step strip a small horizontal inner inset (about 2 logical px) so the playhead outline is
  drawn in the gutter and never touches a child pill's border;
- resolve the LED bar and the outline into a single top edge — either inset the LED bar from the
  strip edges, or drop the outline's top segment where the LED bar already marks it, or give the LED
  bar the same stroke width as the outline. It must not read as a 10 px slab meeting a 2 px line;
- keep the step-number pill's active treatment, but not as a bare white rule floating under the
  number — tie it to the pill's own border.

Note that `theme.rs` still defines `PLAYHEAD_RED` and `PLAYHEAD_RED_GLOW`, which round 11 replaced
with `TEXT_PRIMARY`. Remove them if nothing else uses them.

**Acceptance criteria.** In a running capture, the current step is marked by a single indicator of
uniform stroke width, its outline does not touch any child control's border, and no white element
inside the step column is thicker than any other. Only one step is marked. The unmarked steps are
pixel-identical to their stopped-state rendering.

### 5. Souped-up engine toggle row: 83 % of its width is empty

**Symptom.** In `latest.png` the VCF TAPS / VCA ENV row has content only from x 46 to x 459; the
remaining 2240 px to the panel's right edge is empty.

**Required fix.** Fill or absorb the space. Acceptable approaches, in order of preference: move
related controls onto the row; distribute the toggle groups across the row width; or constrain the
toggle groups inside a bounded container so the emptiness reads as deliberate.

**Acceptance criteria.** No horizontal run wider than 600 px inside a panel is empty of both content
and a deliberate divider or container edge.

### 6. Oscillator panel: 736 px of dead width right of ENVELOPES

**Symptom.** The ENVELOPES box ends at x 1963 while the panel interior runs to x 2708 — 27 % of the
window width, with no divider or content.

**Required fix.** Either widen the ENVELOPES box toward the panel's right edge, or move content into
the space, or right-align the box and rebalance the knob row. Do not reintroduce the round-11 defect
where ENVELOPES overflowed its parent.

**Acceptance criteria.** The oscillator panel's right inner margin matches its left inner margin
within 10 px, and ENVELOPES stays fully inside the panel.

### 7. TRANSPORT & PATTERN column: 265 px of dead height

**Symptom.** The last transport row (CLEAR) ends at y 1702; the panel's bottom border is at y 1967.
That is 44 % of the column empty, and the column stops 209 px above where the step grid to its left
ends.

**Required fix.** Distribute the transport rows down the column, or move content into the space, or
let the panel shrink to its content and give the step grid the height. The step grid must keep its
current geometry.

**Acceptance criteria.** The transport column's content extends to within 60 px of the panel's
bottom border, or the panel's bottom border sits within 60 px of the transport column's last row.

---

## MEDIUM

### 8. ComboBox text renders about 50 % larger than every other control

**Symptom.** Measured glyph heights in `latest.png`: combo box text (ZOOM `100%`, ROOT `C`,
`Pentatonic`, TOL `0%`, `16th`, `Direct`) is 18-19 px; button and segment text (`LOW`, `FUN`,
`CLEAR`) is 11-12 px; numeric fields (`05`, `16`) are 13 px; labels (`ROOT:`, `SCALE:`) are 10-11 px.
The combo boxes render their selected text in egui's default body style rather than the theme font
the surrounding widgets use — see `header_bar.rs:125`, where the label gets
`FontId::new(8.5, FontFamily::Proportional)` but the `ComboBox` at line 128 does not.

**Required fix.** Apply the theme's control font to every `ComboBox` selected-text and popup-item
render, in the header, the tweaks panel, the generator panel and the transport column.

**Acceptance criteria.** No combo box's text is more than 2 px taller than the button text beside it.

### 9. `SLD CHAIN` checkbox label is 18 px

**Symptom.** Every sibling label in the generator panel is 10-13 px; the LINK and ADV ENV checkbox
labels are 12 px. `SLD CHAIN` renders at 18 px.

**Required fix.** Give it the same label font as the other checkboxes in the same panel.

### 10. Three different muted / disabled greys

**Symptom.** `#8F8FA4` (5.70:1, inactive sequencer content), `#6F6F75` (3.55:1, disabled ENVELOPES
MEG group), `#53535E` (2.29:1, ACCURATE-mode disabled).

**Required fix.** Collapse to two tokens — one muted, one disabled — both defined in `theme.rs`, and
route every dimmed render through them. This subsumes issue 3.

**Acceptance criteria.** Sampling any dimmed text in any of the four capture states yields one of
exactly two colours.

### 11. Checked-checkbox label styling is inconsistent

**Symptom.** In `advanced.png` the `ADV ENV` label turns orange when checked. `LINK` in `latest.png`
is checked and stays white. `BYPASS`, `SLD CHAIN` and `JITTER` are unchecked and white.

**Required fix.** Pick one rule for checked-state labels and apply it to all five checkboxes.

### 12. DIODE is the only segmented control drawn as separate buttons

**Symptom.** TOPOLOGY, QUALITY, VCF TAPS, VCA ENV and OS each use a single rounded container with
inner segments. DIODE renders `SI` and `GE` as two detached rounded buttons.

**Required fix.** Render DIODE with the same grouped-container segmented control as its peers.

### 13. Selected-segment text colour is inconsistent

**Symptom.** `4-POLE`, `FUN`, `LOW`, `ENV` and `2X` render white inside their orange stroke. `SI`
renders orange.

**Required fix.** One selected-segment text colour across all segmented controls. Likely resolved
together with issue 12.

### 14. Transport rows are not right-aligned

**Symptom.** Right edges: `SYNC: HOST` at x 2690 and `> SHIFT` at x 2690, but `SYNTH: ON` at x 2674
and `CLEAR` at x 2676.

**Required fix.** All transport rows share one right edge.

**Acceptance criteria.** Every row's rightmost control ends at the same x within 2 px.

### 15. The ACCURATE placeholder panel has no heading

**Symptom.** In FUN the panel is titled "SOUPED-UP ENGINE & ANALOG DRAWER". In ACCURATE
(`accurate.png`) the panel renders headless while every other panel keeps its orange title.

**Required fix.** Give the ACCURATE placeholder the same panel heading in the same position and
style as the FUN variant.

### 16. Panel title sizes differ

**Symptom.** "ACID EUCLIDEAN GENERATOR" is 16 px; "SOUPED-UP ENGINE & ANALOG DRAWER",
"OUTPUT CLIPPER" and "ANALOG CHARACTER & TWEAKS" are 14 px. The 13 px sub-panel titles
(ENVELOPES, TRANSPORT & PATTERN) read as a deliberate second level and should stay.

**Required fix.** One size for top-level panel titles, one for sub-panel titles, both from `theme.rs`.

### 17. ANALOG panel column contents are not centred in their columns

**Symptom.** The panel divides into four divider-separated columns, but DRIFT sits about 70 px left
of its column's centre, and RECAL likewise. Overall panel fill is 4.1 %.

**Required fix.** Centre each column's content within its own divider bounds, or size the columns to
their content so the dividers land where the content actually breaks.

### 18. Generator field left edges are misaligned

**Symptom.** The ROOT combo starts at x 188 while the PULSES and SHIFT fields below it start at
x 212 — 24 px off. The same 25 px offset appears between the SCALE combo and the STEPS / PITCH LEN
fields.

**Required fix.** Within each generator sub-column, all field left edges align. Likely a combo-box
padding difference; fixing issue 8 may change these numbers, so measure again afterwards.

---

## LOW

19. Inter-panel gap is 8 px everywhere except clipper to analog, which is 13 px. Make it uniform.
20. The header has a 1416 px empty span between the `MONO` tag (x 468) and TOPOLOGY (x 1883).
    Acceptable as a left/right split; only address it if a cheap rebalance exists.
21. The header's right inner margin is 40 px against a 20 px left margin. Make them equal.
22. The ZOOM label is centred about 10 px right of its combo box. Centre it on the combo.
23. Step grid insets: 18 px left, 23 px right. Make them equal.
24. Sequencer vertical padding: 19 px top, 56 px bottom. Make them equal, or absorb into issue 7.
25. "TECHNEO 303" appears in both the window title bar and the header bar. Consider dropping one.
26. Generator columns have no vertical dividers while ANALOG and OUTPUT CLIPPER do. Pick one
    convention.

---

## Verification

Re-render the full capture set after the changes, at 2732x1984, and confirm the width is 2732 —
a wrong window ID silently produces a different width.

**States to capture:**

1. FUN topology, LOW quality, ADV ENV off, stopped — the `latest.png` baseline.
2. ACCURATE topology — for issues 2, 3, 15.
3. ADV ENV on — for issues 10, 11.
4. Transport running — for issue 4.
5. **New:** FUN topology with RESONANCE, CUTOFF, DECAY and clipper DRIVE moved off their defaults
   (roughly 25 % and 75 %) — this is the only state that shows issue 1 directly. Without it the fix
   is unverifiable.
6. **New:** ACCURATE topology with ADV ENV on — the one untested state combination.

**Per-issue confirmation.** Report each issue number with what you changed and the measurement that
proves it, or state explicitly that you did not fix it and why:

- **1** — arc end angle and needle angle per knob, and which knobs you declared bipolar.
- **2** — accent pixel count in the QUALITY group in the ACCURATE capture (must be non-zero).
- **3, 10** — measured contrast ratio of every dimmed text colour, and the list of tokens remaining.
- **4** — white-pixel row and column runs inside the current step column; all strokes equal width.
- **5, 6, 7** — the widest empty horizontal run per panel and the transport column's bottom gap.
- **8, 9, 16** — glyph heights per control class.
- **11, 12, 13** — one screenshot region per affected control showing the unified treatment.
- **14, 18, 21, 22, 23, 24** — the aligned edge coordinates.
- **15** — the ACCURATE panel heading present, matching the FUN heading's position and style.
- **17** — each analog column's content centre against its divider bounds.
- **19, 20, 25, 26** — what you changed, or why you left it.

**Regression checks.** Confirm in the new captures that: needle angles are unchanged from round 12
for every knob at its default; ENVELOPES stays within the oscillator panel; the header's right group
stays flush right; `TEXT_MUTED` stays at or above 4.5:1; the step grid keeps 247 px column pitch and
10 px gutters; and `cargo fmt`, `cargo clippy` and the test suite all pass.
