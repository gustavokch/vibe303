# Plan — address PR #4 review findings (303 fidelity tiers)

**Date:** 2026-06-27
**Branch:** `feat/phase2-303-fidelity-tiers`
**Source:** `/review` of PR #4 — https://github.com/gustavokch/vibe303/pull/4#issuecomment-4822084082

Findings grouped by subsystem. Each task is TDD where a test is feasible; commit each separately.

---

## Group 1 — Ultra nodal-solver correctness (highest priority)

### T1 — JFET reverse-mode `gm` sign (`jfet.rs:54`)
- **Bug:** reverse branch returns `gm: r.gm`; correct is `gm: -r.gm` (chain rule, `dId/dVgs = -r.gm` when `vds<0`). `gds`/`id` already correct.
- **Test (red):** unit test in `jfet.rs` — pick `vgs`, `vds<0`; assert returned `gm` ≈ numeric `dId/dVgs` (finite-difference of `channel().id` w.r.t. `vgs`) within tol. Currently fails (wrong sign).
- **Fix:** `gm: -r.gm`.
- **Verify:** new test green; `ultra_stability.rs` + `ultra_spectral.rs` still pass (LSD≤5.5, corr≥.95) — confirm no spectral regression and ideally a convergence-iters improvement on reset edges.

### T2 — homotopy returns non-converged operating point (`vco.rs:375`)
- **Bug:** if `gmin=1e-4` converges but final `gmin=0.0` fails, loop `break`s and returns the `1e-4` solution (~1e-4 S leakage ≈ R8) → wrong sample.
- **Fix options (pick one):**
  - (a) Only accept a solution from the `gmin == 0.0` step; if it fails, hold the previous sample (`self.v` unchanged) and count a fallback.
  - (b) Continue the homotopy with a finer tail (`5e-5, 1e-5, 0.0`) before giving up.
  - Recommend (a) — simplest, audibly safe (1-sample hold ≪ glitch).
- **Test:** add a `#[ignore]` diagnostic that counts gmin>0 acceptances over a sustained Ultra render at a stiff op-point; assert 0 after fix (or expose a counter). At minimum assert no NaN/discontinuity > threshold in the Ultra output.
- **Verify:** `ultra_spectral.rs` still green.

### T3 (optional, related) — BJT `qb` clamp Jacobian inconsistency (`bjt.rs:347`)
- **Issue:** when `inv1` is clamped to `0.1`, `q1` is frozen but `dq1_vbe/dq1_vbc` stay nonzero → Jacobian disagrees with residual in deep saturation, slowing Newton.
- **Fix:** when `inv1 < 0.1`, zero `dq1_vbe`/`dq1_vbc` (q1 constant in clamp region).
- **Verify:** convergence-iters diagnostic improves or unchanged; spectral gate green. Lower priority — convergence robustness, not a wrong sample.

---

## Group 2 — Crossfade / host PDC (F5 correctness)

### T4 — `reset()` reverts tier to Low (`lib.rs:69`)
- **Bug:** `reset()` rebuilds `Voice` (tier→Low) without re-applying `params.tier`; first `process()` then crossfades Low→target on every transport start.
- **Fix:** after `self.voice = Voice::new(...)`, call `self.voice.set_tier_immediate(self.params.tier.value())` (mirror `initialize()`). Also reset `self.reported_latency` consistently.
- **Test:** unit/integration — construct plugin, set params tier=Ultra, call `reset()`, assert `voice` is on Ultra with no pending xfade (`xfade_from.is_none()`).

### T5 — latency reported before crossfade completes (`voice.rs:111`, `lib.rs:85`)
- **Bug:** `latency_samples()` keys off `self.tier` (set instantly); host re-aligns to new delay while output is still a 2-path blend for ~10 ms.
- **Fix (pick one):**
  - (a) **Preferred — pad all tiers to a common max latency** (Ultra's 4× OS delay). Report a constant latency; tiers shorter than max get a fixed delay line. Eliminates runtime latency changes entirely (also fixes host re-init/flush risk).
  - (b) Hold reported latency at `max(from_tier, to_tier)` until `xfade_pos>=1.0`, only then drop to the target.
- **Test:** assert `latency_samples()` never changes mid-fade (option b) or never changes at all after init (option a). Update `params_tier.rs`/PDC contract test.
- **Note:** option (a) also resolves the "DAW treats runtime latency change as graph re-init" hazard — recommend (a).

### T6 — re-entrant `set_tier` hard-cuts a weighted path (`voice.rs:104`)
- **Bug:** mid-fade `set_tier` overwrites `xfade_from` with current target + resets `xfade_pos=0`, dropping the still-weighted original path → click on fast multi-tier sweeps.
- **Fix:** if a fade is in flight (`xfade_from.is_some()` and `xfade_pos<1`), snapshot the current *blended* output as the new `from` baseline, or defer the new target until the current fade completes (queue one pending tier). Simplest robust option: keep fading to the in-flight target, store `pending_tier`, start the next fade when `xfade_pos>=1`.
- **Test:** `tier_switch.rs` — drive Low→High→Ultra within one fade window; assert max inter-sample step ≤ existing click threshold (2.5× steady slew).

---

## Group 3 — Altitude / rate-invariance

### T7 — VCO DC-blocker pole hardcoded (`vco.rs:391`)
- **Fix:** derive pole from a target corner: `r = exp(-2π·f_hp/sr)` (e.g. `f_hp≈5 Hz`); replace literal `0.999`.
- **Test:** assert HP −3 dB corner ≈ f_hp at both 48 k and 96 k (currently corner moves with sr).

### T8 — Ultra saw upsampled after generation (`voice.rs:167`)
- **Issue:** band-unlimited relaxation saw generated at base sr, then upsampled → most-aliased source in the "highest fidelity" tier.
- **Fix options:** generate `Vco303` at the OS rate (tick 4× per base sample) — costly but principled; OR apply polyBLEP/BLEP correction to the reset edge before upsampling; OR run the VCO Newton at OS rate only around reset edges.
- **Decision needed** — cost vs fidelity. Likely defer to an "Ultra headroom" follow-up (already noted as deferred in design doc), but record the aliasing as a known limitation in the design doc now.
- **Test:** alias-floor test analogous to `ladder_alias.rs` on the Ultra path at a high note; document the measured floor.

---

## Group 4 — Cleanup (low risk, do last)

### T9 — dedup `solve()` → `lu_solve` (`ladder_wdf.rs:80`)
- Replace private `solve` with `lu_solve::<10>` from `linalg.rs`; delete the duplicate. Run `ultra_spectral.rs` to confirm identical numerics.

### T10 — drop/gate dead code
- `oversample.rs` `Os8x` and `diode.rs` `Diode1N4148::charge()` — move to `#[cfg(test)]`, feature-gate, or delete (F5 dropped the OS-switch param; 8× unreachable). Keep their tests if the code is retained as headroom.

### T11 — fix `Os2x::latency_samples()` comment (`oversample.rs:110`)
- Comment says "31 base samples"; returned/tested value is 32. Correct comment to 32.

---

## Sequencing
1. **Group 1** (T1, T2) — wrong-sample bugs, ship-blockers for Ultra. T3 optional.
2. **Group 2** (T4, T5, T6) — F5 click-free/PDC claims are currently undermined; T4 is the most user-visible (every transport start).
3. **Group 3** (T7 now; T8 decision — likely defer + document).
4. **Group 4** (T9–T11) — mechanical, batch into one cleanup commit.

Each group → separate commit(s); re-run `cargo test -p plugin303` (65 passed / 8 ignored baseline) + all spectral + no-alloc + click-free gates after each.
