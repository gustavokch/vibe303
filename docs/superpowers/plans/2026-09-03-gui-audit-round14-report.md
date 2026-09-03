# TECHNEO 303 — GUI Visual Audit, Round 14

Date: 2026-09-03
Captures: `target/gui-qa/` (`latest.png`, `running.png`, `advanced.png`,
`accurate.png`, `knobs.png`), all 2732x1984, fresh.
Baseline: commit `4f74339` "Merge pull request #61 from
gustavokch/fix/gui-visual-remediation-r13" — all 12 round-13 issues claimed
fixed there.

## Capture set

| File | State |
|---|---|
| `latest.png` | default (Fun topology, ADV ENV off, seq stopped) |
| `running.png` | seq_run=true, playhead on step 04 |
| `advanced.png` | ADV ENV on (MEG bank enabled) |
| `accurate.png` | CircuitTopology::Accurate |
| `knobs.png` | tuning=25.0, cutoff=0.75 (off-default knob values) |

Pixel diffs confirm five distinct states. `running.png` differs from
`latest.png` only inside the sequencer grid (y 1366-1629, x 809-2406,
n=34622): playhead on step 04, STOP lit orange.

## Round-13 items verified fixed

- **HIGH-1 (bipolar arc polarity).** `knobs.png`: TUNING needle 37.0°,
  fraction 0.637 (= 25/200 + 0.5), arc `from=centre` to +34.7°, delta 2.3°;
  CUTOFF (unipolar 0.75) arc `from=min`, delta 2.3°. Both under the 4°
  threshold, polarity now follows the parameter.
- **HIGH-2 (playhead composite).** Playhead is now one mark: colour #FF3B56,
  strokes probe on the step-04 column box (805,1390)-(1075,1665) reports
  verticals x 809/1056 and horizontal y 1628, all thickness 2 px,
  `uniform weight: 2px`. Control probe on the step-05 column (same colour,
  coverage 0.1) returns empty — no leakage to non-current steps.
- **HIGH-3 (dead run between MEG and VEG).** The envelope module now hugs its
  content (`top_controls.rs:151-155`); the interior run x 1900-2170 is gone.
  The dead space moved instead — see HIGH-1 below.
- **HIGH-4 / MEDIUM-9 (TRANSPORT & PATTERN dead bottom, ragged right).** The
  right column is restructured: the sequencer panel is now y 1356-1794 and
  transport/pattern content sits in a full-width bottom panel y 1794-1958
  whose rows reach right=2690. The dead column bottom and ragged right edge
  are gone.
- **HIGH-5 (souped toggle row 83% dead).** The toggle band y 384-470 now
  distributes VCF TAPS (2-POLE/3-POLE/4-POLE) and VCA ENV (GATE/ENV)
  segments across the full panel width.
- **HIGH-6 (disabled ADSR 2.42:1, non-theme fade).** Latest state (ADV ENV
  off, MEG disabled): colours probe on panel y 180-384 shows only
  TEXT_MUTED #8F8FA4 at 5.48:1 and accents; #54545F is gone, no `BELOW 3:1`
  flags anywhere in the frame.
- **HIGH-7 (capture harness).** Five distinct states including fresh
  `accurate.png`, `advanced.png`, and an off-default-knob state.
- **MEDIUM-8 (zoom combo glyph size).** Header ZOOM combo now measures
  h=12 (was 18), matching its class.
- **MEDIUM-10 (GEN SEED row centring).** The GENERATOR panel is redesigned
  into four full-width columns (PATTERN STRUCTURE / OCTAVE BEHAVIOR /
  PROBABILITIES / GENERATION); the centred GEN SEED row no longer exists.
- **MEDIUM-14 class (row right edges).** Covered by the HIGH-4 restructure.

## Layout map

1. Header — y 84-170 — brand block left; ZOOM, QUALITY, TOPOLOGY right.
   Source: `plugin303/src/gui/panels/header_bar.rs`.
2. Top controls — y 180-384, v-dividers x 1460/1962 — WAVE switch, 7 knobs,
   ENVELOPES module. Source: `plugin303/src/gui/panels/top_controls.rs`.
3. Souped toggles — y 384-470 — VCF TAPS, VCA ENV segments.
4. Souped knobs — y 513-680.
5. Clipper tweaks — y 690-872.
6. Output clipper — y 882-1101.
7. ACID EUCLIDEAN GENERATOR — y 1111-1346, h-divider x 1166.
8. Sequencer — y 1356-1794, v-divider x 2108.
9. Bottom row — y 1794-1958 — pattern/transport controls, 16th/Direct
   combos, JITTER, CLEAR.

---

## HIGH

### 1. Dead space right of the ENVELOPES module: 739 px run, 100% dead column

**Symptom.** `latest.png` and `advanced.png`. Deadspace probe on panel
y 180-384: `DEAD COLUMNS x 1964..2703, 739 px (27.6% of width), band
y 192-375`; the column probe reports `DEAD ROWS y 188..376, 100.0% of
height`. The round-13 HIGH-3 fix made the envelope frame hug its content
(`top_controls.rs:151-155`) but left the top-controls panel full width, so
the dead run moved from between the ADSR banks to the right of the whole
module. ADV ENV on (`advanced.png`) does not expand the banks into it.

**Required fix.** The region right of the ENVELOPES module must carry
content or the panel must shrink to its content — the dead run may not
remain. Module contents (title row, MEG/VEG banks, ADV ENV checkbox) and
the knob row must not change.

### 2. QUALITY selector loses its selection indicator when disabled in ACCURATE

**Symptom.** `accurate.png`. Accent probe on the header: 480 accent pixels
vs 3346 in `latest.png`. Crop of the QUALITY switch shows all four segments
(LOW/HIGH/ULTRA/ULTRA 2X) uniform grey — no thumb, no accent stroke, so the
current tier is unreadable. Root cause in `widgets/switch.rs`: the disabled
path paints the thumb fill as `WELL_BG` (identical to the track fill, lines
334-341) and demotes the accent stroke to `BORDER_SUBTLE` (343-355) and all
option text to `TEXT_MUTED` (361-367). The disabling itself is intentional
(`header_bar.rs:158-172`, with an explanatory hover text); losing the
selected-value indicator is not. Round-12 HIGH-2 class, still open.

**Required fix.** The disabled QUALITY switch must still show which tier is
selected (a visible thumb or marker at ≥3:1 against the track) while
remaining visibly inactive — no accent, no hover, no interaction. The
enabled rendering and the TOPOLOGY switch must not change.

### 3. Disabled ANALOG CHARACTER & TWEAKS panel text at 2.32:1 in ACCURATE

**Symptom.** `accurate.png` only. Colours probe: #54545F at 2.32:1, n=704
pixels, located in the bypassed ANALOG CHARACTER and TWEAKS panels. Below
the 3:1 hard floor — a defect even for disabled content. Round-12 HIGH-3
class, still open. (The same-class round-13 ADSR case is fixed; this group
uses a different dim path.)

**Required fix.** Disabled panel text in the ACCURATE bypass state must
measure ≥3:1 against its background, using theme tokens only. The enabled
(FUN) rendering must not change.

### 4. ACCURATE placeholder replaces the SOUPED-UP ENGINE & ANALOG DRAWER panel title and framing

**Symptom.** `accurate.png`. Where `latest.png` shows the framed
"SOUPED-UP ENGINE & ANALOG DRAWER" section, `accurate.png` shows a bare
placeholder ("ACCURATE MODE ACTIVE / Souped-up engine is bypassed / ENABLE
FUN MODE") — the panel title and frame disappear, so the section's position
and extent are no longer identifiable. Checklist item D (state change
removes structure, not just content).

**Required fix.** The bypassed state must keep the panel frame and title
visible, with the placeholder content inside it. Placeholder wording may
not change.

---

## MEDIUM

### 5. Bottom-row combos render at glyph h=19 next to h=11 button captions

**Symptom.** All captures. Text probe on the bottom panel: the "16th" and
"Direct" combo glyphs measure h=19 (#A0A0A0 on #3C3C3C, 4.22:1); the CLEAR
button caption in the same row measures h=11. Same class as round-13
MEDIUM-8 — the header ZOOM combo received the theme font size (now h=12,
verified above); the bottom-row combos did not. The CLEAR button also
renders as a tall box roughly twice the combos' height, which exaggerates
the mismatch.

**Required fix.** Combo text renders at the same size as the button
captions of its class, and the row's controls share one height. Combo
popup contents and behaviour must not change.

### 6. DRIFT knob arc/needle delta 4.7°

**Symptom.** `latest.png`. Knobs probe: DRIFT at (1152,1002), value 10%,
needle -114.1° vs arc end -109.4°, delta 4.7° — just over the 4° mismatch
threshold. Borderline: may be a ring-fit wobble rather than a real
disagreement (see the TONE false positive below), but it is the only knob
above threshold this round.

**Required fix.** Arc and needle must agree within 4° at every value, or
the over-threshold reading must be traced to a measurement artefact and
recorded.

---

## LOW

7. Inter-panel gutters still inconsistent: `[10, 0, 43, 10, 10, 10, 10, 0]` —
   gap_above=0 above the souped-toggle band and above the bottom row, 43
   above the souped-knob band. Round-13 LOW-11 persists with new values.
8. Header ZOOM combo text #999999 at 3.87:1 on #3C3C3C — below the 4.5:1
   AA floor for an interactive control's current value (size was fixed;
   colour was not).
9. #B4B4B4 appears in three panels (y 882-1101, y 1111-1346, y 1794-1958)
   at 8.37:1, `NOT A THEME TOKEN` — recurring hard-coded grey, likely
   button/combo captions. Contrast is fine; the palette drift is not.
10. #06777F (dim cyan, inactive NOTE cells) at 3.40:1 in the sequencer,
    `NOT A THEME TOKEN` — a second, darker cyan outside the token set.

---

## Tool false positives — recorded so round 15 does not re-investigate

- `knobs` again flagged TONE at (1197,738), delta=10.9°. This is the
  round-13 false positive recurring: the ring fit places the centre ~6 px
  high; fitting from the dim-ring extremes gives arc -120.7°..-1.1° vs
  needle +2.0° (~3° agreement), and `knob.rs` drives arc and needle from
  one `normalized`, so they cannot disagree. **False positive.**
- `knobs` r=20 detections at (914,1220), (1025,1222), (2213,1854),
  (2262,1854) are the generator and bottom-row combo dropdown arrows — the
  known r=20 arrow class. **False positives.**

## Flagged, intent unclear — not asserted as defects

- GENERATOR title row: `DEAD COLUMNS x 340..2594 in band y 1121-1156` is
  the panel title row — "ACID EUCLIDEAN GENERATOR" left, NEW SEED button
  right, nothing between. Title-row pattern, expected.
- The CLEAR button is a tall box spanning both bottom rows while its
  siblings are single-row height. Reads as deliberate emphasis; if it is
  not, it is part of issue 5's row-height mismatch.

## Capture gaps for round 15

- **No hover/focus/pressed state** for any control class (still).
- **No zoom ≠ 100% state** — the ZOOM selector now offers 75%/100%; a 75%
  capture would exercise the round-10 clipping class.
- **No state with QUALITY at a non-default tier** — the disabled-indicator
  finding (HIGH-2) is verified against the default tier only.
- **No state with the generator mutated** — PROBABILITIES values never move
  off their defaults across the set.

## Most urgent

1. **HIGH-2** — QUALITY selection invisible in ACCURATE; a state the user
   cannot read, open since round 12.
2. **HIGH-3** — 2.32:1 disabled text in ACCURATE; under the hard 3:1 floor,
   open since round 12.
3. **HIGH-1** — the ENVELOPES-right dead run; the round-13 fix relocated
   the space instead of absorbing it.
4. **HIGH-4** — ACCURATE placeholder drops the panel frame; small, isolated
   layout fix.
5. **MEDIUM-5** — bottom-row combo font/height; same class as the zoom
   combo already fixed once.
