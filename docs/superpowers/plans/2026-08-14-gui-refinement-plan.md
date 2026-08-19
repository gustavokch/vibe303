# GUI Refinement Plan — Visual QA Round 6

**Date:** 2026-08-19
**Source spec:** `docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md`
**Input:** Visual QA findings across three sections (classic panel, souped-up drawer, sequencer).
**History:** Round 1 (2026-08-14) scored 58.95/100. Round 2 (2026-08-19) scored 86.45/100. Round 3 (2026-08-19) scored 89.65/100. Round 4 (2026-08-19) scored 94.00/100. Round 5 (2026-08-19) scored 94.35/100 with `spec_compliance = true`. This plan supersedes Round 5. Regression note: Round 5 Tasks 1.1 (two-tone TECHNEO wordmark) and 1.2 (vent slits) were not executed or regressed — the wordmark still renders single-tone orange and the vent slits render uneven; both are carried forward here.

## Weighted Alignment Score

| Section | Weight | Score | Weighted |
|---|---|---|---|
| Classic panel | 0.35 | 92 | 32.20 |
| Souped-up drawer | 0.30 | 85 | 25.50 |
| Sequencer | 0.35 | 96 | 33.60 |
| **Overall** | | | **91.30 / 100** |

Gate: all sections must pass for `spec_compliance = true`. Current state: **FAIL** — the souped-up drawer reports `spec_compliance: false` on one major finding (Accurate-mode layout collapse, Task 1.1). Classic panel and sequencer pass; their findings are minor polish.

---

## Phase 1 — Accurate-Mode Layout Collapse (souped drawer, GATE-BLOCKING)

One major finding. It blocks the gate and also produces the secondary finding (dead band at the window bottom). Fix the root cause; the secondary finding resolves with it.

### Task 1.1 — OUTPUT CLIPPER panel overlaps the ghosted drawer in Accurate mode (major, geometry)

- **Files:** `plugin303/src/gui/panels/souped_panel.rs` (drawer allocation and scrim pass), `plugin303/src/gui/mod.rs` (master layout order), `plugin303/src/gui/panels/output_clipper.rs` (victim panel)
- **Observed:** in `accurate.png` the OUTPUT CLIPPER panel shifts up ~155 px and its top edge cuts through the ghosted drawer's knob row. The dimmed top halves of the DRIVE / SUB / GLIDE / PITCH1 / PITCH2 knobs poke out above the vibrant clipper panel, and the DRIFT ghost knob sits beside its right edge. The drawer is painted at its full footprint while its layout reservation collapses beneath it.
- **Expected (spec 4.1):** the SoupedUpPanel renders at 30% opacity **in place** with an overlay badge; surrounding panels must not overlap the dimmed drawer. Either the drawer keeps its layout reservation (clipper stays below it, as in FUN mode) or the drawer is fully hidden and the reflow is clean. The half-and-half state (painted ghost + collapsed reservation) is the defect.
- **Root cause hypothesis:** in `render_souped_panel`, some part of the drawer content stops claiming vertical space when `is_accurate` is true (for example a branch inside the `ui.add_enabled_ui(!is_accurate, ...)` block, a skipped row, or a min-size that is only applied on the FUN path), while the 70% occlusion scrim at the end of the function is painted against the full content rect captured earlier. The next panel in `mod.rs` (output clipper, line 56) then packs into the collapsed allocation.
- **Fix (preferred):**
  1. In `souped_panel.rs`, make the drawer's layout allocation topology-independent: the same rows, the same `ui.set_width(762.0)` / min-width 1060.0, and the same vertical extents must be claimed on both the Accurate and Fun paths. Render the Accurate state as a paint-only difference — full content pass (with input disabled) plus the 70% scrim rect and centered badge painted on top, exactly as the code comments at lines 41–48 already intend.
  2. Verify no branch inside the enabled/disabled block early-returns or skips `ui.add_space` / row closures only in Accurate mode. If a skipped row is deliberate, reserve its height explicitly (e.g. `ui.allocate_space` with the FUN-mode height).
- **Fix (alternative, only if the in-place ghost proves impractical):** do not paint the drawer at all in Accurate mode; show the badge against the chassis background and let the layout reflow cleanly with no ghost geometry. This is a weaker reading of spec 4.1 ("rendered at 30% opacity in place") — prefer the primary fix.
- **Verification:** `accurate.png` capture — the OUTPUT CLIPPER panel's top edge sits at the same y as in `latest.png` (FUN mode); no drawer knob pixels are visible above or beside the clipper panel; the scrim rect exactly covers the drawer frame and nothing else.

### Task 1.2 — Dead dark band at the window bottom in Accurate mode (minor, geometry, resolved by 1.1)

- **File:** `plugin303/src/gui/mod.rs`
- **Observed:** the same ~155 px collapse shifts the generator and sequencer up, leaving an empty dark band at the bottom of the 1080x640 frame in `accurate.png`; FUN mode fills the frame.
- **Expected (spec section 5):** content fills the fixed window in both topologies.
- **Fix:** no separate change — restoring the drawer's reservation (Task 1.1) restores the FUN-mode vertical rhythm. After the fix, confirm no residual band remains; if a small gap persists, absorb it in the inter-panel `ui.add_space(5.0)` stack in `mod.rs` rather than in any panel.
- **Verification:** `accurate.png` capture — bottom-most content row lands within a few pixels of the FUN-mode baseline; no uniform `#16181b` band taller than the standard inter-panel spacing.

---

## Phase 2 — Header and Classic Dial Polish (classic panel, minor)

All three classic-panel findings live in `plugin303/src/gui/panels/header_bar.rs` and `plugin303/src/gui/widgets/knob.rs`. The section passes the gate (92); this phase is polish.

### Task 2.1 — TECHNEO wordmark two-tone split (minor, color; carried from Round 5 Task 1.1)

- **File:** `plugin303/src/gui/panels/header_bar.rs`
- **Observed:** the full TECHNEO wordmark renders in a single orange tone.
- **Expected (mockup):** silver/white `TECHN` with orange `EO`.
- **Fix:** split the wordmark into two spans — paint `TECHN` in the light aluminum token `#d0d4d8` (spec section 5 classic-knob aluminum; optionally a vertical gradient toward `#888e96` for the chrome read) and `EO` in Souped-Up Orange `#ff8800`. If the wordmark is a single `RichText`, convert to a two-section `LayoutJob` with per-section color; keep font, size, and letter spacing unchanged.
- **Verification:** header capture — pixel-sample a stroke pixel inside `TECHN` (near `#d0d4d8`, channels within ±6 of neutral silver) and inside `EO` (near `#ff8800`); no orange remains in the `TECHN` span.

### Task 2.2 — Vent slits uniform width and stride (minor, geometry; supersedes Round 5 Task 1.2)

- **File:** `plugin303/src/gui/panels/header_bar.rs`
- **Observed:** the two center vent-slit groups render with uneven slit widths and irregular spacing, reading as barcode-like strips.
- **Expected (mockup):** uniform, evenly spaced vertical slits of equal width in both groups.
- **Fix:** render the vents procedurally with a fixed slit width and constant stride — compute `x = group_origin + i * stride` for an equal slit count per group, one width value for all slits — centered between the wordmark and the topology switch. Remove any per-slit variation in the current loop. Keep the group extents established in earlier rounds so the slits do not collide with the logotype or the topology switch at 1080x640.
- **Verification:** header capture — within each group, measured slit widths and gaps vary by ≤ 1 px; both groups are symmetric about the header midline.

### Task 2.3 — Knob tick-mark legibility on the 7 classic dials (minor, geometry/color)

- **File:** `plugin303/src/gui/widgets/knob.rs`
- **Observed:** tick marks span the 270° sweep (135°–405°) correctly but render faint and low-contrast against the dark panel at the window scale.
- **Expected (spec 3.1, mockup):** outer bezel ring with clearly visible radial tick marks; mockup shows crisp light ticks.
- **Fix:** increase tick stroke width slightly and raise the tick color brightness toward a lighter gray. The panel-border token `#2a2e33` is too dark for ticks on the `#16181b` chassis — use a mid/light gray in the aluminum family (for example a dimmed `#888e96`–`#d0d4d8` mix) so the ticks read at 1080x640 without competing with the value arc. Keep the existing angular span and count.
- **Verification:** classic-panel capture — ticks are legible at 100% zoom on all 7 dials; sampled tick pixels are clearly brighter than the chassis background (luminance delta ≥ 40) and do not clip into the orange value arc.

---

## Phase 3 — Sequencer Token Fidelity (sequencer, minor)

Sequencer passes the gate (96). Two minor items on the playhead LED bar.

### Task 3.1 — Playhead LED lit core samples `#ff4769`, above the `#ff3355` token (minor, color)

- **Files:** `plugin303/src/gui/widgets/step_button.rs` (playhead bar glow pass), `plugin303/src/gui/theme.rs` (Playhead Neon Coral token)
- **Observed:** the lit bar core at step 13 samples `#ff4769` — slightly lighter and more pink than the spec token.
- **Expected (spec section 5):** Playhead Neon Coral `#ff3355`.
- **Fix:** reduce the bloom/white-core intensity in the LED glow pass so the bar core reads closer to `#ff3355`; keep the halo translucent. Same treatment family as the Round 5 SLD-dot clamp (Round 5 Task 3.1) if that task was executed — apply one shared "hot core clamps to token" helper if both dots and bars need it.
- **Verification:** sequencer capture with the playhead on a lit step — the bar core pixel-samples within ±6 per channel of (255, 51, 85); the halo remains visible.

### Task 3.2 — Lit playhead bar renders ~2–3x taller than dim bars (minor, geometry)

- **File:** `plugin303/src/gui/widgets/step_button.rs`
- **Observed:** the lit bar at step 13 is a noticeably taller/fatter pill than the dim bars, breaking the uniform bar silhouette across the 16-step strip.
- **Expected (spec 3.3 item 1):** a top LED bar that illuminates; uniform bar geometry with only color/glow changing keeps the grid rhythm consistent.
- **Fix (choose one):**
  1. Keep lit and dim bars the same height/width; express the active state only via color fill and outer glow (preferred — matches the mockup rhythm).
  2. If the size pulse is a deliberate accent tied to the border-pulse behavior (spec 4.2), record it as an accepted deviation in the spec alongside the existing 2026-08-19 entries, and keep it.
- **Verification:** sequencer capture — lit and dim bars share the same rect dimensions (option 1), or the deviation note is present in the spec (option 2).

---

## Deferred / No Action

- **MEG/VEG sustain `0%` readouts render orange while other ADSR readouts render gray (minor, color, souped drawer):** consistent across both banks and reads as a deliberate zero-value highlight. Spec 3.2 is silent on value-dependent coloring. Decision: treat as deliberate; no action. If uniform styling is later preferred, render all ADSR readouts in the same neutral gray regardless of value in `plugin303/src/gui/widgets/fader.rs`.

---

## Exit Criteria

1. Task 1.1 verified by `accurate.png` capture: OUTPUT CLIPPER panel below the drawer at the FUN-mode y position, no overlap, scrim confined to the drawer frame. Task 1.2 confirmed resolved by the same capture.
2. Tasks 2.1–2.3 verified by header and classic-panel captures: two-tone wordmark, uniform vent slits, legible tick marks.
3. Tasks 3.1–3.2 verified by sequencer capture (or 3.2 option 2 recorded in the spec).
4. Re-run the visual QA loop; target overall weighted score >= 95 with `spec_compliance = true` restored (souped drawer back above the gate).
5. `cargo test -p plugin303 --all-features` passes; no regressions in the headless DSP suite.
