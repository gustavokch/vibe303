# TECHNEO 303 — GUI Visual Audit, Round 22

Date: 2026-10-30
Captures: `target/gui-qa-offscreen` (`latest`, `accurate`, `advanced`, `running`,
`running_note`, `knobs`, `quality_low`, `generator`), all 2432x1280, offscreen render.
Gap renders: `/tmp/r22_gaps` (25 states, listed below), all 2432x1280, offscreen
render from a throwaway worktree. The worktree added QA states and synthetic
pointer events only. Its GUI source is identical to the baseline.
Baseline: commit `76dfe3a` "Merge pull request #83 from
gustavokch/fix/analog-tweaks-calibrate" on `main`.

The round-21 fixes merged in #78 and were reverted in #79 (`de58c8b`) before
#80 merged round 20 into `main`. Since round 21 (`da9e6ed`), the GUI source
changed in one file only: `tweaks_panel.rs`, the background CALIBRATE from #83.
So every round-21 item is open again, and this round re-measures each one.

## Capture set

| File | State |
|---|---|
| `latest.png` | defaults: FUN, QUALITY DRAFT, stopped |
| `accurate.png` | ACCURATE: SOUPED-UP banner, ANALOG TWEAKS readouts, QUALITY and ADV ENV disabled |
| `advanced.png` | ADV ENV on: both ADSR banks enabled |
| `running.png` | running, playhead on step 04 (a REST step) |
| `running_note.png` | running, every step NOTE |
| `knobs.png` | TUNING +25.0 ct, CUTOFF 0.75 (1.4 kHz) |
| `quality_low.png` | QUALITY LOW |
| `generator.png` | every generator field off its default, SLD CHAIN on |

Gap renders (throwaway, not in the repo):

| File | State |
|---|---|
| `knob_extremes` | TUNING -100 ct, PITCH1 -24 st, PITCH2 +24 st, IN -18 dB, OUT +18 dB, TILT -6 dB (the round-21 QA state, cherry-picked from `8ac4140`) |
| `clipper_off` | OUTPUT CLIPPER bypassed (also from `8ac4140`) |
| `quality_high`, `quality_best` | existing `QaState`s that no script renders |
| `toggles` | WAVE SQR, MIDI on, SYNTH off, LINK off, JITTER on, 12th, Decoupled |
| `wave_swp` | WAVE SWP |
| `accurate_adv` | ACCURATE with ADV ENV on |
| `sync_pending` | the stored VCO trim differs from the parameters (`needs_rebuild`) |
| `calibrating` | `vco_trim_busy` set: a trim runs in the background |
| `running_tie`, `running_acc` | playhead on step 02 (TIE) and on step 01 (ACC) |
| `confirm_clear` | one click on CLEAR: the armed state |
| `popup_scale`, `popup_root`, `popup_timebase`, `popup_mode`, `popup_tol`, `popup_piano` | each popup open |
| `hover_*` (12) | pointer over QUALITY in ACCURATE, ADV ENV in ACCURATE, the MEG bank, RUN, GENERATE, a 2-POLE segment, LINK, the CUTOFF knob, the AGE well, a lit ACC pill, a pitch cell |

Pixel diffs confirm the states are distinct and correctly scoped:

- `accurate`: the app bar (y 14-49), the ADV ENV tile (y 66-101, x 2290-2407),
  the SOUPED-UP header and body (y 272-464) and ANALOG TWEAKS (y 481-566).
- `advanced`: ENVELOPES (y 74-248), the GLIDE value pill (y 401-414, x 432-459,
  5.5 ms → 170 ms), and the CUTOFF value pill (y 231-244, x 468-486,
  867 Hz → 894 Hz). Round 21 missed the CUTOFF pill: its cluster merged with
  the fader cluster. Both pills follow the voice's ADV ENV mapping
  (`value_text.rs:45`, `:55`). The recipe table now lists both.
- `knobs`: TUNING and CUTOFF only (y 118-244).
- `quality_low`: the QUALITY bar only.
- `generator`: the generator body and the GENERATION SEED value.
- `running`: step 04 (LED, number, plate) and the RUN LED.
- `running_note`: as `running`, plus every gate and pitch cell.
- `accurate_adv` against `accurate`: 120 px, the ADV ENV LED only. The banks
  stay disabled, and the CUTOFF and GLIDE pills do not change. That is correct:
  the voice ignores ADV ENV in ACCURATE.
- `sync_pending` and `calibrating`: the CALIBRATE tile (y 272-307,
  x 2248-2407) and the status line (y 554-566) only.
- `toggles`: WAVE, the LINK LED, the MIDI, SYNTH and JITTER LEDs, and the two
  combo values. Nothing in VOICE, SOUPED-UP or OUTPUT CLIPPER changes (see #23).

The first `report` run listed `clipper_off.png` and `knob_extremes.png`, written
on 2026-10-24 by the reverted round-21 renderer. The current renderer does not
draw these states, so the files were never overwritten. Their diffs showed the
reverted round-21 generator and TRANSPORT layouts. They were moved out before
this report. `report` now flags a capture written more than 10 minutes before
the newest one (`stale_captures` in `gui_qa_measure.py`, tested in
`tests/test_gui_qa_measure.py`).

## Previous round (21)

All ten items are **OPEN**, because the fixes were reverted. Each one was
measured again:

- **1 TRANSPORT combos without caption or tooltip: OPEN → #6.** `16th ▼` and
  `Direct ▼` still have no caption. `sequencer_panel.rs:418` and `:471` have
  no `on_hover_text`. The open popups (`popup_timebase`, `popup_mode`) add no
  hint either.
- **2 MEDIUM knob pills narrower than their values: OPEN → #2.** Now proven
  in pixels with `knob_extremes`.
- **3 ROOT/SCALE row off the grid: OPEN → #7.** The SCALE: caption starts at
  x 217 (ROOT C) and at x 226 (ROOT D#). STEPS (n): starts at x 201.
- **4 Disabled bar disappears into a header strip: OPEN → #10.** The
  `clipper_off` render now shows the same fault in a second strip.
- **5 Combo values in three treatments: OPEN → #11.**
- **6 GENERATE and MUTATE labels left-aligned: OPEN → #12.** Text at x 1653
  in a tile from x 1636 to x 2407.
- **7 No contract token for action labels: OPEN → #13.**
- **8 NEW SEED and SEED in different cards: OPEN → #14.**
- **9 Two knobs labelled DRIVE: OPEN → #15.**
- **10 Sign on zero: OPEN → #16.** In `generator`, OCT RANGE shows MIN `0`
  and MAX `+1`.
- Flagged *GLIDE readout follows ADV ENV*: **still true, and CUTOFF does the
  same** (867 → 894 Hz). Carried.
- Flagged *scale names shortened*: **unchanged.** Carried.
- Flagged *TRANSPORT 8 pt labels*: **unchanged.** CH: `#ADADAD` (6.74:1),
  BPM: `#B0B0B0` (6.98:1). These are glyph cores, not drift.
- Flagged *ENABLE FUN MODE lit fill*: **unchanged and per contract.**
- Round-21 capture gaps: knob values at the limits, OUTPUT CLIPPER off, CONFIRM
  CLEAR, open popups, QUALITY HIGH and BEST, SQR and SWP, TIE and ACC under the
  playhead, SYNTH muted, MIDI on, LINK off, and ADV ENV in ACCURATE.
  **All rendered this round**, in the gap set.

## Geometry

Cards match the contract's landmark table in all 8 captures and all 25 gap
renders: 10 cards, every `gap_above=6`, column seam at x 1614 | 1620. The
header-strip tiles measure 36 px, inset 4 px from the top and bottom of the
44 px strip (CALIBRATE y 272-307, NEW SEED y 604-639, ADV ENV y 66-101).
Knobs: 27 in FUN, 18 in ACCURATE. The arc tip is within 2.3°/0.9 px of the
pointer on every knob in `latest`, `knobs` and `accurate`, and within
2.2°/0.8 px in `knob_extremes`. Every bipolar knob at a limit fills from the
centre (PITCH1 -24 st: `arc=[-136.9, 1.2] from=centre`). In `clipper_off`
and `accurate`, the disabled knobs draw `muted` arcs. No probed text is below
3:1 in any state. The lowest values are disabled labels: DIODE 3.68:1, 4X
3.72:1 and ADV ENV 3.77:1. Orange totals are the same as in round 21: 28,650 px
at rest and 30,274 px running.

---

## HIGH

### 1. The piano-roll close button shows a missing-glyph box

**Kind.** Defect (checklist A/F: unreadable text).

**Symptom.** `popup_piano`, close button at x ≈ 248-298, y ≈ 466-498. At 6x
the label is a hollow rectangle, the replacement glyph, not a cross.
`piano_roll_popup.rs:187-193` draws `RichText::new("✕")` (U+2715) at
proportional 9 pt. The theme installs no fonts (`theme.rs` has no
`FontDefinitions`), so the glyph comes from egui's default set, and that set
does not contain it. [INFERENCE] The plugin window uses the same font set, so a
host shows the same box. This is the only visible close control in the popup.
Escape also closes it (`piano_roll_popup.rs:129`), but nothing on screen says
so.

**Required fix.** The close control shows a glyph the font contains, or a
painted cross (as the ▼ of the combos is painted, `sequencer_panel.rs:35`), or
a word. At 3x the mark reads as a close mark. Do NOT change: the popup
position and the Escape behaviour.

---

## MEDIUM

### 2. MEDIUM knob pills are narrower than their longest values

**Kind.** Defect. Checklist A: a value wider than its pill.

**Symptom.** `knob_extremes`. The pill is 38 pt (`theme.rs:61`), which is
76 px. The value is 8 pt monospace and is not clipped to the pill
(`knob.rs:221-230`). Ink and pill extents, measured on the row of the value:

| Knob | Value | Ink x (width) | Pill x (width) |
|---|---|---|---|
| PITCH1 | `-24.0 st` | 597-674 (78 px) | 598-673 (76 px) |
| PITCH2 | `+24.0 st` | 771-849 (79 px) | 773-848 (76 px) |
| IN | `-18.0 dB` | 772-849 (78 px) | 773-848 (76 px) |
| OUT | `+18.0 dB` | 946-1024 (79 px) | 948-1023 (76 px) |
| TILT | `-6.0 dB` | 1127-1194 (68 px) | 1123-1198 (76 px) |
| TUNING (LARGE) | `-100.0 ct` | 241-328 (88 px) | 233-336 (104 px) |

Every 8-character value prints its sign and its last letter outside the
well. On PITCH1 and PITCH2 this is any value of ±10 st or more. On IN and OUT
it is any value of ±10 dB or more. `knob.rs:218-219` also keeps a third pill
width, `KNOB_PILL_WIDTH_SMALL` (34 pt, `theme.rs:62`), but no knob has a
radius that selects it.

**Required fix.** Every value of every knob fits inside its pill with at least
1 pt on each side. Widen `KNOB_PILL_WIDTH_MEDIUM` to at least 42 pt (the
centres of the 9-knob rows are 87.5 pt apart), or shorten the format. Remove
the unused SMALL pill width. Update the Knob row of the contract. Do NOT
change: the two knob tiers and the 8 pt monospace face.

### 3. The ROOT popup hides A# and B

**Kind.** Defect. Checklist A: a scroll area where the layout should fit.

**Symptom.** `popup_root`. The popup covers y 714-1137 (423 px, 211.5 pt) and
shows 10 of the 12 roots, C to A. A# and B are below the fold. egui shows the
scroll bar only while the pointer is over it, so nothing in the popup shows
that two more roots exist. The cause is egui's default popup height (200 pt)
for a list that needs about 250 pt (`generator_panel.rs:90-102`). The other
popups (SCALE with 6 items, TOL with 5, TRANSPORT with 2) fit.

**Required fix.** All 12 roots are visible when the popup opens, with no
scroll. Do NOT change: the order of the roots and the selected-item fill.

### 4. CALIBRATE lights orange while a trim runs in the background

**Kind.** Drift. Contract, Components "Action": "A lit action is an orange
fill … only as the call to action of a state: … CALIBRATE when a rebuild is
due. Any other action is never lit". This is new since round 21 (#83).

**Symptom.** `calibrating`: the tile at x 2248-2407 fills `ACCENT_ORANGE` and
reads `CALIBRATING...` (`TEXT_ON_ACCENT`, 5.78:1). The status line also reads
CALIBRATING (`TEXT_SECONDARY`, 7.3:1). `tweaks_panel.rs:52-62` lights the tile
on `needs_rebuild || calibrating`. While the trim runs, no rebuild is due:
the click has already written the trim, so `needs_rebuild` is false. The lit
fill then means "busy", and in `sync_pending` it means "click me". The two
states differ only by the label text. `tweaks_panel.rs:22-23` and
`lib.rs:355-358` set the busy flag after a topology switch and after a project
load too. So the action lights up, unasked, each time the user leaves
ACCURATE. The busy state also shows twice: in the tile and in the status
line.

**Required fix.** CALIBRATE is lit only when a rebuild is due. While a trim
runs, the tile is raised (or disabled, with a reason on hover), and the status
line alone says CALIBRATING. Do NOT change: SYNC PENDING in orange text
(the contract permits it) and the tile size.

### 5. The piano roll marks the current note in orange text at 3.53:1

**Kind.** Drift. Colour roles: orange as text only in the CALIBRATE status
line. Contrast: enabled text ≥ 4.5:1.

**Symptom.** `popup_piano`, active row (y ≈ 926-938). The note name `C3` at
the right of the row is `ACCENT_ORANGE` on the `SURFACE_ELEVATED` row
highlight: 3.53:1 (`text --box`, `#EF641F` on `#3A3A3A`).
`piano_roll_popup.rs:288-290` paints it. The key at the left of the same row
is already an orange fill with `TEXT_ON_ACCENT` (6.0:1), so the row marks the
selection twice.

**Required fix.** The row label of the active note uses a text token at
≥ 4.5:1 on the row highlight (`TEXT_PRIMARY` is 9.63:1 on `#3A3A3A`). The
orange key alone marks the selection. Do NOT change: the key colours.

### 6. Two TRANSPORT combos have no caption and no tooltip

**Kind.** Design, with the symptom of a checklist I defect (round-21 #1).

**Symptom.** TRANSPORT row 5 (`latest`, y 1140-1190): `16th ▼` and
`Direct ▼`. No caption, and no `on_hover_text` (`sequencer_panel.rs:418`,
`:471`). The comment at `:405` names them "Timebase" and "Mode". In
`toggles` they read `12th` and `Decoupled`. The open popups list
`16th`/`12th` and `Direct`/`Decoupled`, still with no word that says what
they set. Every other TRANSPORT field has a caption (CH:, BPM:, LEN:).

**Required fix.** Each combo says what it sets: a caption in the TRANSPORT
label style (8 pt), a hover tooltip, or both. Add both combos to the Tooltip
row of the contract and to `gui_tooltips.rs`. Do NOT change: the 28 pt row
height and the two-slot grid.

### 7. The ROOT/SCALE row is off the column grid and moves with ROOT

**Kind.** Drift. Checklist G (rows that do not share an edge) and D (a row
that reflows when a value changes). Round-21 #3.

**Symptom.** PATTERN STRUCTURE, second label column, `text --box`: STEPS (n):
starts at x 201 and PITCH LEN: at x 202. SCALE: starts at x 217 in `latest`
(ROOT C) and at x 226 in `generator` (ROOT D#). `edges`: the SCALE well
ends at x 474 in `latest` and at x 466 in `generator`. The ROOT combo grows
with its text, because `.width(42.0)` is a minimum (`generator_panel.rs:90-92`).

**Required fix.** ROOT and SCALE have fixed widths, wide enough for `C#` and
the longest scale name. SCALE: starts on the x of STEPS (n): and PITCH LEN:,
and does not move for any ROOT or SCALE value. Do NOT change: the three-group
layout and the separators.

### 8. Popups merge into the controls under them

**Kind.** Design. No contract rule covers popups. A fix with a shadow or an
outline would change the Planes rule "No outlines, no shadows".

**Symptom.** The ComboBox popups fill `#3A3A3A`, which is `SURFACE_ELEVATED`,
the fill of every tile at rest. `popup_scale` opens over the SEQUENCER. Where
its right edge crosses the step 03 ACC tile (x ≈ 465, y ≈ 900), popup and
tile are the same colour: 1.00:1. At 3x, the tile reads as part of the popup,
with its `ACC` cut to `C`. The piano roll fills `PANEL_BG`
(`piano_roll_popup.rs:168`) and opens over the OUTPUT CLIPPER and generator
bodies, which are also `PANEL_BG`: 1.00:1 along its right edge, x ≈ 305. Only
the card header strips and chassis gutters under a popup show where it ends.

**Proposal.** Give a popup a plane that no control under it uses, or a
separation mark. In the direction: a popup fill that no resting surface uses,
with a `CHASSIS_BG` margin. Outside the direction: egui's popup shadow, or a
1 pt `CHASSIS_BG` stroke. The user decides. Do NOT change: the popup
contents and the selected-item fill.

### 9. RUN shows its state only through a 3 × 10 pt LED

**Kind.** Design. This would change the Boolean row for RUN, or add a row
for the transport. Checklist J: hierarchy and state legibility.

**Symptom.** `latest` against `running`: in the RUN tile (26,358 px at 2x),
329 px change, and 100 of them are orange: the LED. At 1x that is 30 px in a
tile of 190 × 28 pt. RUN has the same size, fill and label weight as SYNC,
MIDI, SYNTH, < SHIFT, > SHIFT, JITTER and CLEAR. RUN is the primary control
of the card. The stopped and running states differ by 1.2% of its tile, while
the playhead (8 px LED, plate, number) shows the state much more clearly in
the SEQUENCER.

**Proposal.** In the direction: a longer LED (the 8 px full-width bar that the
playhead uses, along the tile's bottom edge), a transport glyph (▶ / ■), or a
RUN tile one row taller than the others. Outside the direction: a lit fill
while running (the contract says "the tile never fills orange"). Do NOT
change: the ToggleButton component for the other booleans.

---

## LOW

10. **A disabled segmented bar or toggle disappears into a header strip.**
    Design; would change the Dimming rows "Segment: unselected / off" and
    "Toggle". `SURFACE_PRESSED` `#303030` on `PANEL_HEADER_BG` `#2F2F2F` is
    1.01:1. `accurate`: QUALITY LOW, HIGH and BEST, and the ADV ENV tile.
    `clipper_off`: GE, 4X and the LINK tile. The selected SI and 2X keep a
    visible `SURFACE_HOVER` tile, so in each bar one tile shows and the other
    labels look like loose words. The labels read (3.72-3.98:1). Proposal, in
    the direction: in a header strip, draw disabled tiles on `PANEL_BG` or
    `WELL_BG`. (Round-21 #4.)
11. **Combo values in three treatments.** Drift (Type: every numeric readout
    is monospace). TOL `0%` is 8.5 pt proportional (h = 12), left-aligned at
    x 1773 in a column of centred monospace DragValues (AGE `0 yrs`, h = 16).
    ROOT and SCALE use 9.5 pt (h = 13). TRANSPORT `16th`/`Direct` use 8.5 pt
    (h = 12). (Round-21 #5.)
12. **GENERATE and MUTATE labels are left-aligned.** Drift (Components,
    Action). Both labels start at x 1653 in tiles from x 1636 to x 2407
    (centre x 2021). CLEAR, < SHIFT, NEW SEED and ENABLE FUN MODE centre their
    labels. (Round-21 #6.)
13. **The contract gives no token for action labels at rest.** Contract gap.
    GENERATE, MUTATE and CLEAR measure `TEXT_PRIMARY` (9.28:1 on `#3A3A3A`).
    The Colour roles table gives `TEXT_SECONDARY` for "the text on an unlit
    tile". (Round-21 #7.)
14. **NEW SEED and the SEED it changes are in different cards.** Design
    (checklist I). NEW SEED is in the ACID EUCLIDEAN GENERATOR header
    (x 1486-1597). SEED is a Readout in GENERATION (x 1948-2015).
    (Round-21 #8.)
15. **Two knobs are labelled DRIVE.** Design (naming). SOUPED-UP DRIVE
    (`souped_panel.rs:136`, `0.0 dB`) and OUTPUT CLIPPER DRIVE
    (`output_clipper.rs:93`, `50%`) have labels at the same x (90-133), one
    card above the other. (Round-21 #9.)
16. **The sign on zero is not consistent.** Drift (checklist F). OCT RANGE
    MIN shows `0` and MAX shows `+1` (`generator`). The knobs show zero as
    `+0.0`. (Round-21 #10.)
17. **The armed CONFIRM CLEAR looks like any hovered tile.** Design (state
    legibility); would change the Action row. `confirm_clear`: the tile turns
    `#494949` (`SURFACE_HOVER`), which is also the fill of RUN and GENERATE
    under the pointer (`hover_runtile`, `hover_generate`). The pointer is
    still on the tile after the click, so only the relabel shows the armed
    state.
18. **Lit ACC and SLD pills give no hover feedback.** Drift (Colour roles:
    `ACCENT_ORANGE_HOVER` for a hovered lit tile). `hover_step_acc`: the lit
    ACC pill stays `#F6661E`. `step_button.rs:102-108` and `:133-139` return
    the lit colours before they test hover.
19. **The contract says a hovered well darkens. It gets lighter.** Contract
    defect (Planes). `hover_well`: the AGE well goes from `#1A1A1A` to
    `#303030`. `hover_pitch_cell`: the pitch cell does the same. The code is
    consistent with itself.
20. **The piano-roll title is monospace.** Drift (Type: words are
    proportional). `STEP 01 NOTE` uses `FontId::monospace(8.5)`
    (`piano_roll_popup.rs:179-181`).
21. **Generator group captions look like the field labels.** Design
    (grouping). PATTERN STRUCTURE, OCTAVE BEHAVIOR and PROBABILITIES measure
    h = 12, `#B4B4B4`, 7.30:1. ROOT: and OCT UP: measure the same. Only the
    colon and the position make the caption different from a label.
22. **Labels use colons in some cards only.** Design (naming consistency).
    With a colon: ROOT:, SCALE:, PULSES (k):, MUTATION:, SEED:, CH:, BPM:,
    LEN:. Without: TOL, AGE, NOISE, CMP SEED, WAVE, VCF TAPS, DIODE, OS,
    QUALITY, and every knob label.
23. **With SYNTH off, the synth cards still look live.** Design (state
    legibility). `toggles`: SYNTH off sends silence (`lib.rs:599`), but
    VOICE, SOUPED-UP ENGINE, OUTPUT CLIPPER and ENVELOPES are identical to
    `latest`. The only cue is the unlit SYNTH LED and its tooltip
    (`sequencer_panel.rs:217`). OUTPUT CLIPPER dims its card when it is off.
    Proposal: when SYNTH is off, show a status in the app bar (in the
    direction). The controls stay editable.
24. **One action has three names.** Design (naming). The tile says
    CALIBRATE. The status says SYNC PENDING (`tweaks_panel.rs:308`). The
    tooltip says "Trim the VCO pitch". A user cannot link "SYNC" to CALIBRATE.
25. **The ACC and SLD tooltips name shortcuts that do not exist.** Defect
    (tooltip text). "Toggle Accent (A)" and "Toggle Slide (S)"
    (`step_button.rs:91`, `:122`). `plugin303/src` handles no `Key::A` or
    `Key::S`; the only key it handles is Escape (`piano_roll_popup.rs:129`).
    If "(A)" names the abbreviation, the tooltip must say so.
26. **The step-LED tooltip names the widget.** Design (naming).
    "Step LED strip" (`step_button.rs:54`) does not say that the LED marks the
    playing step.

---

## Flagged, intent unclear — not asserted as defects

- **CUTOFF and GLIDE readouts change with ADV ENV.** With the knobs fixed,
  CUTOFF goes from 867 Hz to 894 Hz, and GLIDE from 5.5 ms to 170 ms.
  The readouts are truthful (`value_text.rs:40-60`). The cause is a toggle in
  another card. The tooltips could say that these readouts depend on ADV ENV.
- **The scale names are shortened.** `Pentatonic` is `MinorPentatonic`, and
  `Harmonic` is `HarmonicMinor` (`generator_panel.rs:106-111`).
- **TRANSPORT labels CH:, BPM: and LEN:** are 8 pt glyph cores (6.74-6.98:1),
  not drift.
- **ENABLE FUN MODE is a lit fill in ACCURATE.** Per the Action row.

## Capture gaps for the next round

- Make `knob_extremes`, `clipper_off`, `sync_pending` and `calibrating`
  permanent QA states. #2, #4 and #10 need them to be proven again, and the
  round-21 attempt was reverted with the fixes.
- How long the lit CALIBRATING state lasts after a topology switch (#4) needs
  a running plugin.
- A window capture: the title bar, GL text rasterisation, and real pointer
  press (`SURFACE_PRESSED`). The hover renders here use synthetic pointer
  events.
- A 1x display: 8 pt knob values and 7.5 pt fader values render at 8 and
  7.5 px there.
- A sequence length below 16 (LEN 12): how the steps after the end look.
- Keyboard focus; drags on knobs and DragValues; the ROOT popup scrolled
  (#3); the CONFIRM CLEAR expiry after 3 s.
- Every remaining tooltip. This round rendered 9 of them.

## Most urgent

1. **#1**: the only close mark in the piano roll is a missing glyph.
2. **#2**: shipped values print outside their pills over half the PITCH and IN/OUT ranges.
3. **#3**: two of the twelve roots are hidden, and nothing shows that they exist.
4. **#4**: the new background CALIBRATE gives the lit action a second meaning ("busy").
