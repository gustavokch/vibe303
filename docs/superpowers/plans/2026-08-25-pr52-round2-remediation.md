# PR #52 Review Remediation — Round 2

**Goal:** Fix 7 findings from round-2 review of PR #52 (GUI audit remediation branch).
**Architecture:** egui GUI layer (`plugin303/src/gui/`), no DSP changes.
**Tech Stack:** Rust, nih-plug, egui. Tests: `cargo test -p plugin303 --lib`.
**Spec:** review comment https://github.com/gustavokch/vibe303/pull/52#issuecomment-5418405986

Baseline: 66 passed, 0 failed.

---

## Task 1 — fader.rs dead conditional

- **Modify:** `plugin303/src/gui/widgets/fader.rs` (L92)
- **Finding:** 🔴 `if is_enabled { BORDER_DEFAULT } else { BORDER_DEFAULT }` — identical branches, disabled state lost.

**Step 1 (Red):** no unit-testable seam (pure paint code); verification = visual parity check via grep + full suite green.
**Step 2:** change else branch to `theme::BORDER_SUBTLE`.
**Step 3:** `cargo test -p plugin303 --lib` green.
**Step 4:** commit `fix(gui): restore disabled fader track border (BORDER_SUBTLE)`.

## Task 2 — knob.rs zero-snap: single parse, dedup label list

- **Modify:** `plugin303/src/gui/widgets/knob.rs` (L140-160, L186-188)
- **Findings:** 🟡 `param.to_string()` allocated+parsed twice per repaint (snap block + badge block); 🟡 redundant `TUNING` in `zero_center_branch` (`"TUNING".contains("IN")` is true). Note: `contains()` dispatch is the established idiom in this file — keep pattern, dedup only.

**Step 1 (Red):** add unit test for a pure helper `parse_plain_value(s: &str) -> Option<f32>` (split_whitespace + parse) in knob.rs `#[cfg(test)]`; confirm it fails to compile (helper missing).
**Step 2:** implement helper; hoist `raw_str`/`clean` parse above needle block; snap block reuses parsed `Option<f32>`; badge block reuses same bindings. Remove `TUNING` from snap label list.
**Step 3:** `cargo test -p plugin303 --lib` green.
**Step 4:** commit `refactor(gui): single display-string parse for knob snap+badge`.

## Task 3 — sequencer_panel stale comment + pin test

- **Modify:** `plugin303/src/gui/panels/sequencer_panel.rs` (L22-24, L500-504)
- **Findings:** 🔵 comment describes 12 px breakdown but value is 4.0; 🔵 `assert_eq!(FRAME_VERT_CHROME_PX, 4.0)` tautology.

**Step 1:** rewrite comment: empirically calibrated 4 px slack so frame bottom reaches window bottom.
**Step 2:** rename test to `frame_vertical_chrome_pinned` and document it as an intentional change-detector pin.
**Step 3:** `cargo test -p plugin303 --lib` green.
**Step 4:** commit `docs(gui): correct FRAME_VERT_CHROME_PX comment, rename pin test`.

## Task 4 — tweaks_panel CALIBRATE button builder cleanup

- **Modify:** `plugin303/src/gui/panels/tweaks_panel.rs` (L160-176)
- **Finding:** 🔵 redundant rebind `let btn_text = btn_text_builder;`, verbose mut flow.

**Step 1:** collapse into one `let btn_text = { ... };` expression (mut inside block).
**Step 2:** `cargo test -p plugin303 --lib` green.
**Step 3:** commit `refactor(gui): simplify CALIBRATE button text builder`.

## Task 5 — generator_panel octave formatter

- **Modify:** `plugin303/src/gui/panels/generator_panel.rs` (L231, L237)
- **Finding:** 🔵 `format!("{:+.0}", v as i32)` double conversion.

**Step 1:** replace with `format!("{:+.0}", v)`.
**Step 2:** `cargo test -p plugin303 --lib` green.
**Step 3:** commit `fix(gui): drop redundant int cast in octave formatter`.

## Task 6 — verify & push

1. `cargo test -p plugin303 --lib` — 100% green.
2. `cargo clippy -p plugin303` — no new warnings on touched files.
3. `git push origin fix/gui-visual-audit-remediation`.
4. Reply summary on PR.
