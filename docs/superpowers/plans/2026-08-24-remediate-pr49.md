# Remediation Plan — PR #49 Fixes

**Goal:** Fix bugs found in PR #49 review: `Ultra2x` missing in ladder wear (`L992`), float equality (`L388`).

---

## Task 6: Fix missing ladder wear/recalibration on tier switch
- Modify: `plugin303/src/dsp/voice.rs` (line 990-999)
- Fix: After `new_ladder` init, call `set_analog_wear` with current `recalibration_scale`.
- Commit: fix(voice): restore ladder wear/recalibration on Ultra2x tier switch
