# TECHNEO 303 — GUI Visual Audit, Round 12

Date: 2026-09-02
Captures: `target/gui-qa/` (`latest.png`, `accurate.png`, `advanced.png`, `running.png`), all 2732x1984.
Baseline: commit `3605609` "fix(gui): remediate round-11 audit defects (items 2-25)".

## Capture set

| File | State |
|---|---|
| `latest.png` | FUN topology, LOW quality, ADV ENV off, transport stopped |
| `accurate.png` | ACCURATE topology (new this round — closes the round-11 harness gap) |
| `advanced.png` | FUN topology, ADV ENV on |
| `running.png` | FUN topology, transport running, playhead on step 04 |

Pixel diffs confirm the states are distinct and that the state changes are correctly scoped:
`advanced` differs from `latest` only inside the ENVELOPES box (y 206-362, x 1474-1744); `running`
differs only inside the step grid and transport column (y 1375-1638, x 809-2406).

## Round-11 items verified fixed

- **Knob needle vs arc mismatch.** Measured on all 27 knobs. Where an arc is drawn, its end angle
  and the needle agree within 3°: GLIDE arc ends -26.6°, needle -24°; DRIFT arc ends -108.4°,
  needle -110°; RECAL arc full sweep, needle at the +135° limit. Sweep range is -135°..+135°.
- **ENVELOPES overflowing its parent panel.** The box now sits inside the oscillator panel
  (x 1461-1962, panel 20-2711).
- **Broken playhead frame.** A playhead indicator now renders — but see HIGH-4 for how.
- **TEXT_MUTED contrast.** Inactive sequencer content (`#8F8FA4`) measures 5.70:1, passes AA.

## Layout map

1. Window title bar (macOS chrome, title "TECHNEO 303")
2. Header bar — y 84-171 — app title, BASS SYNTH / MONO tags, TOPOLOGY, QUALITY, ZOOM
3. Oscillator panel — y 179-380 — WAVE selector, 7 large knobs, ENVELOPES box
4. Souped-up engine & analog drawer — y ~381-679 — VCF TAPS / VCA ENV toggles, 9 knobs
5. Output clipper — y 687-870 — BYPASS/OS group, 9 knobs, LINK/DIODE group
6. Analog character & tweaks — y 883-1109 — TOL/AGE/NOISE, DRIFT, CMP SEED/RECAL, CALIBRATE
7. Acid Euclidean generator — y 1117-1357 — 4 numbered columns, GEN SEED / NEW SEED
8. Sequencer — y 1365-1968 — 16 step columns (left, x 38-2085) + TRANSPORT & PATTERN (right, x 2110-2690)

---

## HIGH

### 1. Unipolar knobs whose default is 50% are drawn as bipolar

`plugin303/src/gui/widgets/knob.rs` infers polarity from the default value
(`is_bipolar = (default - 0.5).abs() < 0.001`) rather than from the parameter's actual polarity.
Every unipolar 0-100 % control that happens to default to 50 % is therefore filled outward from the
12 o'clock detent instead of from the minimum.

Visible symptom in `latest.png`: only 3 of 27 knobs draw a value arc at all — GLIDE, DRIFT and RECAL.
RESONANCE, ENV MOD, ACCENT, PW and the clipper DRIVE / THRESH / BASS / TONE knobs all sit at 50 %,
so their "bipolar" arc has zero length and no arc appears. CUTOFF (2.1 kHz) and DECAY (1.1 s) are
likewise misclassified; they will fill leftward from 12 o'clock once moved below their default.

**Required fix:** derive bipolarity from the parameter descriptor (an explicit flag or a signed
range), not from the default value. A 0-100 % knob must fill from the minimum regardless of where
its default sits. TUNING, PITCH1, PITCH2, VOLUME, IN, OUT and TILT are genuinely bipolar and must
keep filling from the centre detent.

### 2. ACCURATE mode hides which QUALITY tier is selected

`accurate.png` contains **zero** accent pixels in the QUALITY group (`latest.png` has 380). Disabling
the group via `add_enabled_ui(false)` (`plugin303/src/gui/panels/header_bar.rs:144`) also strips the
selected segment's orange stroke, so the active tier becomes unreadable. Disabling the group is
intentional (the tooltip explains quality applies to FUN only); losing the selection indicator is not.

**Required fix:** a disabled segmented control must still mark its selected segment — e.g. a dimmed
accent stroke — so the current value stays legible.

### 3. Disabled text drops to 2.3:1 contrast

Measured foreground / background in `accurate.png`:

| Element | Colour | Contrast |
|---|---|---|
| QUALITY options, disabled | `#53535E` on `#1A1A1E` | **2.29:1** |
| ANALOG CHARACTER & TWEAKS heading, disabled | `#54545F` | **2.32:1** |

The whole ANALOG panel — heading, labels, fields, both knobs — renders at this level in ACCURATE
mode. Compare the ENVELOPES disabled state in `latest.png`, which uses `#6F6F75` at 3.55:1, and
inactive sequencer content at `#8F8FA4` / 5.70:1.

**Required fix:** one disabled token, at or above 3:1 against panel background. See MEDIUM-10.

### 4. Playhead renders as three unaligned white elements

In `running.png`, step 04 is marked by:

- a **solid 10 px white bar** spanning the full column width at y 1375-1384, above the step number;
- a **2 px white rule** at y 1413-1414, directly under the step-number cell;
- a **2 px outline** at x 809-810 / 1056-1057 with its bottom edge at y 1637-1638.

The bar and the outline's top corner merge into one thick, uneven top edge. The outline is also
drawn inside the step's horizontal padding, so the ACC / SLD / REST / note / arrow buttons sit flush
against it — every other step keeps a visible gap between its buttons and the column edge.

**Required fix:** one indicator of uniform stroke width around the step column, drawn in the
inter-column gutter so the step's own padding and button borders are preserved.

### 5. Souped-up engine toggle row: 83 % of its width is empty

In `latest.png` the VCF TAPS / VCA ENV row has content only from x 46 to x 459; the remaining
2240 px to the panel's right edge is empty.

**Required fix:** either spread the toggle groups across the row, or move content up into the space,
or narrow the group container so the emptiness reads as deliberate.

### 6. Oscillator panel: 736 px of dead width right of ENVELOPES

The ENVELOPES box ends at x 1963 while the panel interior runs to x 2708 — 27 % of the window width
is unused, with no divider or content.

### 7. TRANSPORT & PATTERN column: 265 px of dead height

The last transport row (CLEAR) ends at y 1702; the panel's bottom border is at y 1967. That is 44 %
of the column's height empty, and the column stops 209 px above where the step grid to its left ends.

---

## MEDIUM

### 8. ComboBox text renders ~50 % larger than every other control

Measured glyph heights in `latest.png`:

| Control | Height |
|---|---|
| ZOOM `100%`, ROOT `C`, `Pentatonic`, TOL `0%`, `16th`, `Direct` (combos) | 18-19 px |
| `LOW`, `FUN`, `CLEAR` (buttons / segments) | 11-12 px |
| `05`, `16` (numeric fields) | 13 px |
| `ROOT:`, `SCALE:` (labels) | 10-11 px |

The combo boxes appear never to have received the theme's font size.

### 9. `SLD CHAIN` checkbox label is 18 px

Every sibling label in the same generator panel is 10-13 px; LINK and ADV ENV labels are 12 px.

### 10. Three different muted / disabled greys

`#8F8FA4` (5.70:1, inactive sequencer content), `#6F6F75` (3.55:1, disabled ENVELOPES group),
`#53535E` (2.29:1, ACCURATE-mode disabled). Collapse to one muted and one disabled token.

### 11. Checked-checkbox label styling is inconsistent

In `advanced.png` the `ADV ENV` label turns orange when checked. `LINK` in `latest.png` is checked
and stays white. `BYPASS`, `SLD CHAIN` and `JITTER` are unchecked and white.

### 12. DIODE is the only segmented control drawn as separate buttons

TOPOLOGY, QUALITY, VCF TAPS, VCA ENV and OS all use a single rounded container with inner segments.
DIODE renders `SI` and `GE` as two detached rounded buttons.

### 13. Selected-segment text colour is inconsistent

`4-POLE`, `FUN`, `LOW`, `ENV` and `2X` render white inside their orange stroke. `SI` renders orange.

### 14. Transport rows are not right-aligned

Right edges: `SYNC: HOST` 2690, `> SHIFT` 2690, but `SYNTH: ON` 2674 and `CLEAR` 2676 — 14-16 px short.

### 15. The ACCURATE placeholder panel has no heading

In FUN the panel is titled "SOUPED-UP ENGINE & ANALOG DRAWER"; in ACCURATE the panel renders headless
while every other panel keeps its orange title.

### 16. Panel title sizes differ

"ACID EUCLIDEAN GENERATOR" is 16 px; "SOUPED-UP ENGINE & ANALOG DRAWER", "OUTPUT CLIPPER" and
"ANALOG CHARACTER & TWEAKS" are 14 px. (Sub-panel titles ENVELOPES / TRANSPORT & PATTERN are 13 px,
which reads as a deliberate second level.)

### 17. ANALOG panel column contents are not centred in their columns

The panel divides into four divider-separated columns but DRIFT sits ~70 px left of its column
centre and RECAL likewise. Overall panel fill is 4.1 %.

### 18. Generator field left edges are misaligned

ROOT's combo starts at x 188 while the PULSES and SHIFT fields below it start at x 212 (24 px off);
the same 25 px offset appears between the SCALE combo and the STEPS / PITCH LEN fields.

---

## LOW

19. Inter-panel gap is 8 px everywhere except clipper to analog, which is 13 px.
20. Header has a 1416 px empty span between the MONO tag (x 468) and TOPOLOGY (x 1883).
21. Header right inner margin is 40 px against a 20 px left margin.
22. The ZOOM label is centred ~10 px right of its combo box.
23. Step grid insets: 18 px left, 23 px right.
24. Sequencer vertical padding: 19 px top, 56 px bottom.
25. "TECHNEO 303" appears twice — window title bar and header bar.
26. Generator columns have no vertical dividers; ANALOG and OUTPUT CLIPPER do.

---

## Flagged, intent unclear — not asserted as defects

- QUALITY disabled in ACCURATE is deliberate (`header_bar.rs:144`, with a tooltip). Only the lost
  selection indicator (HIGH-2) is a defect.
- The ANALOG panel being disabled in ACCURATE is plausible; combined with HIGH-3 it is unreadable.
- The ACCURATE placeholder keeps the FUN panel's height, which avoids a layout jump. The cost is a
  ~1950x310 region holding a ~500x170 card.
- Two knob radii — 40 px in the oscillator row, 24 px elsewhere — read as deliberate hierarchy.
- The playhead's white accent, against an otherwise orange / cyan / yellow palette, may be deliberate.

## Capture gaps for the next round

- No ACCURATE x ADV ENV combination.
- No capture with a quality tier other than LOW, and none above 100 % zoom.
- No capture with any knob away from its default, which is what would expose HIGH-1 directly.
- No hover or focus state for any control.

## Most urgent

HIGH-1 (arc polarity — a value/position contradiction on ten-plus knobs), HIGH-2 (selected quality
tier invisible), HIGH-3 (2.3:1 disabled text), HIGH-4 (playhead render). Then the three dead-space
items, HIGH-5 through HIGH-7.
