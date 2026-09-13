# TECHNEO 303 — GUI remediation prompt, round 18

Source audit: `docs/superpowers/plans/2026-09-12-gui-audit-round18-report.md`.
Issue numbers below match that report exactly. Keep them stable in your
confirmation.

---

## Context

TECHNEO 303 is a Rust audio plugin (TB-303 clone) with an egui 0.31 editor
(nih_plug_egui). GUI source lives in `plugin303/src/gui/`:

- Tokens: `theme.rs` (all colors, 55 lines — read it first)
- Panels: `panels/{header_bar,top_controls,souped_panel,output_clipper,tweaks_panel,generator_panel,sequencer_panel}.rs`
- Widgets: `widgets/{knob,fader,switch,step_button,led,mini_keyboard,piano_roll_popup}.rs`

The window captures at 2732x1984 physical px (2x DPI, 1366x992 logical).
All coordinates in this prompt are **physical** pixels from `target/gui-qa/`.

**Branch state.** PR #64 (`fix/gui-visual-remediation-r17`) is unmerged and
carries round-17 fixes plus a galley cache key fix. Base your work on that
branch (or merge it first) — do not reimplement it and do not revert it.

### UI regions, top to bottom

1. **Header bar** (y 0-160) — title, TECHNEO badge, TOPOLOGY + QUALITY
   segment groups, ZOOM combo. Source: `panels/header_bar.rs`.
2. **Top controls** (y 160-430) — WAVE segments, synth-core knobs, ENVELOPES
   ADSR banks. Source: `panels/top_controls.rs`, `widgets/knob.rs`,
   `widgets/fader.rs`.
3. **Souped-up engine** (y 430-700) — VCF TAPS + VCA ENV segments, souped
   knob row with value pills. Source: `panels/souped_panel.rs`.
4. **Output clipper** (y 690-872) — IN/OUT/TILT knobs, OS segments, DIODE
   SI/GE boxes, BYPASS checkbox. Source: `panels/output_clipper.rs`.
5. **Analog character & tweaks** (y 882-1101) — TOL/AGE/NOISE, DRIFT,
   CMP SEED + RECAL, CALIBRATE columns. Source: `panels/tweaks_panel.rs`.
6. **Acid euclidean generator** (y 1101-1346) — PATTERN STRUCTURE fields,
   GENERATE/MUTATE, SLD CHAIN, SEED. Source: `panels/generator_panel.rs`.
7. **Sequencer + transport** (y 1346-1984) — 16 step grid, TRANSPORT &
   PATTERN right column. Source: `panels/sequencer_panel.rs`.

---

## Guardrails

- **Presentation layer only.** Do not change parameter ranges, defaults,
  value formatting, DSP, or persistence. A control that reads `25%` must
  still read `25%`.
- Do not add, remove, or reorder controls. Issues ask for boundaries, sizes,
  colors, and alignment only.
- Do not regress earlier fixes. Invariants that must survive:
  - Knob value-to-indicator mapping: sweep -135 deg..+135 deg from 12 o'clock;
    `arc_needle_delta_deg <= 4` per knob; bipolar knobs show no fill at the
    detent.
  - Disabled groups retain their selection marks (round-14 convention):
    QUALITY's grey selection border stays visible when disabled in ACCURATE.
  - TEXT_MUTED `#8f8fa4` and all text contrast ratios currently >= 4.5:1
    stay >= 4.5:1 (except disabled-state text, which stays as-is).
  - Panel-2 geometry identical with ADV ENV on/off once PR#64 is in (no 2 px
    shift).
  - No per-frame allocations in render paths (round-10 MEDIUM-7).
- Prefer fixing the shared widget or token over patching call sites
  (`theme.rs`, `widgets/knob.rs`, `widgets/fader.rs`, `widgets/switch.rs`),
  except where an issue names specific call sites.
- Keep `cargo fmt` and `cargo clippy` clean. `cargo test -p plugin303 --lib`
  must pass.

---

## HIGH

### 1. Structural strokes fail WCAG 1.4.11 (3:1 non-text)

**Symptom.** Measured on `latest.png`, ring scan at knob (622,236) r=50:
knob track `#222228` = `SURFACE_ELEVATED` (`theme.rs:7`, via
`KNOB_BODY_LIGHT` and track arc in `widgets/knob.rs`); tick ring + panel
borders `#2B2B34` = `BORDER_DEFAULT` (`theme.rs:10`); field fill `#3C3C3C`
(combo/value fields — locate the fill constant; it is not in `theme.rs`).
Ratios against panel `#1a1a1e` / window `#16161A`: 1.14-1.57:1. Every
component boundary is sub-perceptible.

**Required fix.**

- New or raised token(s) in `theme.rs` so each structural element reaches
  >= 3:1 against its own background:
  - **Knob track arc**: >= 3:1 vs `#16161A` well (approx `#4A4A57` or lighter).
  - **Tick ring + panel/group borders** (`BORDER_DEFAULT`): >= 3:1 vs
    `#1A1A1E`.
  - **Field fill**: >= 3:1 vs the panel behind it.
- Surfaces used as *fills* behind content (WELL_BG, PANEL_BG) are exempt —
  only boundary/indicator strokes and the field fill move.
- Text colors and `ACCENT_*` must not change. `BORDER_ACCENT` orange stays.

**Acceptance criteria.** `scripts/gui_qa_measure.py colors target/gui-qa/latest.png`
shows each structural token at >= 3:1 vs its background; knob track arc
visible in a re-captured `latest.png` crop at 1x without squinting; no text
ratio regresses.

### 2. Unselected segments have no boundary

**Symptom.** In VCF TAPS (x 46-1616, 523 px per segment), VCA ENV, TOPOLOGY,
QUALITY (`header_bar.rs:178` region), OS (`output_clipper.rs`), WAVE: only
the selected segment draws an orange frame; unselected siblings share one
undivided container. Click targets invisible. VCF TAPS call site:
`souped_panel.rs:141`.

**Required fix.** Every segment in every multi-segment group gets a visible
boundary — 1 px divider at >= 3:1 (uses the issue-1 token) or a per-segment
border. Selected treatment (orange border + white text) unchanged. Do not
change segment order, labels, or which segment is selected in any state.

**Acceptance criteria.** In `latest.png` and `accurate.png`, vertical
boundaries measurable between all adjacent unselected segments in all six
groups; disabled QUALITY group keeps its grey selection mark.

### 3. Three segmented-control patterns; DIODE selected text is orange

**Symptom.** DIODE (`SI`|`GE`, `output_clipper.rs`) renders as two separate
gapped boxes with borders on unselected segments and **orange text** on the
selected label — the only orange selected text in the app. All other groups:
shared container, no unselected borders, white selected text.

**Required fix.** One segmented-control component (extend `widgets/switch.rs`
or extract a shared segment row) applied to VCF TAPS, VCA ENV, TOPOLOGY,
QUALITY, OS, WAVE, DIODE. Selected = orange border + white text everywhere;
DIODE's orange-text variant goes away. DIODE keeps its two-box layout only
if the shared component supports it — otherwise it becomes a standard
segment row like the others.

**Acceptance criteria.** All seven groups render from one code path; in
`latest.png` every selected segment has identical border + text colors;
every unselected segment has the issue-2 boundary.

---

## MEDIUM

### 4. ADSR faders: ~15 pt usable travel, handle overhangs track

**Symptom.** `widgets/fader.rs:38-103`: chrome = 9 px above + 12 px below
the track; track 20 pt tall, handle 5 pt, handle ~2.4x track width; letter
glyph flush against track top. At 0% the handle covers the bottom cap.

**Required fix.** Track height >= 40 pt within the existing bank frame;
handle width <= track width + 4 pt; >= 4 pt gap between the A/D/S/R glyph
and the track top. Value readout stays below the track. Drag behaviour and
value mapping unchanged.

**Acceptance criteria.** `advanced.png` re-capture: MEG D-slider track
measures >= 80 px physical; handle does not overhang the track sides by
more than 8 px physical; `S` at 0% shows track visible below the handle.

### 5. TRANSPORT & PATTERN grid breaks; row-5 heights 20 pt vs 75 pt

**Symptom.** `sequencer_panel.rs` transport column: rows 1/3 set a
470 px + 470 px grid; rows 2, 4, 5 split it at other x positions; `CLEAR`
is ~75 pt tall next to 20 pt combos (`16th`, `Direct`).

**Required fix.** Hold the two-column grid across all five rows (or
introduce an explicit, documented sub-grid aligned to it). Equalise row-5
control heights: combos and CLEAR share one height token. BPM `-`/`+`
buttons, LEN field, and SHIFT buttons keep their current row membership.

**Acceptance criteria.** Re-captured `latest.png`: left edges of row-2/4/5
controls align to the row-1/3 column edges within 4 px physical; row-5
control heights equal within 4 px physical.

### 6. `SLD CHAIN` label 17 px vs 12 px siblings

**Symptom.** `generator_panel.rs:345` `ui.checkbox(..., "SLD CHAIN")`
renders at cap height 17 px; `SLIDE:`/`ACC BIAS:`/`REST:` in the same row
group render at 12 px.

**Required fix.** SLD CHAIN label at the same size as its row siblings.
Checkbox state and hit rect unchanged.

**Acceptance criteria.** Cap heights equal within 1 px in a 3x crop of the
SLD CHAIN row in `latest.png`.

### 7. Two checkbox styles/sizes; LINK check is white

**Symptom.** BYPASS / ADV ENV / SLD CHAIN: filled grey square 28 px.
JITTER (`sequencer_panel.rs:641`, custom painted): outline-only 13 px,
8 pt label. LINK checkmark white; every other on-state is orange.

**Required fix.** One checkbox component, one size (the 14 pt filled
square), used at all five sites. Checkmarks/orange on-state everywhere —
LINK included. Keep the JITTER hit-rect width logic (the comment at
`sequencer_panel.rs:71-72` explains the 52 px budget); only the glyph box
and label size change.

**Acceptance criteria.** All five checkboxes measure the same box size
(within 2 px physical) and style in `latest.png`; LINK on-state is orange.

### 8. Read-only seed styling: CMP SEED boxed vs SEED bare

**Symptom.** `tweaks_panel.rs:233` CMP SEED renders in a bordered field;
`generator_panel.rs:378` SEED is bare text.

**Required fix.** Both read-only seeds use the same treatment — the
bordered field (matches CH field and value readouts). Values and labels
unchanged.

**Acceptance criteria.** In `latest.png` both seeds render in identical
field chrome.

### 9. Panel vertical padding asymmetric (4-14 pt top; +25 pt bottom in two panels)

**Symptom.** Top padding varies 4-14 pt across SYNTH CORE, SOUPED, OUTPUT
CLIPPER, ANALOG, ACID, SEQUENCER; OUTPUT CLIPPER and SEQUENCER carry +25 pt
bottom slack.

**Required fix.** One panel padding token applied by the shared panel frame.
Content must not clip; panels keep current heights (absorb the change in
the slack, not by growing the window).

**Acceptance criteria.** `panels` measurement shows equal top/bottom
padding across all panels within 2 px physical; window stays 1366x992
logical.

### 10. ANALOG CHARACTER dead bands and mixed column alignment

**Symptom.** Four empty bands 238/168/170/109 pt; four columns with four
different vertical alignments; separators 718/385/450 px apart, stopping
short of panel edges.

**Required fix.** Equal-width columns with one shared vertical alignment
(top-aligned), or a single flow row. Separators either span the full panel
content height and sit on the column grid, or are removed. Knob/field
values and labels unchanged.

**Acceptance criteria.** `deadspace` on `latest.png` shows no contiguous
empty band > 40 pt inside the ANALOG panel; separator x positions align to
column edges.

### 11. Header row mixes 13 pt and 18 pt control heights

**Symptom.** TOPOLOGY/QUALITY segments 26 px (y 126-151) vs ZOOM combo
36 px (y 116-151); centres off by 2.5 pt.

**Required fix.** One control height for the header row (18 pt), segments
and combo alike. Header bar total height unchanged.

**Acceptance criteria.** All header controls share y-extent within 2 px
physical in `latest.png`.

### 12. Knob size tiers not systematic

**Symptom.** Outer diameters: synth core ~83 px, souped ~53 px, clipper
~55 px, DRIFT/RECAL ~48 px — 27 vs 27.5 vs 24 pt differ by rounding
amounts. Value-pill widths split 104 px vs 72 px to match.

**Required fix.** Declare explicit size tokens (e.g. LARGE 42 pt / MEDIUM
27 pt / SMALL 24 pt) in the knob widget or theme; assign every knob row to
exactly one token; souped and clipper share MEDIUM. Pill widths follow the
same tokens. Knob positions/values unchanged.

**Acceptance criteria.** Measured diameters in `latest.png` match the
declared tokens exactly (within 2 px physical at 2x).

---

## LOW

13. **Playhead ring red/pink** (`running.png` step 04; `theme.rs:24`
    `ACCENT_RUBY` or the `PLAYHEAD_RED` alias). Required: playhead uses an
    in-palette hue (orange or cyan), distinct from step accents.
14. **REST steps show a dimmed pitch** (`C 3` on step 12). Required: REST
    cells render no pitch text (cell stays; gate row unchanged).
15. **GENERATE = MUTATE styling.** Required: one is primary (solid orange),
    the other secondary (outline/flat). Labels, positions, order unchanged.
16. **Separators stop short of panel edges** (clipper right cluster, analog
    panel). Required: extend to panel content edges or remove; ends must
    align with another element. (Folds into #10 for the analog panel.)
17. **False-affordance pills** (`BASS SYNTH`, `MONO`, step-number cells).
    Required: remove button chrome (border/fill) from static badges, or
    restyle as plain labels. Text unchanged.
18. **Case/family mix** (`16th`, `Direct` sentence case; `BASS SYNTH`/`MONO`
    monospace). Required: match surrounding label conventions — uppercase
    labels, one font family. Combo *values* may keep case if that is the
    param's display string; do not change param formatting.
19. **OCT RANGE `MIN +0`** signed zero. Required: render `0` without a sign
    when the value is zero. Display formatting only.
20. **Generator field labels 4.43:1.** Required: label color >= 4.5:1 vs
    panel background (move to `TEXT_MUTED` `#8f8fa4` or lighter).
    Applies to ROOT:/SCALE:/PULSES/STEPS/SHIFT/PITCH LEN: labels.
21. **Empty-state card padding 15 vs 42 px** (ACCURATE SOUPED panel).
    Required: symmetric vertical padding.
22. **Duplicate `TECHNEO 303` title** (chrome + header 60 px below).
    Required: keep one. Presentation-only — window title string is
    host-visible; prefer dropping the in-app repetition.
23. **`OS` label on panel-title baseline.** Required: sits directly above
    its segment group like every other group label.
24. **Souped value pills off-centre by up to 3.5 px.** Required: pill
    centred on its knob axis within 1 px.
25. **MEG-disabled state unreadable as inert.** Required: disabled MEG bank
    gets the same treatment as the SOUPED ACCURATE panel — a dimmed overlay
    or a one-line reason. No behaviour change: ADV ENV toggle still works.

---

## Verification

Re-render the full capture set after the changes:

```bash
bash scripts/capture_standalone_gui.sh
python3 scripts/gui_qa_measure.py report target/gui-qa
```

Confirm every PNG is 2732 px wide — a wrong window id silently changes the
width and invalidates every coordinate.

**States to capture (existing 8):** latest, accurate, advanced, running,
knobs, quality_high, generator, zoom75. No new states are required; issues
13 (playhead) is covered by `running`, 4 (ADSR) by `advanced`, 2/3 disabled
QUALITY by `accurate`.

**Per-issue confirmation.** Report each issue 1-25 with what changed and
the measurement that proves it, or state explicitly that you did not fix it
and why:

- **1** — contrast ratios from `colors` for track/ticks/borders/field fill.
- **2** — divider pixel columns measured between unselected segments in all
  six groups.
- **3** — one code path; identical selected treatment sampled in captures.
- **4** — track px height + handle overhang px in `advanced.png`.
- **5** — column-edge alignment px + row-5 height equality px.
- **6/7/8/11/12/24** — px measurements from crops.
- **13/14/15/17/18/19/20/21/22/23/25** — crop visual + the relevant token
  or pixel check.
- **9/10/16** — `panels` / `deadspace` numbers.

**Regression checks.** `knobs` still shows `arc_needle_delta_deg <= 4` on
all knobs (ignore the known TONE ring-fit false positive, ~6 px high, and
r=20 combo-arrow false positives); `accurate` still shows the disabled
QUALITY grey selection border; no text contrast regresses below 4.5:1;
`cargo fmt --check`, `cargo clippy -p plugin303 --all-targets`, and
`cargo test -p plugin303 --lib` are clean.
