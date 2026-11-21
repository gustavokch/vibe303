# TECHNEO 303 — GUI Visual Audit, Round 23

Date: 2026-11-21
Captures: `target/gui-qa-offscreen` (`latest`, `accurate`, `advanced`, `running`,
`running_note`, `knobs`, `knob_extremes`, `clipper_off`, `quality_low`,
`generator`), all 2432x1280, offscreen render.
Gap renders: `/tmp/r23_gaps` (77 renders, listed below), all 2432x1280, offscreen
render from a throwaway worktree. The worktree added parameter overrides and
synthetic pointer and key events only. Its GUI source was the baseline's. It is
removed.
Baseline: commit `9c428e6` "Merge pull request #93 from gustavokch/perf/tier-2x"
on `main`.

Since round 22 (`76dfe3a`) the GUI source changed in three commits: `4f2d55a`
(the 26 round-22 fixes), `3e260d0` (lit actions and CLEAR answer the pointer) and
`59f17cd` (a third LARGE knob, BRIGHT, in ANALOG TWEAKS). This round re-measures
every round-22 item and audits BRIGHT.

**Status after the fix pass (same day):** findings 1-11 are fixed on branch
`fix/gui-audit-round23`. The flagged items stay open, except CUTOFF's tooltip.
See "Fix pass" at the end.

## Capture set

| File | State |
|---|---|
| `latest.png` | defaults: FUN, QUALITY DRAFT, stopped |
| `accurate.png` | ACCURATE: SOUPED-UP banner, ANALOG TWEAKS readouts, QUALITY and ADV ENV disabled |
| `advanced.png` | ADV ENV on: both ADSR banks enabled |
| `running.png` | running, playhead on step 04 (a REST step) |
| `running_note.png` | running, every step NOTE |
| `knobs.png` | TUNING +25.0 ct, CUTOFF 0.75 (1.4 kHz) |
| `knob_extremes.png` | TUNING -100 ct, PITCH1 -24 st, PITCH2 +24 st, IN -18 dB, OUT +18 dB, TILT -6 dB |
| `clipper_off.png` | OUTPUT CLIPPER off |
| `quality_low.png` | QUALITY LOW |
| `generator.png` | every generator field off its default, SLD CHAIN on |

Gap renders (throwaway, not in the repo):

| File | State |
|---|---|
| `bright_lo`, `bright_mid`, `bright_hi` | BRIGHT 50%, 87%, 150% (with DRIFT 0% / 100%, RECAL 0% / 50%) |
| `bright_hi_accurate` | ACCURATE with BRIGHT stored at 150% |
| `len12`, `len12_running`, `len01` | LEN 12 (stopped, running) and LEN 01 |
| `quality_high`, `quality_best` | existing `QaState`s that no script renders |
| `toggles` | WAVE SQR, MIDI on, SYNTH off, LINK off, JITTER on, 12th, Decoupled |
| `wave_swp`, `accurate_adv` | WAVE SWP; ACCURATE with ADV ENV on |
| `sync_pending`, `calibrating` | a rebuild is due; a trim runs in the background |
| `running_acc`, `running_tie`, `running_sld` | playhead on step 01 (lit ACC), 02 (TIE), 03 (lit SLD) |
| `confirm_clear`, `confirm_clear_away` | CLEAR armed, pointer on the tile and gone |
| `popup_root`, `popup_scale`, `popup_rate`, `popup_mode`, `popup_tol` | each ComboBox popup open |
| `popup_piano`, `piano_settled`, `piano_row_hover2`, `piano_row_click`, `piano_esc`, `piano_close_tip2` | the piano roll: opening, settled on the active note, a hovered row, a clicked row, Escape, the close tile hovered |
| `tip_*` (20) | tooltips: BRIGHT, DRIFT, BRIGHT in ACCURATE, QUALITY and ADV ENV in ACCURATE, the MEG bank, a knob of the bypassed clipper, CUTOFF, RATE, MODE, TOL, AGE, a lit ACC, the step LED, SYNTH OFF, NEW SEED, ENABLE FUN MODE, CALIBRATE when due |
| `hover_*` (14) | hover fills: lit and unlit ACC, lit SLD, RUN, GENERATE, the AGE well, a pitch cell, the BPM well, a knob, a 2-POLE segment, LINK, SLD CHAIN, ENABLE FUN MODE, CALIBRATE when due |
| `press_generate`, `press_run`, `press_knob` | the primary button held down |
| `focus_tab1`, `focus_tab6` | Tab pressed once and six times |

Pixel diffs confirm the states are distinct and correctly scoped:

- `accurate`: the app bar (y 14-49), the ADV ENV tile (y 66-101), the SOUPED-UP
  header and body (y 272-464) and ANALOG TWEAKS (knob labels y 481-493, CMP SEED
  y 501-515, status y 554-566).
- `advanced`: ENVELOPES (y 74-248), the CUTOFF pill (867 Hz → 894 Hz) and the
  GLIDE pill (y 401-414, 5.5 ms → 170 ms).
- `knobs`: TUNING and CUTOFF. `knob_extremes`: TUNING, PITCH1, PITCH2, IN, OUT
  and TILT. `quality_low`: the QUALITY bar. `clipper_off`: the OUTPUT CLIPPER
  card.
- `generator`: the generator body and one cluster in GENERATION (y 663-677,
  x 1779-2002). The cluster holds two values: MUTATION (15% → 25%) and SEED
  (0042 → 1337). The recipe table listed SEED only. It now lists both.
- `running`: step 04 (plate, LED, number) and the RUN tile. `running_note`: as
  `running`, plus every gate and pitch cell.
- `bright_*`: the three ANALOG TWEAKS knobs and their pills only (y 389-461,
  y 502-515). `calibrating`: 189 px of the status line. `sync_pending`: the
  CALIBRATE tile and the status line. `len12`, `len01`: the LEN value only
  (see #3). `accurate_adv` against `accurate`: 120 px, the ADV ENV LED.
- `focus_tab1` and `focus_tab6` are IDENTICAL to `latest` (see Flagged).

## Previous round (22)

All 26 items are **FIXED**. Each one was measured again:

- **1 Piano-roll close button is a missing glyph: FIXED.** `piano_settled`: a
  32 px tile (x 260-291, y 470-501) with a painted cross. It lifts from
  `#3A3A3A` to `#494949` under the pointer. Escape closes the popup
  (`piano_esc` differs from `latest` in the hovered pitch cell only).
- **2 MEDIUM knob pills narrower than their values: FIXED.** `knob_extremes`:
  every MEDIUM pill is 88 px. PITCH1 `-24.0 st`: ink x 597-674 in pill x 592-679
  (5 px each side). OUT `+18.0 dB`: ink x 946-1024 in pill x 942-1029 (4 and
  5 px). TUNING `-100.0 ct`: 8 px each side in the 104 px LARGE pill.
- **3 ROOT popup hides A# and B: FIXED.** `popup_root`: y 680-1249, 12 rows,
  C to B, no scroll.
- **4 CALIBRATE lit while a trim runs: FIXED.** `calibrating`: the tile stays
  `#3A3A3A`. `sync_pending`: `#F6661E`, label 5.85:1.
- **5 Piano-roll active label in orange text: FIXED.** `C3` reads
  `TEXT_PRIMARY`, 9.63:1 on the row highlight.
- **6 TRANSPORT combos without caption or tooltip: FIXED.** RATE (x 1638-1672)
  and MODE (x 2025-2068), h = 11, 7.3:1. `tip_rate` and `tip_mode` show their
  tooltips.
- **7 ROOT/SCALE row off the grid: FIXED.** SCALE starts at x 201 in `latest`
  (ROOT C) and in `generator` (ROOT D#). STEPS (n) starts at x 201 and PITCH LEN
  at x 202. The row ends at x 467 in both captures.
- **8 Popups merge into the controls under them: FIXED.** Every popup and
  tooltip carries a 6 px `#151515` band (`popup_scale` x 302-307, `piano_settled`
  x 17-22 and x 300-305, `tip_bright` x 1640-1645).
- **9 RUN shows its state through a 3 × 10 pt LED: FIXED.** `running`: one
  8 px bar at y 902 across the tile. TRANSPORT holds 3,132 orange px running
  and 216 stopped.
- **10 Disabled bar disappears into a header strip: FIXED.** QUALITY LOW, HIGH
  and BEST, ADV ENV, GE, 4X and LINK fill `#1A1A1A` on `#2F2F2F` (1.30:1). The
  selected DRAFT, SI and 2X fill `#494949`.
- **11 Combo values in three treatments: FIXED.** TOL `0%`: h = 13, x 1826-1845,
  the same x as NOISE `0%` (x 1826-1846).
- **12 GENERATE and MUTATE labels left-aligned: FIXED.** GENERATE: x 1983-2061,
  centre 2022, in a tile centred on 2021.5.
- **13 No contract token for action labels: FIXED.** The Colour roles table
  gives `TEXT_PRIMARY` to "the label of an action at rest". GENERATE measures
  9.28:1.
- **14 NEW SEED and SEED in different cards: FIXED.** NEW SEED follows the SEED
  readout in GENERATION (y 652-687).
- **15 Two knobs labelled DRIVE: FIXED.** SOUPED-UP reads VCF DRIVE.
- **16 Sign on zero: FIXED.** `generator`: OCT RANGE MIN `+0`.
- **17 Armed CONFIRM CLEAR looks like a hovered tile: FIXED.** `#F6661E` with
  the pointer gone, `#FF7F3A` under it, label 5.8:1.
- **18 Lit ACC and SLD pills give no hover feedback: FIXED.** Lit ACC
  `#F6661E` → `#FF7F3A`. Lit SLD `#5CA8EE` → `#7ABAF2`. Unlit `#3A3A3A` →
  `#494949`.
- **19 Contract says a hovered well darkens: FIXED.** The contract says it
  lifts. The AGE well, a pitch cell and the BPM well go `#1A1A1A` → `#303030`.
- **20 Piano-roll title is monospace: FIXED.** `STEP 01 NOTE` is 9 pt
  proportional (`piano_roll_popup.rs:209`), 12.35:1.
- **21 Group captions look like field labels: FIXED.** Each caption carries a
  2 px `#1A1A1A` rule to the group's edge (PATTERN STRUCTURE y 663-664, MEG
  y 125-126).
- **22 Colons in some cards only: FIXED.** No label in any capture or in
  `gui/panels` carries a colon.
- **23 SYNTH off, cards look live: FIXED.** `toggles`: SYNTH OFF in the app bar
  (x 1174-1258, `TEXT_PRIMARY`, 10.63:1), with a tooltip that names the toggle.
- **24 One action, three names: FIXED.** CALIBRATE, CALIBRATION DUE,
  CALIBRATING, CALIBRATED, and "Calibrate the VCO pitch …".
- **25 ACC and SLD tooltips name shortcuts that do not exist: FIXED.** "Lit:
  this step is accented (louder, with a stronger filter sweep)." (`tip_acc`);
  `SLD_HINT` names no key (`step_button.rs:23`).
- **26 Step-LED tooltip names the widget: FIXED.** "Lit: the sequencer is
  playing this step." (`tip_step_led`).
- `3e260d0` (after round 22): **verified.** ENABLE FUN MODE, CALIBRATE when due
  and CONFIRM CLEAR lift from `#F6661E` to `#FF7F3A` under the pointer.
- Flagged *CUTOFF and GLIDE readouts change with ADV ENV*: **unchanged**, and
  wider now (see Flagged).
- Flagged *scale names shortened*: **unchanged.** Carried.
- Round-22 capture gaps: `knob_extremes` and `clipper_off` are permanent QA
  states now. LEN below 16, 20 more tooltips and a pressed button were rendered
  this round. `sync_pending` and `calibrating` are still throwaway states.

The GUI tests (`cargo test -p plugin303`, the `gui_*` binaries) were **not** run
this round: see Capture gaps.

## Geometry

Cards match the contract's landmark table in all 10 captures and all 77 gap
renders: 10 cards, every `gap_above=6`, column seam at x 1614 | 1620. Knobs: 28
in FUN (10 LARGE, 18 MEDIUM) and 19 in ACCURATE. The contract and the recipes
said 27 and 18; they predate BRIGHT and are corrected (see Tooling). The arc tip
is within 2.3°/0.9 px of the pointer on every knob in `latest`, `knobs`,
`knob_extremes`, `accurate` and `clipper_off`. Every bipolar knob at a limit
fills from the centre. The disabled knobs in `accurate` and `clipper_off` draw
`muted` arcs. No probed text is below 3:1 in any state. The lowest values are
disabled: QUALITY DRAFT (selected) 4.34:1, the MEG caption 4.38:1 and DRIFT in
ACCURATE 4.38:1. Orange totals: 29,188 px at rest and 33,620 px running (round
22: 28,650 and 30,274). The difference is BRIGHT's arc (538 px) and RUN's LED
bar.

---

## HIGH

### 1. WAVE is drawn live in FUN, where the voice ignores it

**Kind.** Defect. Checklist D: an inert control that does not say so. Colour
roles: orange is live state. Dimming: a disabled control says why on hover.

**Symptom.** VOICE, WAVE (x 24-131, y 136-247), in every FUN capture. FUN is the
default mode. In FUN the voice discards the WAVE parameter: `voice.rs:866-873`
forces `OscWave::Sweep`, and the SWEEP knob alone sets the shape ("the discrete
waveform switch is ignored"). `params.rs:492` reports `waveform` as inactive in
FUN, and `tests/topology.rs:9-10` asserts it. No GUI code reads that helper. The
editor draws WAVE enabled, with an orange selected segment
(`top_controls.rs:64`). `toggles` shows SQR lit and `wave_swp` shows SWP lit,
both in FUN, with SWEEP at 0%: the voice plays a saw in both. With SWEEP at
100% the voice plays a square while WAVE shows SAW lit. WAVE has no tooltip.

ACCURATE has the mirror fault. There WAVE is live, but `voice.rs:845-849`
coerces SWP to SAW. The SWP segment stays selectable.

**Required fix.** In each mode, WAVE shows only what the voice plays. Either
the editor follows the voice or the voice follows the editor; the user decides.
If the editor follows: in FUN, WAVE takes the disabled paint (selected segment
`SURFACE_HOVER`, never orange) and says why on hover (SWEEP sets the waveform in
FUN); in ACCURATE, SWP cannot be selected, or says that it plays SAW. Add WAVE
to `gui_tooltips.rs`. Do NOT change: the segment geometry, the SWEEP knob and
its tooltip.

---

## MEDIUM

### 2. ACCURATE says the engine is bypassed, but R-COMP still acts and is hidden

**Kind.** Defect. Checklist D: a placeholder that misstates the state of what
it replaces.

**Symptom.** `accurate`, SOUPED-UP ENGINE body: the banner reads "ACCURATE MODE
ACTIVE — Souped-up engine is bypassed" (`souped_panel.rs:54-55`) and replaces
all nine knobs. One of them is not bypassed. In ACCURATE the voice zeroes or
fixes the other souped parameters, but passes `reso_comp` through
(`voice.rs:862`) and applies it to the VCA gain (`voice.rs:1691`). R-COMP is at
0% by default, so the banner is true out of the box. This comes from the
source: no render shows it.

**Correction (fix pass).** The first version of this finding said that a user
who sets R-COMP in FUN and switches to ACCURATE keeps the make-up gain. That
is wrong for the editor: its TOPOLOGY switch writes R-COMP to 0% on entering
ACCURATE and to 50% on entering FUN (`header_bar.rs:92-106`, since `73ca42b`),
and ENABLE FUN MODE writes 50% (`souped_panel.rs:118-120`). R-COMP above 0%
in ACCURATE arises only when the host changes TOPOLOGY or R-COMP: an
automation lane, the host's own parameter view, or a stored state. The
finding holds for those paths only.

**Required fix.** The banner is true for every knob it hides. Either ACCURATE
bypasses R-COMP like the other eight (the user decides: it changes the sound of
ACCURATE with R-COMP above 0%), or R-COMP stays visible and editable in
ACCURATE. Do NOT change: the banner's title, the ENABLE FUN MODE action, the
card geometry.

### 3. Steps past LEN look the same as the steps in the loop

**Kind.** Design. No contract rule covers the sequence length. Checklist J:
state legibility.

**Symptom.** `len12` against `latest`: 108 px differ, all in the LEN well
(y 1090-1104, x 2233-2241). `len01`: 211 px, the same well. The SEQUENCER card
is identical in all three. With LEN 12 the sequencer plays steps 01-12
(`lib.rs:388`); steps 13-16 keep their lit ACC and SLD pills, their NOTE gates
and their bright pitch text. With LEN 01, fifteen of the sixteen steps are
never played and nothing in the grid says so. `render_sequencer_grid`
(`sequencer_panel.rs:80`) reads the playing step only, not the length. The LEN
well sits in another card, right of the seam.

**Proposal.** In the direction: a step past LEN keeps its controls editable but
takes muted tokens (step number, labels and pitch in `TEXT_MUTED`; lit pills
keep their hue), or the LED slot of a step past LEN is not drawn. Add the rule
to the Step column row. Do NOT change: the 2 × 8 grid, the column rhythm, the
playhead.

### 4. BRIGHT at its neutral 100% draws a half ring, beside RECAL whose 100% is a full ring

**Kind.** Design. Would change the Knob row: "orange arc from the minimum, or
from 0 when the range's plain minimum is below 0".

**Symptom.** `latest`, ANALOG TWEAKS. BRIGHT (2338.8, 430.5): pointer 0.0°
(0.500), arc [-135.8°, 0.3°] `from=min`, value `100%`. RECAL (2201.5, 430.5),
137 px to its left: pointer 135.0° (1.000), full arc, value `100%`. Two
neighbours read `100%` at two positions. BRIGHT's range is 50-150% around a
calibrated 100% (its tooltip: "around its calibrated setting (100%)"). The
other seven knobs with a neutral point (TUNING, VOLUME, PITCH1, PITCH2, IN, OUT,
TILT) draw no arc there and fill outward from it. BRIGHT is the only knob whose
untouched setting draws half a ring, so the ring cannot show at a glance
whether BRIGHT is trimmed. `knob.rs:144` picks the origin from the sign of the
plain minimum, and 50% is not below 0. The values agree with the positions at
both limits (`bright_lo`: 50%, no arc; `bright_hi`: 150%, full arc).

**Proposal.** In the direction: BRIGHT fills from its 100% detent, left to 50%
and right to 150%, with no arc at 100%. The Knob row gains "or from the
parameter's neutral value (BRIGHT: 100%)". Do NOT change: the LARGE tier, the
`100%` text (the host shows the same string), the double-click reset.

---

## LOW

5. **ANALOG TWEAKS keeps 32 pt idle between the fields and the knob bank.**
   Layout drift ("banks spread across the full width"). The field column
   content ends at x 1915 (60 pt label + 80 pt field); `FIELD_COL_W` reserves
   172 pt (`tweaks_panel.rs:89`, whose comment says label + widest field). The
   three pills run x 2013-2390: 98 px before the first, 33 px between pills,
   18 px after the last. Two knobs hid the slack; three do not.
6. **Disabled values come in two weights in one card.** Design; would change
   the Dimming row "Knob". `accurate`, ANALOG TWEAKS: CMP SEED `1337` is
   `TEXT_SECONDARY` (8.77:1) and BRIGHT `100%` is `TEXT_PRIMARY` (14.73:1),
   both inert, 500 px apart. The three knob values are the brightest text in
   the disabled card, brighter than its dimmed title. The fader values of the
   disabled ADSR banks are `TEXT_SECONDARY` too. Proposal, in the direction: a
   disabled knob value is `TEXT_SECONDARY`.
7. **Three set-and-forget knobs outweigh the eighteen sound knobs.** Design
   (hierarchy, accent budget); would change the Knob row's tiers. DRIFT, RECAL
   and BRIGHT are LARGE (r 21 pt) like the seven VOICE knobs; the SOUPED-UP and
   OUTPUT CLIPPER knobs are MEDIUM (r 12 pt). Orange in the card bodies at
   rest (`accent`): ANALOG TWEAKS 1,743 px for three knobs, OUTPUT CLIPPER
   842 px and SOUPED-UP ENGINE 380 px for nine each. RECAL at its default is
   the only full ring in the window. Proposal, in the direction: MEDIUM knobs
   in ANALOG TWEAKS, with #5's width given to the fields. The user decides.
8. **Toggles, segments, step pills and knobs show no press.** Drift (Planes:
   "press drops it to `SURFACE_PRESSED`"). `press_run` is identical to
   `hover_run` (0 px differ); `press_knob` to `hover_knob`. GENERATE drops from
   `#494949` to `#303030`. `toggle_colors`, `segment_colors` and `flag_colors`
   take `hovered` and no pressed state (`toggle.rs:73`, `switch.rs:76`,
   `step_button.rs:137`). Either the widgets drop on press, or the contract
   limits the rule to actions.
9. **CALIBRATING and CALIBRATED differ by three letters.** Design (state
   legibility). `calibrating` against `latest`: 189 px, x 1833-1856 of one
   8.5 pt word in the same colour. The tile is correctly raised (round-22 #4),
   so the status line is the only cue that a trim runs.
10. **NOISE reads percent in the editor and degrees in the host.** Design
    (naming); would extend the Knob row's value-text rule to value fields. The
    editor prints `{v}%` (`tweaks_panel.rs:266`). The parameter's own text is
    `Off` at 0 and the ambient temperature above it, `30.0°C` at 50
    (`params.rs:880-886`). One control has two readings.
11. **ANALOG TWEAKS tooltips come in two styles.** Design (naming). AGE:
    "Component Aging (0-100 years drag)", NOISE: "Thermal Noise floor (0-100%
    drag)", CMP SEED: "Component Variation Seed (0000-9999 drag)"
    (`tweaks_panel.rs:236`, `:268`, `:303`). TOL, DRIFT, RECAL and BRIGHT use
    "Name: effect" in sentence case (`tip_tol`, `tip_drift`, `tip_bright`).

---

## Flagged, intent unclear — not asserted as defects

- **CUTOFF's readout ignores BRIGHT.** CUTOFF reads `867 Hz` at BRIGHT 50%,
  100% and 150% (`bright_lo`, `latest`, `bright_hi`: no pixel of the pill
  changes). BRIGHT scales the played corner by 0.5 to 1.5 (`voice.rs:1605`,
  `:1741-1743`). `value_text.rs:44` calls the readout "the base cutoff", and
  BRIGHT's tooltip says it moves the corner "around its calibrated setting", so
  the readout is probably meant as nominal. AGE has shifted the corner the same
  way since before BRIGHT. The readout does follow ADV ENV (867 → 894 Hz), so
  it follows one dependency and not the other. Still open either way: CUTOFF
  has no tooltip (`tip_cutoff`: 0 px of tooltip) to say what its Hz means.
- **GLIDE's readout changes with ADV ENV** (5.5 ms → 170 ms). Carried.
- **BRIGHT sits in the component-wear card.** It is a tone control: it moves
  the filter corner like CUTOFF (VOICE, 1,850 px away). Its neighbours feed
  CALIBRATE, whose tooltip lists "AGE, TOL, CMP SEED and RECAL"; BRIGHT is not
  part of a calibration and does not make one due. SOUPED-UP ENGINE has no free
  slot (nine knobs on a 175 px pitch), and `59f17cd` placed BRIGHT here on
  purpose. The user decides whether the grouping holds.
- **Keyboard focus shows nothing.** `focus_tab1` and `focus_tab6` are
  IDENTICAL to `latest`. No widget in `gui/` paints `has_focus`. [INFERENCE]
  egui gives focus to click-sensing widgets on Tab and clicks the focused one
  on Space, so a focused TOPOLOGY segment could be switched with no cue. The
  render cannot tell "no focus" from "invisible focus". A render after Tab and
  Space would confirm it.
- **The popups shorten the host's names.** SCALE: `Pentatonic`, `Harmonic`
  (carried). RATE: `16th`, `12th` for "16th (Straight)", "12th (Triplet)".
  MODE: `Direct`, `Decoupled` for "Direct Grid", "Decoupled (Roland 303)". The
  RATE and MODE popups are 250 pt wide. TOL's popup keeps the long form
  (`0% (Ideal)`).
- **CMP SEED reaches 9999 in the editor and 999,999 in the host**
  (`tweaks_panel.rs:296`, `params.rs:889-895`). A seed above 9999 was not
  rendered.
- **The ADSR captions are mixed case** ("MEG ADSR (Mod Envelope)"). Every other
  caption is in capitals.

## Tooling and contract changes made this round

- `scripts/render_gui_offscreen.sh` and `scripts/capture_standalone_gui.sh`
  run the crop step through `uv` when `python3` has no working Pillow. The
  Phase 1 command exited with `ImportError` after writing the PNGs (also noted
  in round 22). It now exits 0 and writes the crops.
- Design contract, Knob row: LARGE lists BRIGHT. `measurement-recipes.md`: 28
  knobs (19 in `accurate`), the `generator` scope, and a triage note for
  `#7B7B7B`. That grey is the edge of a `TEXT_MUTED` label; `colors` reports it
  as `NOT A THEME TOKEN`, 3.58:1, in ANALOG TWEAKS of `accurate` (n = 217),
  where `text --box` reads the labels at 4.4-4.6:1. `SKILL.md`: the knob count
  and two standing gaps.

## Capture gaps for the next round

- **The GUI tests were not run.** The disk held 0.1 to 0.8 GB free during the
  session, and one fat-LTO link pushed swap to 5.4 GB. The first gap build died
  with "No space left on device". Run `cargo test -p plugin303` before a fix
  round.
- WAVE SWP lit in ACCURATE (#1) and R-COMP above 0% in ACCURATE (#2): both
  rest on the source. #2 needs an audio render in ACCURATE at two R-COMP
  values.
- Tab focus past six controls, and Space on a focused control.
- The "Close (Esc)" tooltip of the piano roll. In the offscreen harness no
  tooltip shows after a click: at 0.25 s steps the pointer's velocity stays 0,
  so egui's `clicked_more_recently_than_moved` gate holds (`t_esc_run`,
  `t_clear_run`: RUN's tooltip is missing after a click and present without
  one). Open the popup with `set_open_state` instead of a click.
- Make `sync_pending`, `calibrating`, a LEN below 16 and BRIGHT off 100%
  permanent QA states. #3 and #4 need them to be proven again.
- A window capture: the title bar, GL text rasterisation, a real pointer.
- A 1x display; drags on knobs and DragValues; the CONFIRM CLEAR expiry after
  3 s; a CMP SEED above 9999.

## Most urgent

1. **#1**: the default mode lights a waveform the voice does not play.
2. **#2**: ACCURATE's banner is false for one hidden knob that still shapes the level.
3. **#3**: a shortened pattern shows up to fifteen dead steps as live.
4. **#4**: the new knob reads half full when it is untouched.

## Fix pass (2026-11-21)

Findings 1-11 are fixed on branch `fix/gui-audit-round23`, on top of `9c428e6`.
The measurements are from a fresh `target/gui-qa-offscreen` of eleven states
(`len12` is new). Where no state shows a fix, a test is named instead.

- **1 WAVE: FIXED.** The editor follows the voice. `latest` (FUN): WAVE takes
  the disabled paint: SAW on `#494949` with `TEXT_SECONDARY`, SQR
  `TEXT_MUTED` on `#1A1A1A`, caption `TEXT_MUTED`. On hover it says that the
  SWEEP knob sets the waveform (`fun_wave_explains_why_it_is_inert`).
  `accurate`: SAW `#F6661E`, SQR a raised segment (`#3A3A3A`), SWP a well
  that cannot be picked and says that it plays SAW
  (`accurate_swp_explains_that_it_plays_saw`). A stored SWP in ACCURATE was
  not rendered.
- **2 R-COMP: FIXED.** ACCURATE passes 0 for R-COMP (`voice.rs:862`), and
  `is_param_active("reso_comp")` is false there.
  `test_accurate_topology_dsp_coercion` now sets R-COMP to 100% in ACCURATE.
  It fails without the fix (-0.4767 against -0.5594) and passes with it. Every
  other ACCURATE call in the tests and in `render303` passes R-COMP 0, so no
  other gate moves.
- **3 Steps past LEN: FIXED.** `len12` against `latest`: 57,886 px differ (was
  108), in steps 13-16 (x 814-2241, y 1066-1250). Step number 13 is
  `TEXT_MUTED` (12: `TEXT_SECONDARY`). A set ACC or SLD is `#494949` with
  `TEXT_SECONDARY`, a clear one `TEXT_MUTED` on `#1A1A1A`. NOTE is `#494949`,
  TIE `#3A3A3A`, the pitch `TEXT_MUTED`, and the LED slot is not drawn. The
  steps stay editable. A set flag does not keep its hue: orange or blue would
  still claim a live step. Tests: `steps_past_len_are_muted_and_carry_no_hue`,
  `flags_past_len_keep_their_position_without_a_hue`,
  `gates_keep_their_order_past_len`.
- **4 BRIGHT: FIXED.** `latest` (100%): `arc=NONE`. `knobs` (75%): arc
  [-69.7°, 2.1°] `from=centre`. `knob_extremes` (150%): arc [-0.7°, 136.5°]
  `from=centre`. `Knob303::neutral` sets the origin
  (`bright_fills_from_its_neutral_value`).
- **5, 7 ANALOG TWEAKS layout: FIXED.** 28 knobs, 7 LARGE and 21 MEDIUM (was
  10 and 18). DRIFT, RECAL and BRIGHT sit at x 2010.9, 2169.6 and 2328.3, a
  158.7 px pitch. The field column is the 60 pt label plus the 80 pt field.
- **6 Disabled values: FIXED.** `accurate`: BRIGHT `100%` and NOISE `Off` are
  both `TEXT_SECONDARY`, 8.77:1. `clipper_off`: the DRIVE value is
  `TEXT_SECONDARY`. `latest`: both stay `TEXT_PRIMARY`
  (`disabled_values_share_one_weight`).
- **8 Press: FIXED, proven by test only.** `theme::raised_fill` drops toggles,
  unselected segments, unlit flags, the octave arrows and the piano roll's
  close tile to `SURFACE_PRESSED`. A lit flag or LED returns to its resting
  fill. A knob body stays lifted for the whole drag and does not drop:
  `SURFACE_PRESSED` is 1.13:1 on the panel. The contract's Planes section says
  so. Tests: `tile_shows_rest_hover_and_press`,
  `unselected_segment_shows_rest_hover_and_press`,
  `unlit_flag_drops_while_pressed`, `toggle_tile_drops_while_pressed`.
- **9 CALIBRATING: FIXED, proven by test only.** The word is `TEXT_PRIMARY`
  while a trim runs; CALIBRATED stays `TEXT_SECONDARY`
  (`a_running_trim_reads_brighter_than_a_settled_one`).
- **10 NOISE: FIXED.** The field prints the parameter's own text: `Off` in
  `latest` and `accurate`. Every text the field shows parses back to its
  setting (`noise_text_parses_back_to_its_setting`), and the font has the
  glyphs (`noise_readout_has_its_glyphs`). A NOISE above 0 was not rendered.
- **11 Tooltips: FIXED.** AGE, NOISE and CMP SEED say what the field does.
  CUTOFF has a tooltip that says its readout is the calibrated corner (from
  the flagged list).

**Checks run.** `cargo test -p plugin303 --release` for the lib (92 passed, 7
ignored), `gui_build` (8), `gui_calibrate` (2), `gui_clear_confirm` (3),
`gui_layout_test` (7), `gui_lit_actions` (3), `gui_text_contrast` (5),
`gui_tooltips` (11), `reso_gain_comp` (2) and `topology` (3): all pass.
`pytest tests/test_crop_gui_sections.py`: 2 passed. Clippy reports nothing in
`gui/`. The rest of the suite (the spectral gates) was not run.

**Still not captured.** A pressed control, `calibrating`, `sync_pending`, a
NOISE above 0, a stored SWP in ACCURATE, and the new hover texts have no QA
state. The disk had 2.2 GB free, so no gap worktree was built.
