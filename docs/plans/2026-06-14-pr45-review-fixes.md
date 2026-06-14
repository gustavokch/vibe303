# PR #45 review fixes — GUI HiDPI scale

Addresses review on PR #45 (`fix/gui-hidpi-scale`). Order: blocker → medium → minor.

## 1. 🔴 BLOCKER — platform double-scaling

**Problem:** old code gated the scale pin with `#[cfg(not(target_os = "macos"))]`; new code applies `ctx.set_zoom_factor` on every platform. On macOS Retina (and possibly Windows), baseview/OS may already render at backing scale. If the host also reports scale via `set_scale_factor`, egui zooms on top → double-scaled UI. Verified only on Linux/X11.

**Options (pick one):**
- (A) Platform-gate the zoom application: only apply egui zoom on `target_os = "linux"`; leave OS-native scaling on macOS/Windows. Lowest risk, matches old intent.
- (B) Verify on Windows + macOS Retina that hosts do NOT call `set_scale_factor` (so `host_scale` stays 1.0) and document the evidence. Keeps unconditional code.

**Decision:** chose (A) — gated egui zoom + `frame_scale` clone to `target_os = "linux"`. (commit `17b2eac`)

**Tasks:**
- [x] In `editor.rs` update closure, gate `set_zoom_factor` behind `#[cfg(target_os = "linux")]`.
- [~] Re-test: Linux/X11 HiDPI verified. Win/macOS now structurally match native behavior (no egui zoom applied) — risk removed, but real HiDPI smoke test still worthwhile before release.
- [x] Update the in-code comment to state the platform behavior.

## 2. 🟡 Clamp `effective_zoom` upper bound

**Problem:** `Auto` accepts any `host_scale >= 0.25`, no ceiling — garbage large value → unusable giant UI, user may not be able to reach the override.

- [x] Clamp to sane range — `host_scale.clamp(0.5, 4.0)` in the `Auto` arm.

## 3. 🟡 `UiScale` persisted across display changes

**Problem:** non-automatable `EnumParam` is saved in state; 200% set on HiDPI then loaded on 1× display = oversized UI.

- [x] Documented escape-hatch behavior on the `UiScale` enum (kept persisted; `Auto` default sidesteps).

## 4. 🟢 Unit test for `effective_zoom`

- [x] Added `mod tests`: Auto 0/-1/NaN/∞ → 1.0; Auto passthrough; clamp (0.1→0.5, 99→4.0); each explicit `UiScale::Xnnn` → correct factor. `effective_zoom` gated `#[cfg(any(target_os = "linux", test))]`.

## 5. 🟢 Minor

- [x] `v2s_f32_rounded` display-only — knobs custom-drawn, no text entry; left as-is.
- [x] `docs/plans/2026-06-10-gui-plan.md` 660×460 — left as historical plan doc.

## Verification gate
- [x] `cargo test -p ba283 --release` → 83 passed (+4), 5 ignored.
- [x] Default + `--features trafo-core` build clean; no new clippy on touched files.
- [~] Visual: Linux/X11 HiDPI exact fit verified. Win/macOS no-double-scale = structural (no egui zoom); real smoke test pending before release.
