# TECHNEO 303 — GUI remediation prompt, round 13

Source audit: `docs/superpowers/plans/2026-09-02-gui-audit-round13-report.md`.
Issue numbers below match that report exactly. Keep them stable in your
confirmation.

---

## Context

TECHNEO 303 is a TB-303-style bass synth plugin (Rust, nih-plug with
nih-plug-egui). GUI source lives in `plugin303/src/gui/`: panels under
`panels/`, shared widgets under `widgets/`, palette tokens in `theme.rs`.

The window captures at 2732x1984 physical pixels (2x DPI, so 1366x992
logical). All coordinates in this prompt are **physical** pixels from the
captures in `target/gui-qa/`. Measurement tool: `scripts/gui_qa_measure.py`
(subcommands `report`, `panels`, `deadspace`, `colors`, `accent`, `knobs`,
`text --box`, `edges --box`, `strokes --box`, `crop`). Capture harness:
`scripts/capture_standalone_gui.sh`, state selection via `VIBE303_QA_STATE` in
`plugin303/src/params.rs:414-426`.

### UI regions, top to bottom

1. **Header** (y 84-170) — logo, QUALITY combo, ZOOM combo. Source:
   `plugin303/src/gui/panels/header_bar.rs`.
2. **ENVELOPES frame** (y 179-400, x 1460-2699) — ADV ENV checkbox, MEG ADSR
   and VEG ADSR banks. Source: `plugin303/src/gui/panels/top_controls.rs:105-187`,
   bank renderer `render_adsr_bank` at `top_controls.rs:12-59`.
3. **Oscillator / souped section** (y 400-698, divider x 541) — toggle row
   y 472-521. Source: `plugin303/src/gui/panels/souped_panel.rs`.
4. **Clipper tweaks** (y 707-889). Source: `panels/tweaks_panel.rs`.
5. **Output clipper** (y 903-1140) — 9 clipper knobs. Source:
   `panels/output_clipper.rs:85-112`.
6. **GENERATOR** (y 1149-1376, divider x 1210) — GEN SEED, MUTATION,
   GENERATE / MUTATE. Source: `panels/generator_panel.rs`.
7. **Sequencer + right column** (y 1385-1971, v-divider x 2136) — step grid
   left (playhead in `widgets/step_button.rs`), TRANSPORT & PATTERN right.
   Source: `panels/sequencer_panel.rs`.

---

## Guardrails

- **Presentation layer only.** Do not change parameter ranges, defaults,
  value formatting, sequencing behaviour, or persistence. A control that
  reads `+0.0 dB` must still read `+0.0 dB`.
- Do not add, remove, or reorder controls except where an issue explicitly
  asks (issue 7 touches the capture harness, not the UI; no UI issue asks for
  control changes).
- Do not regress earlier fixes. Invariants from previous rounds, each testable:
  - header right group stays flush right (round 10 HIGH-1);
  - ZOOM presets stay ≤ 100 % (round 10 HIGH-2);
  - TEXT_MUTED stays `#8f8fa4` ≥ 4.5:1 on panel backgrounds (round 10 HIGH-3);
  - no per-frame heap allocations in knob rendering (round 10 MEDIUM-7) —
    `knob.rs` uses a fixed 24-segment arc loop; keep it allocation-free;
  - margins around panels stay symmetric (round 8).
- Prefer fixing the shared widget or token over patching each call site,
  except where the issue names specific call sites.
- Keep `cargo fmt` and `cargo clippy` clean. Existing tests must pass
  (`cargo test -p plugin303`).

---

## HIGH

### 1. Bipolar knobs fill their value arc from minimum, contradicting the centre-detent value

**Symptom.** All captures. `plugin303/src/gui/widgets/knob.rs:139-151` draws
the active arc from `start_angle` whenever `normalized > 0.001` — no polarity
branch exists. TUNING, VOLUME, PITCH, IN, OUT, TILT sit at their centre
detent (badge reads `+0.0 ct` / `+0.0 dB`, needle at 12 o'clock,
normalized = 0.5) while the arc renders half-filled from the minimum.

**Required fix.** The active arc's origin must follow the parameter's
polarity, derived from the parameter's range (unipolar min ≥ 0 vs bipolar
min < 0), not from the current value, the default, or the label string:

- **Unipolar** (CUTOFF, RESONANCE, ENV MOD, DECAY, ACCENT, DRIVE, TONE,
  THRESH, BASS, and the rest): fill from minimum — current rendering is
  already correct, do not change it.
- **Bipolar** (TUNING, VOLUME, PITCH, IN, OUT, TILT): fill from the centre
  detent (normalized 0.5) outward toward the current value. At the detent the
  arc is zero-length (correctly absent); right of centre fills the right half,
  left of centre fills the left half.

Needle angle, tick marks, badge formatting, and the 24-segment
allocation-free loop must not change.

**Acceptance criteria.** In a capture with bipolar knobs at centre detent,
`knobs` reports `arc=NONE` (or zero angular extent) for TUNING/VOLUME/PITCH/
IN/OUT/TILT while their badges read `+0.0 …`. In a capture with one bipolar
knob moved off centre (see Verification state 4), its arc spans from 0° (12
o'clock) toward the needle with `ARC/NEEDLE` delta ≤ 4°, and unipolar knobs
still fill from minimum.

### 2. Playhead indicator is still a multi-weight composite

**Symptom.** `running.png`, strokes probe box (1855,1390)-(2115,1665) colour
#D2456A: thicknesses [1, 2, 3] px — NON-UNIFORM WEIGHT. The active step
currently stacks three marks (`widgets/step_button.rs`): a 4 px filled LED bar
with its own 1 px stroke (lines 73-97), a 1 px column rect stroke in
`ACCENT_RUBY.gamma_multiply(0.7)` (lines 64-71), and the step-number pill
switching to ruby text + ruby stroke (lines 103-118). Two different rubies
(full and gamma 0.7) plus three weights read as one uneven blob.

**Required fix.** One mark, one colour, one weight: keep a single ruby
(`ACCENT_RUBY`, no `gamma_multiply`) indicator of uniform stroke weight
identifying the current step. Pick one carrier — LED bar OR column outline OR
pill — and render the others in their non-active treatment. The `is_current
&& !is_running` treatment and all non-current steps must not change.

**Acceptance criteria.** `strokes --box` over the active step with
`--color "#D2456A" --coverage 0.001` reports a single thickness. The same
probe over a neighbouring non-current step reports no #D2456A segments.

### 3. ENVELOPES frame interior: ~265 logical px dead run between MEG and VEG banks

**Symptom.** All captures. `top_controls.rs:154-156` computes
`spacer_w = (avail - 2*115).max(6)` and places it between the banks, pushing
VEG to the frame's right edge; the middle (≈ x 1900-2170, y 179-400) is empty.

**Required fix.** Distribute the two banks across the frame without a dead
centre run — e.g. equal thirds (bank, gap, bank) or a shared grid. Frame
bounds, the ENVELOPES + ADV ENV title row, and bank contents must not change.

**Acceptance criteria.** `deadspace` on the ENVELOPES panel band reports no
DEAD COLUMNS run > 120 physical px inside the frame.

### 4. TRANSPORT & PATTERN column: dead bottom and dead right side

**Symptom.** All captures. Column x 2136-2703 inside panel y 1385-1971:
content ends at y ≈ 1660 (BPM/LEN row), leaving ≈ 150 logical px dead below;
rows stop at x ≈ 2495-2570, leaving ≈ 70-100 logical px dead on the right of
every row.

**Required fix.** Rows fill the column width (equal-width buttons/fields
spanning the column), and the column's vertical space is consumed — the
sequencer grid may grow taller or the column rows may space to fill, but no
> 60 logical px dead block may remain inside the column. Row contents, order,
and labels must not change.

**Acceptance criteria.** `deadspace` on panel y 1385-1971 reports no DEAD
ROWS run > 120 physical px in the column right of divider x 2136, and
`edges --box 2136,1390,2705,1760` row bands share a right edge within 4 px.

### 5. Souped-section toggle row: 83 % dead width

**Symptom.** All captures. Band y 472-521 (panel y 400-698): one left-aligned
toggle cluster, `deadspace` reports a 2243 px dead run.

**Required fix.** The toggle row uses its width — distribute its controls
across the band — or the band collapses to content height. Toggle labels,
order, and state colours must not change.

**Acceptance criteria.** `deadspace` reports no DEAD COLUMNS run > 240
physical px in band y 472-521.

### 6. Disabled ADSR content renders below 3:1 with a non-theme colour

**Symptom.** All captures (ADV ENV off → both banks inside
`add_enabled_ui(is_adv, …)`, `top_controls.rs:158/173`). The bank already
passes `TEXT_MUTED` as its accent (`top_controls.rs:22-26`) — then egui's
disabled fade dims everything a second time. Result: slider bodies at #54545F
2.42:1 (no theme token), titles #64646A/#6B6B71 at 3.02-3.35:1, values
#6F6F75 3.55:1. The disabling is deliberate; the double-dim is the defect.

**Required fix.** Remove the compounding: the disabled bank must render with
its explicit muted theme palette and no additional egui disabled fade (or the
fade alone, with no explicit muted colours — pick one dimming mechanism).
Every visible mark in the disabled bank — title, A/D/S/R labels, value text,
fader track and handle — must measure ≥ 3:1 against its background and must
come from `theme.rs` tokens. The disabled state must stay visibly inactive
(no accent orange, no hover response) and the enabled (ADV ENV on) rendering
must not change. Fix in `render_adsr_bank`/`MiniFader`, not per-call-site.

**Acceptance criteria.** `colors` on the ENVELOPES panel reports no
foreground below 3:1 and no non-token colour with > 200 px; text probes on
the disabled MEG/VEG titles and value rows measure ≥ 3.2:1.

### 7. Capture harness: "fun" state duplicates "default"; ACCURATE no longer captured

**Symptom.** `fun.png` is pixel-identical to `latest.png` inside the app —
`params.rs:414-426` maps both "default" and "fun" to `CircuitTopology::Fun`.
The script captures three files but two states; no state selects
`CircuitTopology::Accurate`, so ACCURATE-only findings are unverifiable.

**Required fix.** In `scripts/capture_standalone_gui.sh` and the
`VIBE303_QA_STATE` handling in `params.rs`:

- restore an `accurate` capture (`CircuitTopology::Accurate`);
- make "default" and "fun" differ visually, or drop "fun" — every captured
  file must map to a distinct state;
- keep `latest.png`, `running.png` semantics unchanged.

This is test-harness code, not plugin UI; plugin runtime behaviour when the
env var is absent must not change.

**Acceptance criteria.** A fresh capture run produces ≥ 3 PNGs; `diff` shows
each pair differs inside the app window, and the ACCURATE capture shows the
QUALITY control state that round-12 HIGH-2/HIGH-3 describe.

---

## MEDIUM

### 8. Combo box glyph height 18 px vs 12 px on adjacent buttons/segments

**Symptom.** All captures. Text probe (2555,112)-(2680,158): ZOOM "100%"
glyphs h=18; neighbouring segment captions h=12. The header combos
(`header_bar.rs:122` ZOOM, `:143` QUALITY) never received the theme font
size.

**Required fix.** Combo box button text and popup items render at the header
caption size (matching the 8.5 pt class used around them → measured 12 px
glyphs). Combo ids, options, widths, and popup behaviour must not change.

**Acceptance criteria.** Text probe over the ZOOM combo reports h ≤ 13 px;
the QUALITY combo matches.

### 9. TRANSPORT & PATTERN rows do not share a right edge

**Symptom.** All captures. Rows end at x ≈ 2495 / 2570 / 2540 / 2537 —
left-flush, ragged right.

**Required fix.** Resolved together with HIGH-4 (rows filling the column
width fixes both); if HIGH-4 is done another way, rows must share a common
right edge within 4 px. Row contents must not change.

**Acceptance criteria.** Covered by HIGH-4's `edges` acceptance; restate the
measurement in your confirmation.

### 10. GENERATOR: GEN SEED row centred while sibling rows are flush-left

**Symptom.** All captures. `edges --box 2110,1150,2710,1390`: GEN SEED band
left=2304; MUTATION and GENERATE/MUTATE bands left=2110.

**Required fix.** All rows in the GENERATOR region (`generator_panel.rs:25`
and following) share one left edge. GENERATE/MUTATE button split and widths
must not change.

**Acceptance criteria.** The edges probe reports all row bands with left
edge within 4 px of each other.

---

## LOW

11. Inter-panel gutters [9, 0, 9, 14, 9, 9] — make one gutter value rule;
    the y 179-400 → y 400-698 boundary currently has gap_above=0 and the
    output clipper has 14. Required outcome: `panels` reports a single gap
    value (±1 px) for all six gutters.
12. VEG ADSR title at 3.02:1 — resolves automatically if issue 6 lifts
    disabled text to ≥ 3.2:1. No separate change; restate the measurement.

---

## Verification

Re-render the full capture set after the changes with
`scripts/capture_standalone_gui.sh`, confirm every PNG is 2732 px wide — a
wrong window id silently produces a different width.

**States to capture:**

1. `latest.png` (default) — the baseline.
2. `running.png` — for the playhead measurement (issue 2).
3. **New:** `accurate.png` — ACCURATE topology (issue 7).
4. **New:** a state with at least one bipolar knob off its centre detent and
   one unipolar knob off its default (issue 1 is otherwise unverifiable —
   add a `VIBE303_QA_STATE` variant that sets e.g. TUNING +25 ct and CUTOFF
   75 %; harness-only change).
5. **New:** ADV ENV on — exercises the enabled ADSR rendering next to the
   disabled fix (issue 6 regression check).

**Per-issue confirmation.** Report each issue number with what you changed
and the measurement that proves it, or state explicitly that you did not fix
it and why:

- **1** — `knobs` output: bipolar knobs at detent show `arc=NONE`; the
  off-centre bipolar knob's arc/needle delta ≤ 4° from centre origin.
- **2** — `strokes` thickness list on the active step is a single value;
  control probe on a non-current step is empty.
- **3, 4, 5** — `deadspace` numbers for the three regions.
- **6** — `colors` panel output: no foreground < 3:1, no non-token colour
  > 200 px.
- **7** — `diff` between each capture pair shows distinct in-app content.
- **8** — text probe glyph heights for both combos.
- **9, 10** — `edges` right/left edge spreads ≤ 4 px.
- **11** — `panels` gutter list.

**Regression checks.** Confirm in the new captures that header right group is
flush right, ZOOM presets are ≤ 100 %, TEXT_MUTED is unchanged in `theme.rs`,
and that `cargo fmt`, `cargo clippy` and `cargo test -p plugin303` pass.
