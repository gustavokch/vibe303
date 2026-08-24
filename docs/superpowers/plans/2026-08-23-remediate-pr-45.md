# Remediation Plan — PR #45 MIDI Out & Synth Bypass

**Branch:** `feat/midi-out-synth-bypass`
**Commit:** `9f53104`
**Date:** 2026-08-23
**Status:** Remediation in progress (post-review fixes applied)

---

## Findings from Review (caveman-style)

- `plugin303/src/lib.rs`: 🟡 risk — `SlideTo` sends `NoteOn` before `NoteOff` at same timing. Should be `NoteOff` then `NoteOn` for clean transition.
- `plugin303/src/lib.rs`: 🟡 risk — `synth_bypass` skips `self.voice.update_params()` and `set_tier()`; crossfade/state may desync when toggling bypass.
- `plugin303/src/lib.rs`: 🟡 risk — `NoteOff` when `!midi_out_enabled` uses `prev_ch` but if `prev_ch` changed since last event, channel routing could be wrong. Verify `prev_ch` always reflects previous event.
- `plugin303/src/params.rs`: 🔵 nit — `clipper_*` parameters switched from physical `FloatRange::Skewed` (ohm/nF values) to normalized `FloatRange::Linear` (0-1); mapping updated in `voice.rs`. Confirm docs reflect this.

---

## Remediation Tasks

### Task R1: Fix SlideTo Note Order
**Target:** `plugin303/src/lib.rs` `SlideTo` block (line 503-532)
**Fix:** Swap `NoteOn` / `NoteOff` order — send `NoteOff` (prev) first, then `NoteOn` (new).
**Test:** `plugin303/tests/midi_out.rs`
**Commit:** `fix(midi): correct SlideTo NoteOff/NoteOn sequence`

### Task R2: Preserve DSP State Across Bypass Toggle
**Target:** `plugin303/src/lib.rs` `process()` line 279-288
**Fix:** Keep `self.voice.update_params()` and `self.voice.set_tier()` calls even when `synth_bypass` is true; only skip `self.voice.tick()` and audio output.
**Test:** Add `test_bypass_preserves_voice_params`
**Commit:** `fix(dsp): preserve voice params during synth bypass`

### Task R3: Verify prev_ch Routing for NoteOff on Disable
**Target:** `plugin303/src/lib.rs` line 396-405
**Fix:** Confirm `prev_ch` is taken from `self.active_seq_midi_note` before `.take()`; this is already correct. No change needed unless test fails.
**Commit:** `test(midi): verify NoteOff channel routing on midi disable`

---

## Verification Gate

- `uv run pytest` (or `cargo test`) — 100% green required.
- `git push origin feat/midi-out-synth-bypass`
