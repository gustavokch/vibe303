# PR #51 Review Remediation — Ultra Ladder Rate Invariant

**Date:** 2026-08-24
**PR:** [#51 fix(dsp): Ultra ladder rate mismatch on tier-immediate path](https://github.com/gustavokch/vibe303/pull/51)
**Branch:** `fix/ultra-ladder-tier-immediate`
**Review:** posted to PR (3 findings: 1 risk, 1 nit, 1 question)

## Goal

Close all review findings on PR #51:

1. **🟡 `voice.rs` `Voice::new` adaptive ladder rate** — constructor still builds
   the shared Ultra ladder at an SR-dependent factor (2x @44.1/48k, 1x @>=88.2k)
   inherited from buggy commit 21b7307, while the Tier::Ultra render arm ticks
   it 4x/sample. Post-fix this state is always overwritten by a setter before
   any Ultra render (initial tier is `Low`; every caller routes through
   `set_tier`/`set_tier_immediate`), so it is latent-only — but any future
   code path that renders Ultra without a setter silently mistunes.
   Fix: build at `sr * ULTRA_OS_FACTOR`, restoring the pre-21b7307 line, and
   pin with a unit test.
2. **🔵 guard consolidation** — `rebuild_ultra_ladder_if_needed` uses
   `debug_assert!` plus duplicated call-site guards (`set_tier_immediate`
   L888, `set_tier` L1021). Release builds misbuild an Os4x ladder if a future
   caller passes non-Ultra. Fix: runtime early-return inside the helper,
   drop both guards and the assert.
3. **❓ LSD budget headroom** — 17.0 dB budget vs measured 15.63 dB leaves
   1.37 dB margin (baseline margin was 4.4). Resolve by documenting the
   intentional tight margin in the test comment; no numeric change.

## Architecture

Single crate change confined to `plugin303/src/dsp/voice.rs` +
`plugin303/src/dsp/ladder_wdf.rs` (one diagnostic getter) + one test comment.
No public API change beyond a diagnostic getter consistent with existing
idiom in `ladder_wdf.rs`. No DSP behavior change: constructor-built ladder is
always rebuilt by a setter before any Ultra render today, and Task 2 is a
pure control-flow refactor with identical semantics.

## Tech Stack

Rust, cargo nextest (`cargo nextest run -p plugin303 --release --profile slow`).

## Tasks

### Task 1 — Pin constructor ladder rate at `sr * ULTRA_OS_FACTOR`

**Target files:**
- Modify `plugin303/src/dsp/ladder_wdf.rs`: add `pub fn sample_rate(&self) -> f64`
  diagnostic getter (file already carries "Diagnostic:" getters).
- Create `#[cfg(test)] mod tests` at bottom of `plugin303/src/dsp/voice.rs`.
- Modify `plugin303/src/dsp/voice.rs::Voice::new`: delete the `ultra_factor`
  adaptive block, restore `LadderVcf::new(sr * ULTRA_OS_FACTOR)`.

**Consumes:** `LadderVcf::sample_rate()`, `ULTRA_OS_FACTOR = 4.0`,
`HIGH_OS_FACTOR = 2.0` (unchanged, correct — High ticks 2x/sample via `Os2x`).

**Step 1 — failing test** (`plugin303/src/dsp/voice.rs`):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// Regression pin for 21b7307: the shared Ultra ladder must be built at
    /// the tier's oversampled clock (`sr * ULTRA_OS_FACTOR`) because the
    /// Tier::Ultra arm drives it 4x per output sample through Os4x and the
    /// nodal solver tunes its capacitors from the constructor rate. The
    /// setter-based rebuild (rebuild_ultra_ladder_if_needed) assumes this too.
    #[test]
    fn voice_new_builds_ultra_ladder_at_oversampled_rate() {
        for sr in [44_100.0f32, 48_000.0, 96_000.0] {
            let v = Voice::new(sr);
            assert!(
                (v.ladder.sample_rate() - (sr as f64) * ULTRA_OS_FACTOR as f64).abs() < 1e-6,
                "sr {}: ladder built at {} Hz, expected {} Hz",
                sr,
                v.ladder.sample_rate(),
                sr as f64 * ULTRA_OS_FACTOR as f64
            );
        }
    }
}
```

**Step 2 — confirm red:**
`cargo nextest run -p plugin303 --release --test-threads=1 --lib voice::tests::voice_new_builds_ultra_ladder_at_oversampled_rate`

**Step 3 — implementation:** delete lines building `ultra_factor`
(`if (sr - 44100.0).abs() < 0.5 ... ULTRA_OS_FACTOR` block) and replace with:

```rust
let mut ladder = LadderVcf::new(sr * ULTRA_OS_FACTOR);
```

Add getter in `ladder_wdf.rs` near `set_cal`:

```rust
/// Diagnostic: sample rate the filter was constructed at (Hz). The nodal
/// solver tunes its capacitors from this value, so it must equal the actual
/// tick clock (`tier OS factor x host SR`) for correct tuning.
pub fn sample_rate(&self) -> f64 {
    self.sr
}
```

**Step 4 — confirm green:** same command.

**Step 5 — commit:**
```
fix(dsp): build constructor Ultra ladder at oversampled rate

21b7307's adaptive ultra_factor (2x @44.1/48k, 1x @>=88.2k) left Voice::new
with a ladder mistuned for the Tier::Ultra render arm, which ticks it 4x per
sample. Latent only — initial tier is Low and every Ultra render goes through
a setter rebuild — but any future direct-render path would be silently
mistuned. Restore pre-21b7307 construction and pin with a unit test.
```

### Task 2 — Move non-Ultra guard into helper, drop call-site duplication

**Target files:** Modify only `plugin303/src/dsp/voice.rs`.

**Semantics:** identical behavior; existing suite (200 tests incl.
`no_alloc`, `tier_switch`, `tier_calibration`) is the regression net — no new
test, since the guarded branch is not externally observable.

Helper becomes:

```rust
fn rebuild_ultra_ladder_if_needed(&mut self, tier: Tier) {
    if !matches!(tier, Tier::Ultra | Tier::Ultra2x) || self.tier == tier {
        return;
    }
    // ... unchanged body ...
}
```

Call sites: drop `if tier == Tier::Ultra || tier == Tier::Ultra2x { }` wrappers
at both `set_tier_immediate` (~L888) and `set_tier` (~L1021), calling
`self.rebuild_ultra_ladder_if_needed(tier);` unconditionally. Remove
`debug_assert!`.

**Verify green:** `cargo nextest run -p plugin303 --release --profile slow --no-fail-fast`

**Commit:**
```
refactor(dsp): fold non-Ultra guard into rebuild helper

Runtime early-return replaces debug_assert! + duplicated call-site guards,
so a release-build caller passing non-Ultra no longer silently builds an
Os4x ladder. Behavior identical.
```

### Task 3 — Document intentional budget margin

**Target files:** Modify `plugin303/tests/tier_calibration.rs` comment block
(~L197-204): append one line:
`Margin intentionally tight (~1.4 dB over the measured operating point) to keep the parity property under active drift watch.`

No test change (comment only). Commit with Task 2 or standalone:
```
docs(test): note intentional tight High-vs-Ultra LSD margin
```

## Verification Gate

Full suite before push:
```
cargo nextest run -p plugin303 --release --profile slow --no-fail-fast
```
Must be fully green (was 200/200 on PR #51 head), then
`git push origin fix/ultra-ladder-tier-immediate`.
