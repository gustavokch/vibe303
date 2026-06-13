# PR #42 review fixes — CIRCUIT panel layout

Source: review of `fix/gui-circuit-panel-layout` (PR #42). Fix itself is correct and merged-ready;
these are cleanups surfaced in review. All in `plugin/src/editor.rs`.

## 1. Restore `knob()`'s doc comment (minor, do now)

**Problem:** No blank line between the `knob` doc block (L304-307) and the new `knob_cell` doc
block (L308-312). Rust merges adjacent `///` lines, so the "270° arc knob… Drag/scroll… double-click
resets" prose now attaches to `knob_cell`, and `knob()` is left undocumented.

**Fix:** Insert a blank line after the `knob_cell` doc block (before `fn knob_cell`), and move the
original "270° arc knob…" doc block down so it sits directly above `fn knob`. Each block attaches to
its own fn.

**Verify:** `cargo doc -p ba283 --no-deps` — confirm `knob` and `knob_cell` each carry the right doc.

## 2. De-duplicate cell dims (nit)

**Problem:** `egui::vec2(diameter + 20.0, diameter + 36.0)` appears in both `knob` and `knob_cell`.
Silent drift risk: if one changes, `knob_cell` no longer bounds to exactly one knob.

**Fix:** Extract `fn knob_cell_size(diameter: f32) -> egui::Vec2 { egui::vec2(diameter + 20.0,
diameter + 36.0) }`; call from both sites.

**Verify:** `cargo build -p ba283 --release` and `--features trafo-core`; visual parity in
`gui_standalone`.

## 3. (Optional follow-up) Layout regression guard

**Problem:** `cargo test` can't catch the clipping these bugs caused; only the manual `gui_standalone`
harness does. Recurrence likely given subtlety of egui centering rules.

**Options:**
- Lightweight test rendering `draw_ui` into an offscreen egui context at 660×460 and asserting the
  final cursor.y / content rect fits the viewport.
- Or an egui_kittest snapshot test of the panel.

**Decision:** Defer to its own PR; not blocking. Track as issue if not done with this batch.

## Sequencing
Items 1 + 2 are a single trivial commit on `fix/gui-circuit-panel-layout` (or a follow-up branch if
#42 already merged). Item 3 is separate. No DSP/param impact; verification = build both feature sets
+ visual `gui_standalone`.
