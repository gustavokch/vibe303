# TECHNEO 303 GUI — Visual Audit Fix Prompt

Source audit: visual inspection of `target/gui-qa/latest.png` (FUN topology, transport
stopped), `target/gui-qa/accurate.png` (ACCURATE topology, transport stopped), and
`target/gui-qa/running.png` (FUN topology, transport playing, playhead on step 04).
All three are 2560x1504 device pixels = 1280x752 logical points at 2x. Every coordinate
in this document is in device pixels of those screenshots unless stated otherwise.

Issue numbers are stable and match the audit. Reference them in your confirmation list.

## Context

The app is the standalone build of the `plugin303` TB-303-style synthesizer, rendered
with `nih_plug_egui`. The window is fixed at 1280x752 logical points.

### UI regions, top to bottom

1. **Title bar** — macOS traffic lights, "TECHNEO 303".
2. **Header bar** — "TECHNEO 303" wordmark, `BASS SYNTH` tag, `MONO` tag, `TOPOLOGY`
   segmented control (ACCURATE / FUN), `QUALITY` segmented control (LOW / HIGH / ULTRA),
   `ZOOM` dropdown.
3. **Core synth panel** — `WAVE` selector (SAW / SQR / SWP) currently rendered to the
   left of the panel, seven large knobs (TUNING, CUTOFF, RESONANCE, ENV MOD, DECAY,
   ACCENT, VOLUME), a vertical divider, and the `ENVELOPES` sub-panel (`ADV ENV`
   checkbox, `MEG ADSR` and `VEG ADSR` fader groups).
4. **SOUPED-UP ENGINE & ANALOG DRAWER** — `VCF TAPS` and `VCA ENV` segmented controls,
   then nine small knobs (DRIVE, SUB, GLIDE, PITCH1, PITCH2, MIX2, PW, SWEEP, R-COMP).
   Bypassed and overlaid in ACCURATE topology.
5. **OUTPUT CLIPPER** (left) and **ANALOG CHARACTER & TWEAKS** (right) — a side-by-side
   panel row. Clipper holds `BYPASS`, `OS` segmented control, ten knobs, `LINK` checkbox
   and `DIODE` segmented control. Tweaks holds `TOL`, `AGE`, `NOISE`, `COMP SEED` fields
   plus `DRIFT` and `RECAL` knobs.
6. **ACID EUCLIDEAN GENERATOR** — `GEN SEED` readout, `NEW SEED` button, and four
   numbered subsections: 1. PATTERN STRUCTURE, 2. OCTAVE BEHAVIOR, 3. PROBABILITIES,
   4. GENERATION (`GENERATE`, `MUTATE`).
7. **Sequencer** — sixteen step columns (step number, ACC/SLD pair, NOTE/TIE/REST,
   pitch, up/down arrows) plus the `TRANSPORT & PATTERN` sub-panel (RUN/STOP,
   SYNC: HOST, `< SHIFT` / `> SHIFT`, BPM, LEN, two unlabeled dropdowns, JIT, CLEAR).

### Likely source locations

- `plugin303/src/gui/mod.rs` — top-level layout and panel ordering.
- `plugin303/src/gui/theme.rs` — colors, accents, spacing constants.
- `plugin303/src/gui/panels/{header_bar,top_controls,souped_panel,output_clipper,tweaks_panel,generator_panel,sequencer_panel}.rs`
- `plugin303/src/gui/widgets/{knob,fader,switch,step_button,led}.rs`

---

## Issue 1 — ACCURATE topology leaves a 157 px dead band at the window bottom

**Observed:** In `latest.png` and `running.png` the last content row is y=1454, leaving a
49 px bottom margin. In `accurate.png` the last content row is y=1346, leaving 157 px.
The souped-up panel collapses from 233 px tall to about 155 px because its knob row is
removed, everything below shifts up by ~78 px, and the reclaimed space accumulates as
empty background at the bottom of the window.

**Required fix:** The window must be visually filled in both topologies. The bottom
margin below the last panel must be the same in ACCURATE as in FUN, within 4 px. Achieve
this by keeping the souped-up panel's outer height constant across topologies (reserve
the knob-row height while the overlay is shown) rather than by resizing the panels below
it. No panel below the souped-up panel may change its y position, height, or internal
layout between the two topologies.

## Issue 2 — OUTPUT CLIPPER and ANALOG CHARACTER & TWEAKS are misaligned

**Observed:** Measured panel borders in `latest.png`: clipper spans y 780–940 (160 px
tall), tweaks spans y 792–1025 (233 px tall). The two panels in the same row start 12 px
apart vertically and end 85 px apart. The gap under the clipper reads as a hole between
that panel and the generator panel below.

**Required fix:** Both panels in this row must share the same top y and the same bottom y
(equal height), within 2 px. The vertical gap between the bottom of this row and the top
of the ACID EUCLIDEAN GENERATOR panel must match the gap used between the other panel
rows, within 2 px. Verify in all three states.

## Issue 3 — Core synth panel is inset; the WAVE selector is orphaned outside it

**Observed:** At y=190 the core synth panel's left border is at x=140, while every other
panel border sits at x=20. The `WAVE` label, its selector capsule, and the SAW/SQR/SWP
labels occupy the x 20–140 gutter, outside the panel they belong to.

**Required fix:** The core synth panel's left border must align with all other panels at
x=20 (10 logical points), and the `WAVE` selector must sit inside that panel with the
same inner padding as the panel's other contents. No control may live in the gutter
between the window edge and a panel border.

## Issue 4 — ANALOG CHARACTER & TWEAKS panel has an empty left half

**Observed:** The `TOL` / `AGE` / `NOISE` / `COMP SEED` field row runs across the top of
the panel. Below it is a ~90 px void, then `DRIFT` at x≈1720 and `RECAL` at x≈2140 —
both right of the panel's horizontal center. The entire left half below the field row is
empty.

**Required fix:** Distribute the panel's contents so no region larger than one control's
footprint is left empty. Either lay `DRIFT` and `RECAL` out on the same baseline as a
proper knob row with the same left inset as the field row above, or place them inline in
the field row. Result must have no void wider than 200 px and no void taller than 40 px
inside the panel.

## Issue 5 — SOUPED-UP ENGINE toggle row leaves ~1500 px empty

**Observed:** `VCF TAPS` and `VCA ENV` occupy x 20–460. The remaining width of the row is
empty background.

**Required fix:** Either widen the toggle row's content (spread the two segmented
controls across the available width using the same column pitch as the knob row below),
or move something into the row so no single row of a panel is more than 60% empty. Do not
introduce new controls.

## Issue 6 — Core synth panel has ~980 px of empty width right of ENVELOPES

**Observed:** The `ENVELOPES` sub-panel's right edge is at x≈1558; the core synth panel's
right border is at x≈2540. The horizontal rule at y≈395 runs through this void.

**Required fix:** Either stretch the `ENVELOPES` sub-panel to the panel's right inner
edge (matching the right margin used on the left side of the panel), or reflow the seven
main knobs and the sub-panel to consume the width. Decorative rules must not extend into
empty regions — every rule must terminate at content.

## Issue 7 — `%` readouts disagree with knob arc position, inconsistently

**Observed:** Knobs share a `%` unit but four different value-to-arc mappings:

- `RECAL 100%` — arc full, pointer at the maximum (4:30).
- `TONE 100%` — arc half, pointer at 12:00.
- `OUTPUT CLIPPER DRIVE 32%` — arc at ~65% of sweep.
- `DRIFT 10%` — arc at ~45% of sweep.
- souped-up `DRIVE / SUB / MIX2 / SWEEP / R-COMP 0%` — arc at the minimum (correct).

**Required fix:** Pick one convention and apply it to every knob whose readout ends in
`%`: the readout must be the same normalized quantity the arc draws. A knob reading `N%`
must place its pointer at N% of its visual sweep, for every `%`-suffixed knob, verified at
0%, 50%, and 100%. Knobs whose parameter range is not 0–100% must not display a bare `%`
readout — either normalize the readout to the parameter's own range or show the real unit.
Non-percentage knobs (`+0.0 dB`, `200 ms`, `+0.0 st`, `2.1 kHz`, `1.10 s`, `+0 ct`) keep
their current behavior of drawing fraction-of-range.

## Issue 8 — PULSES (k) disagrees with the number of onsets in the grid

**Observed:** `PULSES (k): 5`, `STEPS (n): 16`. The rendered grid holds 8 `NOTE` cells,
4 `TIE`, 4 `REST`. A Euclidean pattern with k=5 should produce 5 onsets.

**Required fix:** Determine whether this is a display bug or a generator bug. If the
generator is correct and `NOTE` includes ties or slide continuations, the readout must be
disambiguated so it cannot be misread (label the field with what it actually controls, or
show the resulting onset count). If the generator does not honor `k`, fix it. Do not
change the meaning of `k` silently.

## Issue 9 — Generated pattern is exactly period-4

**Observed:** Grid repeats `NOTE / TIE / NOTE / REST` four times. `ACC` lands on steps
1, 5, 9, 13. `SLD` lands on steps 3, 7, 11, 15. Settings at capture time were
`REST 20%`, `SLIDE 25%`, `ACCENT 35%`, `ACC BIAS 50%`, `MUTATION 15%`,
`GEN SEED 000000000000002A`. A seeded probabilistic generator producing a perfectly
periodic result for all four attributes at once is suspicious.

**Required fix:** Verify the generator consumes the seed and the probability parameters.
If the periodicity is a genuine property of the Euclidean algorithm at these settings,
leave the behavior alone and note it. If accents, slides, or rests are being placed by a
fixed stride rather than by the seeded RNG, fix the placement. This issue is flagged, not
asserted — report your finding either way.

## Issue 10 — Three undisambiguated "16" fields

**Observed:** `STEPS (n): 16` in the generator, `PITCH LEN: 16` in the generator, and
`LEN: 16` in the transport panel. A user cannot tell which governs playback length.

**Required fix:** Label the three fields so their scope is unambiguous without a tooltip.
Presentation only — do not change which parameter each field is bound to.

## Issue 11 — ADSR fader fill detaches from the handle at low values

**Observed:** In `MEG ADSR` and `VEG ADSR`, the `A` fader (`3 ms`) draws its handle bar
at y 405–406 and its orange fill at y 411–415, with a 4 px dark gap between them — the
fill reads as a detached dot. The `S` fader (`0%`) draws a flat bar with no fill column
at all, styled unlike `A` / `D` / `R`.

**Required fix:** The fill must be contiguous with the handle at every value, including
the minimum. All four faders in a group must use identical handle and fill geometry; a
zero value renders a zero-height fill flush to the handle, not a different widget style.
Verify at 0%, at a low non-zero value, and at maximum.

## Issue 12 — Disabled QUALITY group loses its selection indicator

**Observed:** In `accurate.png` the `QUALITY` group is dimmed and `LOW` drops from
orange-selected to a plain grey outline, visually indistinguishable from `HIGH` and
`ULTRA`. The `QUALITY` label's contrast against the panel is near-illegible.

**Required fix:** A disabled segmented control must still show which option is selected —
dim the accent rather than removing it, keeping the selected option clearly distinct from
the unselected ones. Disabled label text must remain legible; use a dimmed foreground that
still reaches at least 3:1 contrast against its background.

## Issue 13 — Dimmed section title is unreadable

**Observed:** In `accurate.png` the title "SOUPED-UP ENGINE & ANALOG DRAWER" is dimmed to
barely above the panel background.

**Required fix:** Dimmed section titles must stay readable at the same minimum contrast
used for other disabled text (at least 3:1 against their background). Apply the same rule
to the dimmed `ANALOG CHARACTER & TWEAKS` title.

## Issue 14 — Non-uniform dimming inside the bypassed souped-up panel

**Observed:** In `accurate.png` the souped-up knob row is removed entirely, while the
`VCF TAPS` and `VCA ENV` toggles ghost through, and the selected `4-POLE` and `ENV`
options retain visibly colored outlines while everything around them is grey.

**Required fix:** The bypassed panel must dim uniformly. Every element in it — titles,
labels, toggle frames, selected-option accents, knobs — takes the same dim treatment, and
the same set of elements is present as in the active state (see Issue 1: keep the knob
row rendered and dimmed rather than removing it). No element may ghost through brighter
than its neighbors.

## Issue 15 — TWEAKS panel dims with no overlay while its neighbor stays active

**Observed:** In `accurate.png` the `ANALOG CHARACTER & TWEAKS` panel is dimmed but
carries no overlay or explanation, while the adjacent `OUTPUT CLIPPER` panel in the same
row is fully active. Two side-by-side panels in opposite states, only one of which
explains itself.

**Required fix:** Any panel disabled by the topology switch must state why, using the same
overlay treatment as the souped-up panel (or a single overlay that visibly covers both
disabled regions). A dimmed panel with no explanation is not acceptable.

## Issue 16 — Overlay call-to-action does not read as a button

**Observed:** The `ENABLE FUN MODE` action inside the "ACCURATE MODE ACTIVE" overlay is
orange text with a faint outline. The comparable primary action elsewhere in the UI,
`GENERATE`, is a solid orange fill. The overlay's own background is only slightly lighter
than the dimmed panel behind it.

**Required fix:** `ENABLE FUN MODE` must use the same primary-button treatment as
`GENERATE` — solid accent fill, dark label. The overlay card must be clearly separated
from the dimmed panel behind it (raise its background, or add a border at the same
contrast used for active panel borders). Overlay body text must reach at least 4.5:1
contrast against the card.

## Issue 17 — Playhead highlight uses two treatments and skips rows

**Observed:** In `running.png`, playhead step 04 draws the step-number cell as a solid
orange fill and the pitch cell as an orange outline, while the `ACC`/`SLD` pair and the
`NOTE`/`TIE`/`REST` cell in the same column carry no playhead marking at all.

**Required fix:** The playhead must mark the step column with one consistent treatment
that covers the whole column, and it must not conflict with a cell's own state color
(orange `ACC`, cyan `NOTE`, yellow `TIE`). Preferred: a single column-wide indicator
(a full-height column background tint, or a bar spanning the column's full width) rather
than per-cell fills and outlines. The marking must be equally visible on a `REST` step
(step 04) and on a `NOTE` step — verify both.

## Issue 18 — Four different colors mean "selected"

**Observed:** Orange marks selection on `FUN`, `LOW`, `4-POLE`, `ENV`, `2X`,
`SYNC: HOST`. Red/pink marks selection on `DIODE: SI` (border and label). Cyan marks
`MUTATE` and the `NOTE`/`SLD` cells. Yellow marks `TIE` cells. Red as a neutral selection
reads as an error state.

**Required fix:** Segmented-control selection uses exactly one accent across the whole
UI — orange. Convert `DIODE: SI` to that treatment. Sequencer cell colors may keep their
semantic palette (note / tie / accent / slide), but no semantic color may double as the
generic "this option is selected" indicator. Document the final mapping in
`plugin303/src/gui/theme.rs` so it is enforced in one place.

## Issue 19 — Three different section-title colors

**Observed:** Orange: `SOUPED-UP ENGINE & ANALOG DRAWER`, `ACID EUCLIDEAN GENERATOR`.
Red: `OUTPUT CLIPPER`. White/grey: `ANALOG CHARACTER & TWEAKS`, `ENVELOPES`,
`TRANSPORT & PATTERN`.

**Required fix:** One color for top-level panel titles, one for sub-panel titles.
Every panel of the same rank uses the same title color. `OUTPUT CLIPPER` must stop being
red.

## Issue 20 — OUTPUT CLIPPER knob arcs are pink/red; every other knob is orange

**Observed:** The clipper's ten knobs draw pink/red arcs. All other knobs in the UI —
core synth, souped-up, tweaks — draw orange arcs.

**Required fix:** Use one arc color for all knobs, unless the deviation encodes something
the user needs (it currently does not — the clipper is not in a warning state). If the
intent is to visually group the clipper section, express that with the panel frame or
title, not with a warning-adjacent hue on every control.

## Issue 21 — GENERATE and MUTATE are sibling buttons in unrelated colors

**Observed:** `GENERATE` is solid orange; `MUTATE` is solid cyan. Cyan already means
`NOTE` in the sequencer grid immediately below.

**Required fix:** Give the two buttons a primary/secondary relationship in the same
palette — for example orange fill for `GENERATE` and an orange-outlined secondary for
`MUTATE`. Cyan must not be used for buttons while it carries sequencer-cell meaning.

## Issue 22 — Adjacent header tags are styled differently

**Observed:** `BASS SYNTH` renders as orange text; `MONO` renders as white text. They sit
side by side and are the same class of element.

**Required fix:** Both tags use the same treatment, or a documented difference (for
example: product-class tag versus voice-mode tag) expressed consistently. Not one orange
and one white by accident.

## Issue 23 — Dropdown arrow glyphs point in different directions

**Observed:** `ZOOM` and `TOL` use a down-pointing triangle. The transport's `16th` and
`Direct` dropdowns use an up-pointing triangle.

**Required fix:** Every dropdown uses the same glyph and orientation.

## Issue 24 — Two transport dropdowns are unlabeled

**Observed:** `16th` and `Direct` sit in the transport panel with no label, while their
neighbors show `BPM:`, `LEN:`, and `JIT`.

**Required fix:** Label both dropdowns in the same style as their labeled neighbors.

## Issue 25 — WAVE selector renders an icon only for the selected option

**Observed:** The `SAW` row shows an orange-bordered icon box containing a ramp glyph.
The `SQR` and `SWP` rows are empty inside the selector capsule; only their text labels
appear, outside the capsule to the right. The capsule track also extends roughly 50 px
below the `SWP` row.

**Required fix:** All three options render their own waveform glyph in the same slot
geometry, with the selected one distinguished by the standard orange selection treatment.
The capsule's height must end at the last option's slot — no empty tail. Labels and glyphs
must sit in a consistent relationship for all three rows.

## Issue 26 — The two seed fields use different formats

**Observed:** `COMP SEED: 1337` is decimal. `GEN SEED: 000000000000002A` is a 16-digit
zero-padded hex value — a raw internal representation, and wide enough to dominate the
generator panel's header row.

**Required fix:** Both seeds use the same user-facing format. Prefer decimal for both, or
hex for both without unnecessary zero padding. If the full 64-bit value must remain
copyable, keep it but shorten the default presentation.

## Issue 27 — BPM `-` and `+` buttons are undersized

**Observed:** The two micro-buttons flanking `120.0` are about 20 px tall against a taller
value field, and are the smallest hit targets in the UI.

**Required fix:** Match the height of the value field they flank, and reach at least
44x44 device pixels (22x22 logical points) of clickable area each.

## Issue 28 — Truncated and unusual labels

**Observed:** `JIT` is a truncated abbreviation. `AGE: 0 yrs` uses an unusual unit.

**Required fix:** Expand `JIT` to a full word if it fits (`JITTER`), otherwise keep the
abbreviation but ensure a tooltip explains it. `AGE`/`yrs` is flagged as probably
intentional — leave it unless it is not.

## Issue 29 — `CLEAR` renders dimmer than its sibling buttons

**Observed:** In all three screenshots `CLEAR` is visibly dimmer than `SYNC: HOST` and the
`SHIFT` buttons, with no evident reason for it to be disabled.

**Required fix:** If `CLEAR` is enabled, style it like its enabled siblings. If it is
genuinely disabled in these states, make the disabled treatment match the one used
elsewhere in the UI so the state is unambiguous.

## Issue 30 — DIODE control crowds the OUTPUT CLIPPER panel's right border

**Observed:** The `GE` option's right edge is at x≈1280 against a panel border at x≈1296
(16 px), while the panel's left inner margin is 20 px.

**Required fix:** Equalize the panel's left and right inner margins, within 2 px.

## Issue 31 — Product name

**Observed:** The title bar and header both read `TECHNEO 303`; the repository and crate
are named `vibe303`.

**Required fix:** Confirm which name ships. If `TECHNEO 303` is correct, no code change —
say so in your report. If not, update the display name in one place and make the title bar
and header read from it.

---

## Guardrails

- Presentation layer only for Issues 1–7 and 10–31. Do not change parameter ranges,
  defaults, smoothing, persistence, DSP behavior, or the audio thread. Issues 8 and 9 may
  require a generator change; scope it to the generator and cover it with a test.
- Do not add new controls, new parameters, or new panels.
- Do not remove any existing control, readout, or state.
- Do not introduce real-time allocation. The DSP no-allocation tests must stay green.
- Colors and spacing constants belong in `plugin303/src/gui/theme.rs`. Do not hardcode a
  new hue or pad value at a call site when a token exists or should exist.
- No regressions to what the previous pass already fixed: transport content must not clip
  at the window bottom (last content row near y=1454, panel border near y=1476, window
  1503), panel left and right margins stay symmetric at 20 px, and the playhead must
  remain present while the transport is running.
- Keep the fixed 1280x752 logical window size.

## Verification

Re-capture all three states and re-inspect:

```
./scripts/capture_standalone_gui.sh
```

This writes `target/gui-qa/latest.png` (default / FUN, stopped),
`target/gui-qa/accurate.png` (ACCURATE, stopped), and `target/gui-qa/running.png`
(FUN, playing), then regenerates the section crops.

Then run the gates:

```
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

Measure, do not eyeball, for these:

- Issue 1: bottom margin below the last panel in `accurate.png` versus `latest.png` —
  must match within 4 px.
- Issue 2: top y and bottom y of the OUTPUT CLIPPER and TWEAKS panel borders — must match
  within 2 px.
- Issue 3: left border x of the core synth panel — must equal every other panel's.
- Issue 7: for each `%`-suffixed knob, pointer angle at 0 / 50 / 100 percent.
- Issue 12: `LOW` must remain visibly selected in `accurate.png`.
- Issue 17: playhead marking on a REST step and on a NOTE step.
- Issue 30: left and right inner margins of the OUTPUT CLIPPER panel.

Report per-issue outcomes as `Issue N — fixed` / `Issue N — no change needed, because …`
/ `Issue N — deferred, because …`, using these numbers. For Issues 8, 9, and 31, report
your finding even when no code changes.
