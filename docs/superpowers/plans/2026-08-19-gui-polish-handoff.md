# GUI Visual QA — Phases 3 & 4 (Polish)

## Context

Repo: `/Users/gus/Git/vibe303` — nih_plug_egui TB-303 plugin.
Plan: `docs/superpowers/plans/2026-08-14-gui-refinement-plan.md` (full task list + spec references).
Spec: `docs/superpowers/specs/2026-08-13-gui-frontend-egui-design.md`.

State: Phases 1 & 2 COMPLETE. Sequencer strip mounts and renders inside the
fixed 1080x640 frame; ACCURATE-mode dimming/banner/quick-switch verified.
Overall weighted score was 58.95/100 (sequencer scored 5/100); re-score after
this round. Uncommitted GUI changes on `main` — check `git status` first.

## Verification loop (use after every change)

```bash
bash scripts/capture_standalone_gui.sh   # builds release standalone, writes target/gui-qa/latest.png
```

Read `target/gui-qa/latest.png` (2160x1344 = 2x retina; divide by 2 for logical px).
Default topology is `CircuitTopology::Accurate` (params.rs:395), so captures land
in ACCURATE mode — toggle the CIRCUIT TOPOLOGY switch in the UI for FUN-mode
captures, or temporarily flip the default.

Layout budget: GUI_HEIGHT=640 logical. Current content ≈580 — ~60px headroom
available for polish work. Do NOT exceed the frame; no ScrollArea (spec deviation).

## Phase 3 — Souped-up drawer polish

1. **Task 3.1** — Replace VCA ENV ComboBox in `plugin303/src/gui/panels/souped_panel.rs`
   with the shared `SlideSwitch` widget (spec 3.2, detents: Gate / Env).
2. **Task 3.2** — DRIFT knob baseline: verify the merged knob row shares a common
   bottom edge (DRIFT moved into the 10-knob row in Phase 1 — likely already fixed;
   confirm visually).
3. **Task 3.3** — Add thin inset frame (`#2a2e33`) or divider around MEG ADSR /
   VEG ADSR banks; section-label weight on headers.

## Phase 4 — Classic panel & header polish

1. **Task 4.1** — Metallic TECHNEO logotype in `header_bar.rs` (silver gradient
   `#d0d4d8`→`#888e96` + orange accent).
2. **Task 4.2** — Two banks of procedural vent slits in the header.
3. **Task 4.3** — Master volume: decide header control vs. amend spec (record decision).
4. **Task 4.4** — Brushed-metal knob faces in `widgets/knob.rs` (gradient + bevel
   highlight ring; keep tick ring, pointer, orange arc).
5. **Tasks 4.5/4.6** — Layout deviations (switch label stacking, switches beside vs.
   below knob row): decide match-mockup vs. record accepted deviation in spec.

## Rules

- Batch edits by file to reduce churn (`souped_panel.rs`+`switch.rs`;
  `header_bar.rs`+`knob.rs`+`theme.rs`).
- Capture + visually inspect after each task; keep everything inside 1080x640.
- `cargo test -p plugin303 --release` must exit 0 (GUI-only changes; exit code is the gate).
- Exit criteria: weighted score ≥95, zero critical/major findings. Re-run the
  `/gui-visual-audit` skill when phases 3–4 are done.
