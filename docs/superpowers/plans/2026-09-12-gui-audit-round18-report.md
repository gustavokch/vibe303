# TECHNEO 303 — GUI Visual Audit, Round 18

Date: 2026-09-12
Captures: `target/gui-qa/` (8 files), all 2732x1984.
Baseline: commit `ffc8647` "Merge pull request #63 from gustavokch/feat/gui-qa-extended-capture-states" on `main`.

## Capture set

| File | State |
|---|---|
| `latest.png` | defaults, FUN topology, QUALITY LOW |
| `accurate.png` | ACCURATE topology (SOUPED panel merged/bypassed, ANALOG bypassed) |
| `advanced.png` | ADV ENV engaged (MEG bank active) |
| `running.png` | transport running, playhead on step 04 |
| `knobs.png` | TUNING +25ct from centre, CUTOFF from min — non-default knob positions |
| `quality_high.png` | QUALITY HIGH selected |
| `generator.png` | ACID EUCLIDEAN generator panel in view |
| `zoom75.png` | 75% zoom |

All captures verified 2732 px wide. Pixel diffs confirm distinct, correctly
scoped states: `accurate` differs from `latest` in the SOUPED/banner region
and ANALOG arcs; `advanced` differs only inside the ENVELOPES box plus the
panel-2 title row; `knobs` differs only at the TUNING and CUTOFF knobs.

**Repository-state caveat.** Round-17's fixes and the PR#64 galley-key
remediation exist only on the unmerged branch `fix/gui-visual-remediation-r17`
(PR #64). `main` HEAD `ffc8647` is byte-identical to the round-17 baseline.
"Verified fixed" below therefore means *verified on the branch diff, still
reproducing on main*.

## Round 17 items — status on main

- **LOW-1 (2 px ADV-ENV panel shift) — STILL OPEN on main.** `advanced` vs
  `latest` whole-window diff = 451409 px; panel-2 title row shifts 2 px because
  `render_adsr_bank` uses `ui.label` active vs `allocate_exact_size(..., 11.0)`
  inactive (`plugin303/src/gui/panels/top_controls.rs`). Fix exists on PR#64.
- **LOW-2 (asymmetric gutters) — STILL OPEN on main.** Inter-group gutters
  measure `[10, 0, 43, 10, 10, 10, 10, 0]`. The 0 above transport is documented
  on the branch as by-design (transport is the right column of the sequencer
  Frame); the 43 remains unexplained.
- **LOW-3 (inactive NOTE cyan `#06777F` not a theme token) — STILL OPEN on
  main.** `#06777F` appears n=156 in `latest.png`, measures 3.40:1 against the
  cell background, and is absent from `plugin303/src/gui/theme.rs` on main.
  PR#64 adds `GATE_NOTE_INACTIVE_EDGE` + a `SURFACE_TOKENS` entry in the tool.
- **PR#64 review finding (galley cache key missing `title_color`) — fixed on
  branch only** (commit `54b6937`); main still keys on `(ui.id(), title)`.

Round-14 disabled-group convention check: QUALITY's grey selection border is
retained when the group is disabled in `accurate` (header accent count 936 vs
480 explained by the mode change, not a lost mark). Held.

---

## HIGH

### 1. Every structural stroke fails WCAG 1.4.11 non-text contrast (3:1)

Re-measured this round on `latest.png`, ring scan at knob centre (622,236),
radius 50: track `#222228` and tick ring `#2B2B34` both present against panel
`#1A1A1E` — ratios 1.2:1 and 1.3:1. Panel/group borders `#2B2B34` vs window
`#1A1A1E` 1.24:1; field fill `#3C3C3C` vs panel 1.57:1. All sub-threshold.

Consequence: knob range, panel edge, segment container and field boundary are
all effectively invisible; only text and the orange accent are perceivable.

**Required fix.** Raise the structural stroke token(s) to >= 3:1 against their
own backgrounds (approx `#4A4A57` on `#16161A`), knob track included. Text
tokens must not change.

### 2. Unselected segments have no boundary in every segmented control

Re-verified visually on fresh crops (`vcftaps2_latest`, header crops). Only
the selected segment gets an orange frame; unselected segments share one
undivided container in VCF TAPS (1570 px / 3 = 523 px per segment), VCA ENV,
TOPOLOGY, QUALITY, OS, WAVE. Click targets are invisible.

**Required fix.** Per-segment boundary (1 px divider or per-segment border)
on all six groups; cap segment width.

### 3. Three segmented-control patterns coexist; DIODE selected text is unique

Re-verified (`diode_latest` crop): DIODE (`SI`|`GE`) renders as two separate
bordered boxes with a gap, selected label in **orange** text — the only place
selected text turns orange. Every other group uses a shared container with
orange border + white text.

**Required fix.** One segmented-control component applied to all groups.

---

## MEDIUM

### 4. ADSR sliders: ~15 pt usable travel, handle overhangs track

Re-verified on `advanced.png` (`adsr_advanced` crop): track 40 px = 20 pt
tall, handle 10 px = 5 pt, handle capsule ~2.4x track width, letter glyphs
flush against track top (0 px gap). At 0% the handle covers the bottom cap —
reads as a plinth, not a slider.

**Required fix.** Track >= 40 pt, handle width <= track + 4 pt, >= 4 pt gap
between label and track.

### 5. TRANSPORT & PATTERN grid breaks; row-5 heights mismatch 20 pt vs 75 pt

Re-verified (`bottom_latest` crop): combos `16th` / `Direct` are ~20 pt tall
while `CLEAR` on the same row is ~75 pt (measured y 1834-1873 vs y 1780-1929
in the critical pass). Rows 2, 4, 5 split the 470 px column grid at
non-column x positions.

**Required fix.** Hold the two-column grid or declare a sub-grid; equalise
row-5 control heights.

### 6. Type scale break: `SLD CHAIN` 17 px vs 12 px siblings

Re-verified (`sldchain_latest` 3x crop): `SLD CHAIN` cap height 17 px,
`SLIDE:` / `ACC BIAS:` / `REST:` at 12 px in the same panel row group. 42%
larger, unique in the panel.

### 7. Two checkbox styles and sizes; LINK check is white

Re-verified (`bottom_latest`, `sldchain_latest`): BYPASS / ADV ENV /
SLD CHAIN = filled grey square 28 px (14 pt); JITTER = outline-only 13 px
(6.5 pt). LINK's checkmark is white; every other on-state is orange.

### 8. Read-only seed values styled inconsistently

Re-verified (`newseed_latest` + prior analog crops): `CMP SEED: 1337` boxed
field vs `SEED: 0042` bare text. Same concept, two treatments.

### 9. Asymmetric panel vertical padding

Unchanged from critical pass: top padding 4-14 pt across six panels; OUTPUT
CLIPPER and SEQUENCER carry +25 pt un-anchored bottom slack.

### 10. ANALOG CHARACTER dead bands and mixed column alignment

Unchanged: four contiguous empty bands of 238 / 168 / 170 / 109 pt; four
columns with four different vertical alignments; separators uneven
(718/385/450 px) and stopping short of panel edges.

### 11. Header row mixes 13 pt and 18 pt control heights

Unchanged: TOPOLOGY/QUALITY segments 26 px vs ZOOM combo 36 px; bottoms
align, centres off by 2.5 pt.

### 12. Knob size tiers not systematic

Unchanged: 42 pt synth core / 27 pt souped / 27.5 pt clipper / 24 pt
DRIFT-RECAL — four tiers where two differ by rounding-scale amounts.

---

## LOW

13. Playhead ring red/pink — hue outside the palette, reads as error
    (`running.png`, step 04).
14. REST steps display a dimmed pitch (`C 3` on step 12, `seqcells_latest`
    crop). A rest has no pitch.
15. `GENERATE` and `MUTATE` identical solid orange, equal weight — no
    primary/secondary distinction (`genright_latest` crop).
16. Vertical separators stop short of panel edges, ends align with nothing.
17. `BASS SYNTH` / `MONO` bordered pills look like buttons but are static;
    same false affordance on step-number cells.
18. Case/family mix: `16th` / `Direct` sentence case among uppercase labels;
    `BASS SYNTH` / `MONO` monospace among proportional sans.
19. OCT RANGE displays `MIN +0` — signed zero, cosmetic (`generator.png`).
20. Generator field labels measure `#808089` = **4.43:1** — 0.07 under WCAG
    AA 4.5:1 for normal text. Combo arrows / section heading `#B2B2BD` pass
    at 8.26:1. (Grey inventory attributed this round: sequencer `#666675`
    3.20:1 / `#6E6E7E` 3.61:1 = dimmed REST-step octave triangles, step
    numbers, inactive ACC/SLD labels — disabled state, acceptable; generator
    label grey is not a disabled state.)
21. Empty-state card padding 15 px top vs 42 px bottom (ACCURATE SOUPED
    panel).
22. Duplicate `TECHNEO 303` title: window chrome + app header 60 px below.
23. `OS` label rendered on the panel-title baseline, reads as a second
    heading.
24. Souped-row value pills off-centre from knobs by up to 3.5 px.
25. MEG-disabled state (ADV ENV off) signalled only by text/fill colour;
    group does not read inert, no reason given (SOUPED panel handles the
    analogous case with an explanatory banner).

---

## Flagged, intent unclear — not asserted as defects

- **ANALOG CHARACTER accent loss in `accurate`.** Arc pixel count 1304 in
  `latest` vs 0 in `accurate` (panel box y 882-1101). Consistent with a
  deliberate bypass of the analog stage in ACCURATE mode, and value arcs are
  not selection marks under the round-14 convention. Confirm intent in
  source; if bypass is intended, no defect.
- **SOUPED panel title grey (not orange) in `accurate`.** Consistent with
  the disabled-group convention; the `ACCURATE MODE ACTIVE` banner explains
  it. No defect if deliberate.
- **Stray `Octave down (-1)` tooltip in `zoom75.png`.** Almost certainly a
  capture artifact — the physical mouse cursor was over the octave button
  when the window was grabbed. Capture harness should park the pointer
  off-window. Not a GUI defect.
- **Grey traffic lights in `generator.png`.** Window was unfocused at
  capture time. Harness artifact, not a GUI defect.

## Tool false positives recorded (do not re-report)

- `knobs` TONE delta 10.9 deg — ring fit lands ~6 px high on this knob;
  needle/arc agree visually.
- `knobs` r=20 flags on combo arrows — not knobs.
- `zoom75` AA fringing flagged as text edges — 75% scaling artifact.
- `accent` cross-state table aligns **by panel index**; `accurate` has 8
  panels vs 9 elsewhere, so rows misalign and print false
  "ACCENT LOST IN ONE STATE" flags (OUTPUT CLIPPER row actually compares two
  different panels). Box probes confirmed OUTPUT CLIPPER 1600 vs 1603 — fine.
- On main, `report`'s file filter `"_" not in f` silently excludes
  `quality_high.png` (and any underscore-named state) from the report.
  Fixed in the PR#64 tool version; workaround is running
  panels/deadspace/colors on the file directly.
- Sub-1% AA jitter between captures produced an 11.5 k px "diff" in the
  sequencer region `latest` vs `accurate`; side-by-side crops are pixel
  identical. Treat small diffs outside known dynamic regions as noise until
  cropped.

## Capture gaps for the next round

- No hover or keyboard-focus state for any control.
- QUALITY ULTRA and ULTRA 2X never captured (only LOW and HIGH).
- No WAVE swap (SAW/SQR/SWP) state.
- No combined ADV-ENV-on + running state.
- Tooltip/pointer artifacts: harness does not move the physical mouse
  off-window before capture.

## Most urgent

1. **#1 structural strokes** — the whole boundary layer is invisible.
2. **#2 segment boundaries** — six controls have invisible hit regions.
3. **#3 segmented-control unification** — one component, one selected style.
4. **Merge PR#64** — round-17 fixes (LOW-1/2/3 + galley key) are done but
   unmerged; every further round re-measures them on main.
5. **#4 ADSR travel** — 15 pt for a 3 ms..2 s range is a usability defect.
6. **#5 transport grid / row-5 heights** — most visible layout break.
