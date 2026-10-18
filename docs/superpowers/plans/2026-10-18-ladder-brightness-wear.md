# Ladder Brightness vs Analog Wear Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make each nodal tier play its calibrated VCF brightness (Low `0.50`, High/Best `0.70`) scaled by analog wear, instead of the `1.0 × age factor` that `LadderVcf::set_analog_wear` writes today. Measure the sound before and after the fix.

**Architecture:** `LadderVcf` stores two inputs: the tier brightness (`set_cal`) and the wear age factor (`set_analog_wear`). The effective `cal` is always their product. `set_thermal` only recomputes the bias. A diagnostic test file (`tests/brightness_audit.rs`) records which brightness each tier really plays and the effect on the sound. It runs once before the fix and once after. The two outputs are compared in a results file.

**Tech Stack:** Rust, `plugin303` crate (nih-plug). Tests run with `cargo test -p plugin303 --release`.

**Spec:** PR #72 remediation comment (https://github.com/gustavokch/vibe303/pull/72), section "Pre-existing issue". Defect summary:

- `LadderVcf::set_analog_wear` (`plugin303/src/dsp/ladder_wdf.rs:243`) sets `self.cal = CAL * bjt_age_factor` (`CAL = 1.0`). This replaces the tier brightness that `Voice` set with `set_cal`.
- High and Best: `Voice::new` applies wear to the circuit ladders, so they play at `cal = 1.0`, not `CIRCUIT_BRIGHT = 0.70`.
- Low: loses `LOW_BRIGHT = 0.50` the first time age, tolerance, seed or recalibration changes. In the plugin this happens on the first `process()` block: the `comp_seed` param defaults to `1337` (`params.rs:739-741`) but `Voice::new` starts at `comp_seed: 0` (`voice.rs:525`), so `update_params` sees a change and applies wear to every ladder. `Voice::reset` sets `comp_seed = -1`, so the wear is applied again after each host reset.
- `tests/best_spectral.rs` calls `set_circuit_brightness(0.70)` after the tier switch, so the gate measures a sound the plugin does not play.

## Global Constraints

- Work on branch `fix/ladder-brightness-wear` in the worktree `.worktrees/ladder-brightness-wear`. The branch starts from `refactor/quality-tier-names` at `6653f48` (PR #72 head). PR #72 changed the voice structure that this plan relies on: `ladder_low`, `ladder_high` and `ladder_best` are separate fields, and there is no ladder rebuild.
- Zero-alloc audio path. Nothing in this plan may allocate in `Voice::tick`, `Voice::update_params`, `Voice::set_tier`, `Voice::reset` or any `LadderVcf` setter. `tests/no_alloc.rs` must stay green.
- Always run Rust tests in release mode: `cargo test -p plugin303 --release ...`.
- Diagnostic tests use `#[ignore = "diagnostic: ..."]`. They are not gates.
- Do not loosen a gate budget (`LSD_BUDGET_DB`, `CORR_MIN`, the `±2.5 dB` level window, the `tier_calibration` budgets) without owner approval. If a gate fails and its decision rule in Task 3 does not cover the failure, stop and report the measured numbers.
- Do not edit `plugin303/src/dsp/ladder_wdf.rs.patched`, `docs/plans/**` or older `docs/superpowers/plans/**` files. They are historical records.
- Commit messages: Conventional Commits (`fix(ladder): ...`, `test(voice): ...`).
- This machine has `rtk`, which rewrites `cargo test` output. Prefix every `cargo test ... | grep/awk` pipeline in this plan with `rtk proxy` (for example `rtk proxy cargo test -p plugin303 --release ...`) so the raw output reaches `grep`.

## File Structure

| File | Change | Responsibility |
|---|---|---|
| `plugin303/src/dsp/ladder_wdf.rs` | Modify | `brightness` and `age_factor` fields; `update_bias()`; `cal()` getter; unit test |
| `plugin303/src/dsp/voice.rs` | Modify | `dbg_ladder_cals()` diagnostic getter |
| `plugin303/tests/common/mod.rs` | Modify | `apply_plugin_defaults(v, comp_age)`: the `update_params` call that the plugin makes, with default params |
| `plugin303/tests/brightness_audit.rs` | Create | Diagnostic: brightness state table, spectral table, level table, Low-vs-Best parity table |
| `plugin303/tests/ladder_brightness.rs` | Create | Gate: tier brightness survives plugin param updates, wear and reset |
| `plugin303/tests/best_spectral.rs` | Modify | Gate measures the voice's own brightness (no override) |
| `plugin303/src/dsp/voice.rs` (`LOW_LEVEL_TRIM`) | Maybe modify | Only if Task 3's level decision rule fires |
| `eval/303/results/2026-10-18-brightness-audit-before.txt` | Create | Raw audit output before the fix |
| `eval/303/results/2026-10-18-brightness-audit-after.txt` | Create | Raw audit output after the fix |
| `eval/303/results/2026-10-18-brightness-audit.md` | Create | Before/after comparison and verdict |
| `CHANGELOG.md` | Modify | `### Fixed` entry |

---

### Task 1: Measurement instruments and the "before" baseline

**Files:**
- Modify: `plugin303/src/dsp/ladder_wdf.rs:212-217` (add a getter next to `sample_rate`)
- Modify: `plugin303/src/dsp/voice.rs` (add a getter after `set_circuit_reso_gain`, about line 1660)
- Modify: `plugin303/tests/common/mod.rs` (append a helper)
- Create: `plugin303/tests/brightness_audit.rs`
- Create: `eval/303/results/2026-10-18-brightness-audit-before.txt`

**Interfaces:**
- Consumes: `Voice::{new, set_tier_immediate, set_low_brightness, set_circuit_brightness, update_params, reset, tick}`; `voice::{LOW_BRIGHT, CIRCUIT_BRIGHT}` (both `pub const f64`); `common::{gold_dir, load_f32, load_gold_op, spectral_match, bypassed_clipper}`.
- Produces:
  - `LadderVcf::cal(&self) -> f64`: effective brightness (diagnostic).
  - `Voice::dbg_ladder_cals(&self) -> [f64; 3]`: effective `cal` of `[ladder_low, ladder_high, ladder_best]` (`#[doc(hidden)]`).
  - `common::apply_plugin_defaults(v: &mut Voice, comp_age: i32)`: calls `update_params` with `Plugin303Params::default()` values, the same argument list as `Plugin303::process` (`lib.rs:287-321`), with `comp_age` overridden.

This task adds no fix, so there is no red/green cycle. The deliverable is the instrument and the recorded baseline.

- [ ] **Step 1: Add the `LadderVcf::cal` getter**

In `plugin303/src/dsp/ladder_wdf.rs`, directly after `pub fn sample_rate(&self) -> f64 { ... }`:

```rust
    /// Diagnostic: effective brightness on the cutoff→bias map (the value that
    /// sets the bias current in `set_cutoff`).
    pub fn cal(&self) -> f64 {
        self.cal
    }
```

- [ ] **Step 2: Add the `Voice::dbg_ladder_cals` getter**

In `plugin303/src/dsp/voice.rs`, directly after `pub fn set_circuit_reso_gain(...) { ... }`:

```rust
    /// Diagnostic: effective brightness of the Low, High and Best nodal ladders.
    #[doc(hidden)]
    pub fn dbg_ladder_cals(&self) -> [f64; 3] {
        [self.ladder_low.cal(), self.ladder_high.cal(), self.ladder_best.cal()]
    }
```

- [ ] **Step 3: Add `apply_plugin_defaults` to the shared test helpers**

Append to `plugin303/tests/common/mod.rs` (the file already has `#![allow(dead_code)]`):

```rust
/// Applies the default parameter set the way `Plugin303::process` does on every
/// block (`lib.rs` `update_params` call), with `comp_age` overridden. Tests that
/// skip this never hit the first-block re-wear (`comp_seed` 1337 vs `Voice::new`'s
/// 0) that resets the Low brightness.
pub fn apply_plugin_defaults(v: &mut plugin303::dsp::voice::Voice, comp_age: i32) {
    let p = plugin303::params::Plugin303Params::default();
    v.update_params(
        p.tuning.value(),
        p.osc1_pitch.value(),
        p.osc2_pitch.value(),
        p.osc2_vol.value(),
        p.waveform.value(),
        p.osc1_pw.value(),
        p.sub_vol.value(),
        p.cutoff.value(),
        p.resonance.value(),
        p.env_mod.value(),
        p.decay.value(),
        p.accent.value(),
        p.volume.value(),
        p.vcf_taps.value(),
        p.vcf_drive.value(),
        p.advanced_env.value(),
        p.meg_attack.value(),
        p.meg_decay_adj.value(),
        p.meg_sustain.value(),
        p.meg_release.value(),
        p.veg_attack.value(),
        p.veg_decay_adj.value(),
        p.veg_sustain.value(),
        p.veg_release.value(),
        p.vca_env_mode.value(),
        comp_age,
        p.comp_drift.value(),
        p.tolerance.value(),
        p.thermal_noise.value(),
        p.topology.value(),
        p.sweep_pos.value(),
        p.comp_seed.value(),
        p.glide.value(),
        p.reso_comp.value(),
        p.recalibration_scale.value() as f64,
    );
}
```

If a field name does not compile, copy the exact name from the `update_params` call in `plugin303/src/lib.rs` `fn process`. That call is the source of truth.

- [ ] **Step 4: Create the audit test file**

Create `plugin303/tests/brightness_audit.rs`:

```rust
//! Diagnostic: which VCF brightness each nodal tier really plays, and what it
//! does to the sound. Run before and after a brightness change and compare:
//!
//!   cargo test -p plugin303 --release --test brightness_audit -- \
//!       --ignored --nocapture --test-threads=1
//!
//! Brightness is the `cal` factor on the ladder's cutoff→bias map. Intended
//! values: Low `LOW_BRIGHT`, High/Best `CIRCUIT_BRIGHT`, each scaled by the
//! analog-wear age factor `1 - 0.15 * age/100 * recalibration_scale`.

mod common;

use common::{apply_plugin_defaults, gold_dir, load_f32, load_gold_op, spectral_match, GoldOp};
use plugin303::dsp::filter::Tier;
use plugin303::dsp::voice::{Voice, CIRCUIT_BRIGHT, LOW_BRIGHT};
use plugin303::params::VcaEnvMode;

const SR: f32 = 48_000.0;

fn print_cals(label: &str, v: &Voice) {
    let [low, high, best] = v.dbg_ladder_cals();
    println!("{label:<36} low {low:.4}  high {high:.4}  best {best:.4}");
}

/// Overrides the brightness of `tier`'s ladder. `None` keeps the voice's state.
fn override_brightness(v: &mut Voice, tier: Tier, bright: Option<f64>) {
    match (tier, bright) {
        (_, None) | (Tier::Draft, _) => {}
        (Tier::Low, Some(c)) => v.set_low_brightness(c),
        (Tier::High | Tier::Best, Some(c)) => v.set_circuit_brightness(c),
    }
}

#[test]
#[ignore = "diagnostic: effective ladder brightness per scenario"]
fn a_brightness_state() {
    println!("== A. Effective brightness (cal) ==");
    println!("intended (age 0)                     low {LOW_BRIGHT:.4}  high {CIRCUIT_BRIGHT:.4}  best {CIRCUIT_BRIGHT:.4}");
    let mut v = Voice::new(SR);
    print_cals("Voice::new", &v);
    for tier in [Tier::Low, Tier::High, Tier::Best] {
        v.set_tier_immediate(tier);
    }
    print_cals("after set_tier_immediate Low/High/Best", &v);
    apply_plugin_defaults(&mut v, 0);
    print_cals("plugin defaults (seed 1337, age 0)", &v);
    apply_plugin_defaults(&mut v, 50);
    print_cals("plugin defaults, age 50", &v);
    v.reset();
    apply_plugin_defaults(&mut v, 0);
    print_cals("reset + plugin defaults", &v);
}

/// Renders `tier` at the gold operating point the way the spectral gates do.
/// `plugin` first applies the plugin's default param update: the whole default
/// patch, including analog wear. `bright` then overrides only the brightness.
fn render_gold_op(tier: Tier, bright: Option<f64>, plugin: bool, op: &GoldOp, n: usize) -> Vec<f32> {
    let mut v = Voice::new(op.sr);
    v.update_clipper_params_immediate(common::bypassed_clipper());
    v.set_tier_immediate(tier);
    if plugin {
        apply_plugin_defaults(&mut v, 0);
    }
    override_brightness(&mut v, tier, bright);
    v.set_advanced_env(true);
    v.set_cutoff(op.cutoff);
    v.set_reso(op.reso);
    v.note_on(op.note);
    for _ in 0..48 {
        v.tick();
    }
    (0..n).map(|_| v.tick()).collect()
}

fn rms_db(x: &[f32]) -> f32 {
    let p: f64 = x.iter().map(|&s| (s as f64) * (s as f64)).sum::<f64>() / x.len() as f64;
    10.0 * p.max(1e-30).log10() as f32
}

#[test]
#[ignore = "diagnostic: spectral match vs SPICE gold per tier and brightness"]
fn b_spectral_vs_gold() {
    println!("== B. Spectral match vs gold voice_A2_mid (LSD dB, corr, RMS dBFS) ==");
    let gold = load_f32(&gold_dir().join("voice_A2_mid.f32"));
    let op = load_gold_op("voice_A2_mid.json");
    let rows: [(Tier, &str, Option<f64>, bool); 18] = [
        (Tier::Low, "voice state", None, false),
        (Tier::Low, "plugin defaults", None, true),
        (Tier::Low, "plugin defaults, cal 0.50", Some(0.50), true),
        (Tier::Low, "plugin defaults, cal 1.00", Some(1.00), true),
        (Tier::Low, "cal 0.50", Some(0.50), false),
        (Tier::Low, "cal 1.00", Some(1.00), false),
        (Tier::High, "voice state", None, false),
        (Tier::High, "plugin defaults", None, true),
        (Tier::High, "plugin defaults, cal 0.70", Some(0.70), true),
        (Tier::High, "plugin defaults, cal 1.00", Some(1.00), true),
        (Tier::High, "cal 0.70", Some(0.70), false),
        (Tier::High, "cal 1.00", Some(1.00), false),
        (Tier::Best, "voice state", None, false),
        (Tier::Best, "plugin defaults", None, true),
        (Tier::Best, "plugin defaults, cal 0.70", Some(0.70), true),
        (Tier::Best, "plugin defaults, cal 1.00", Some(1.00), true),
        (Tier::Best, "cal 0.70", Some(0.70), false),
        (Tier::Best, "cal 1.00", Some(1.00), false),
    ];
    for (tier, label, bright, plugin) in rows {
        let m = render_gold_op(tier, bright, plugin, &op, gold.len());
        let (lsd, corr) = spectral_match(&gold, &m, op.sr);
        println!(
            "{:<5} {label:<16} LSD {lsd:6.2}  corr {corr:.3}  rms {:6.1}",
            format!("{tier:?}"),
            rms_db(&m)
        );
    }
}

/// Gate-mode RMS of one tier, as `tests/tier_levels.rs` measures it.
fn tier_rms(tier: Tier, bright: Option<f64>, note: u8, cutoff: f32, reso: f32) -> f32 {
    let mut v = Voice::new(SR);
    v.set_tier_immediate(tier);
    override_brightness(&mut v, tier, bright);
    v.set_vca_env_mode(VcaEnvMode::Gate);
    v.set_cutoff(cutoff);
    v.set_reso(reso);
    v.set_gate(true, true, false, false);
    v.note_on(note);
    for _ in 0..400 {
        v.tick();
    }
    let n = 8_000;
    let acc: f64 = (0..n).map(|_| (v.tick() as f64).powi(2)).sum();
    (acc / n as f64).sqrt() as f32
}

/// (label, Low brightness, Best brightness). `None` = voice state.
const COMBOS: [(&str, Option<f64>, Option<f64>); 4] = [
    ("voice state", None, None),
    ("plugin today (1.00/1.00)", Some(1.00), Some(1.00)),
    ("tests today (0.50/1.00)", Some(0.50), Some(1.00)),
    ("intended (0.50/0.70)", Some(0.50), Some(0.70)),
];

#[test]
#[ignore = "diagnostic: Low vs Best level at the tier_levels patches"]
fn c_low_vs_best_level() {
    println!("== C. Low vs Best level, dB (tier_levels::low_tracks_best_level window ±2.5) ==");
    let patches = [(45u8, 1_500.0f32, 0.3f32), (33, 800.0, 0.5), (57, 3_000.0, 0.2), (57, 1_002.0, 0.85)];
    for (label, low_c, best_c) in COMBOS {
        let dbs: Vec<f32> = patches
            .iter()
            .map(|&(note, fc, r)| {
                20.0 * (tier_rms(Tier::Low, low_c, note, fc, r) / tier_rms(Tier::Best, best_c, note, fc, r)).log10()
            })
            .collect();
        let mean = dbs.iter().sum::<f32>() / dbs.len() as f32;
        println!("{label:<26} {:?}  mean {mean:+.2}", dbs.iter().map(|d| format!("{d:+.2}")).collect::<Vec<_>>());
    }
}

/// Envelope sweep, as `tests/tier_calibration.rs::render_sweep` renders it.
fn render_sweep(tier: Tier, bright: Option<f64>, n: usize) -> Vec<f32> {
    let mut v = Voice::new(SR);
    v.update_clipper_params_immediate(common::bypassed_clipper());
    v.set_tier_immediate(tier);
    override_brightness(&mut v, tier, bright);
    v.set_vca_env_mode(VcaEnvMode::Gate);
    v.set_cutoff(200.0 * 20.0f32.powf(0.4));
    v.set_reso(0.65);
    v.set_env_mod(0.7);
    v.note_on(57);
    v.set_gate(true, true, false, false);
    for _ in 0..48 {
        v.tick();
    }
    (0..n).map(|_| v.tick()).collect()
}

#[test]
#[ignore = "diagnostic: Low and Draft vs Best parity (tier_calibration sweep)"]
fn d_parity_vs_best() {
    println!("== D. Parity vs Best, envelope sweep (budgets: Low LSD<=17 corr>=0.975; Draft LSD<=42 corr>=0.94) ==");
    for (label, low_c, best_c) in COMBOS {
        let best = render_sweep(Tier::Best, best_c, 48_000);
        let low = render_sweep(Tier::Low, low_c, 48_000);
        let draft = render_sweep(Tier::Draft, None, 48_000);
        let (ll, lc) = spectral_match(&best, &low, SR);
        let (dl, dc) = spectral_match(&best, &draft, SR);
        println!("{label:<26} Low LSD {ll:6.2} corr {lc:.3} | Draft LSD {dl:6.2} corr {dc:.3}");
    }
}
```

- [ ] **Step 5: Run the audit and save the "before" output**

```bash
cd .worktrees/ladder-brightness-wear
cargo test -p plugin303 --release --test brightness_audit -- --ignored --nocapture --test-threads=1 2>&1 \
  | grep -E "^(test [A-Za-z_]+ \.\.\. )?(==|intended|Voice|after|plugin|reset|Low|High|Best|Draft|voice|tests|cal )" \
  > eval/303/results/2026-10-18-brightness-audit-before.txt
cat eval/303/results/2026-10-18-brightness-audit-before.txt
```

Expected section A (this confirms the defect):

```
intended (age 0)                     low 0.5000  high 0.7000  best 0.7000
Voice::new                           low 0.5000  high 1.0000  best 1.0000
after set_tier_immediate Low/High/Best low 0.5000  high 1.0000  best 1.0000
plugin defaults (seed 1337, age 0)   low 1.0000  high 1.0000  best 1.0000
plugin defaults, age 50              low 0.9250  high 0.9250  best 0.9250
reset + plugin defaults              low 1.0000  high 1.0000  best 1.0000
```

If section A does not match, stop: the defect is not the one this plan fixes. Sections B-D have no expected values yet. They are the baseline.

- [ ] **Step 6: Save the "before" render fingerprints and gate numbers**

```bash
cargo build --release -q -p plugin303 --bin render303
for t in draft low high best; do for w in 0 1; do
  ./target/release/render303 --note 57 --sr 48000 --secs 0.3 --cutoff 1000 --reso 85 \
    --wave $w --tier $t --out /tmp/bright_before_${t}_$w.f32
done; done
shasum /tmp/bright_before_*.f32 >> eval/303/results/2026-10-18-brightness-audit-before.txt
cargo test -p plugin303 --release --test best_spectral --test low_spectral --test tier_levels \
  --test tier_calibration -- --nocapture 2>&1 \
  | grep -E "spectral match|vs Best|LSD =|test result" >> eval/303/results/2026-10-18-brightness-audit-before.txt
```

Expected: all four gate files pass (`test result: ok`).

- [ ] **Step 7: Commit**

```bash
git add plugin303/src/dsp/ladder_wdf.rs plugin303/src/dsp/voice.rs plugin303/tests/common/mod.rs \
  plugin303/tests/brightness_audit.rs eval/303/results/2026-10-18-brightness-audit-before.txt \
  docs/superpowers/plans/2026-10-18-ladder-brightness-wear.md
git commit -m "test(voice): add VCF brightness audit and record pre-fix baseline"
```

---

### Task 2: Analog wear scales the tier brightness

**Files:**
- Modify: `plugin303/src/dsp/ladder_wdf.rs:123-126` (field docs), `:166-189` (`new`), `:205-210` (`set_cal`), `:240-244` (`set_analog_wear`), `:253-258` (`set_thermal`); unit test in `mod tests` (starts at line 523)
- Create: `plugin303/tests/ladder_brightness.rs`

**Interfaces:**
- Consumes: `LadderVcf::cal()`, `Voice::dbg_ladder_cals()`, `common::apply_plugin_defaults` (Task 1).
- Produces: `LadderVcf::set_cal(cal)` now sets the **tier brightness**. The effective `cal()` is `brightness × age_factor`. `set_analog_wear` sets only `age_factor` (and the cap multipliers). `set_thermal` keeps `cal()`. Public signatures do not change.

- [ ] **Step 1: Write the failing ladder unit test**

Append inside `mod tests` in `plugin303/src/dsp/ladder_wdf.rs` (the block that starts with `fn ac_gain`):

```rust
    /// Analog wear scales the tier brightness set by `set_cal`; it must not
    /// replace it. Order of the two calls must not matter, and a thermal
    /// update must not change the brightness.
    #[test]
    fn analog_wear_scales_brightness_instead_of_replacing_it() {
        let age_50 = 1.0 - 0.15 * 0.5; // bjt_age_factor at age 50, full recalibration
        let mut f = LadderVcf::new(192_000.0);
        f.set_cal(0.70);
        f.set_analog_wear(0.0, 50, 1, 1.0);
        assert!((f.cal() - 0.70 * age_50).abs() < 1e-12, "wear after brightness: {}", f.cal());

        f.set_thermal(20);
        assert!((f.cal() - 0.70 * age_50).abs() < 1e-12, "thermal changed brightness: {}", f.cal());

        f.set_cal(0.50);
        assert!((f.cal() - 0.50 * age_50).abs() < 1e-12, "brightness after wear: {}", f.cal());

        f.set_analog_wear(0.0, 0, 1, 1.0);
        assert!((f.cal() - 0.50).abs() < 1e-12, "age 0 must give back the brightness: {}", f.cal());
    }
```

- [ ] **Step 2: Write the failing voice-level test**

Create `plugin303/tests/ladder_brightness.rs`:

```rust
//! Each nodal tier must play its calibrated VCF brightness (Low `LOW_BRIGHT`,
//! High/Best `CIRCUIT_BRIGHT`), scaled only by the analog-wear age factor. The
//! plugin applies analog wear on its first block and after every host reset,
//! so this must hold through those updates, not only on a fresh `Voice`.

mod common;

use common::apply_plugin_defaults;
use plugin303::dsp::filter::Tier;
use plugin303::dsp::voice::{Voice, CIRCUIT_BRIGHT, LOW_BRIGHT};

fn assert_cals(v: &Voice, age_factor: f64, when: &str) {
    let expected = [LOW_BRIGHT, CIRCUIT_BRIGHT, CIRCUIT_BRIGHT].map(|c| c * age_factor);
    let actual = v.dbg_ladder_cals();
    for ((name, e), a) in ["low", "high", "best"].iter().zip(expected).zip(actual) {
        assert!((a - e).abs() < 1e-12, "{when}: {name} ladder cal {a:.4}, expected {e:.4}");
    }
}

#[test]
fn tier_brightness_survives_plugin_param_updates() {
    let mut v = Voice::new(48_000.0);
    assert_cals(&v, 1.0, "Voice::new");

    for tier in [Tier::Low, Tier::High, Tier::Best, Tier::Draft] {
        v.set_tier_immediate(tier);
    }
    assert_cals(&v, 1.0, "after tier changes");

    apply_plugin_defaults(&mut v, 0);
    assert_cals(&v, 1.0, "first plugin block (age 0)");

    apply_plugin_defaults(&mut v, 50);
    assert_cals(&v, 1.0 - 0.15 * 0.5, "age 50");

    v.reset();
    apply_plugin_defaults(&mut v, 0);
    assert_cals(&v, 1.0, "host reset + first block");
}
```

- [ ] **Step 3: Run both tests to verify they fail**

```bash
cargo test -p plugin303 --release --lib analog_wear_scales_brightness
cargo test -p plugin303 --release --test ladder_brightness
```

Expected: both FAIL. The unit test fails with `wear after brightness: 0.925`. The voice test fails with `Voice::new: high ladder cal 1.0000, expected 0.7000`.

- [ ] **Step 4: Implement the fix in `LadderVcf`**

In `plugin303/src/dsp/ladder_wdf.rs`:

(a) Replace the `cal` field and its doc (lines 123-126) with:

```rust
    /// Tier brightness on the cutoff→bias map (spectral V5), set by `set_cal`.
    /// Default 1.0 keeps the F3.3 ladder tests in their tracking regime; the
    /// voice sets a per-tier value (`LOW_BRIGHT`, `CIRCUIT_BRIGHT`).
    brightness: f64,
    /// Analog-wear BJT age factor (1.0 = new), set by `set_analog_wear`.
    age_factor: f64,
    /// Effective brightness `brightness * age_factor`; sets the bias current.
    cal: f64,
```

(b) In `new`, replace `cal: CAL,` with:

```rust
            brightness: CAL,
            age_factor: 1.0,
            cal: CAL,
```

(c) Replace `set_cal` (lines 205-210) with:

```rust
    /// Tier brightness on the cutoff→bias map (spectral V5). Analog wear scales
    /// it (see `set_analog_wear`). Recomputes the bias at the current cutoff.
    pub fn set_cal(&mut self, cal: f64) {
        self.brightness = cal;
        self.update_bias();
    }

    /// Recomputes the effective brightness and the bias current at the current
    /// cutoff and diode temperature.
    fn update_bias(&mut self) {
        self.cal = self.brightness * self.age_factor;
        self.i0 = self.diode.n_vt * TAU * self.fc * C_REF * self.cal;
    }
```

(d) In `set_analog_wear`, replace

```rust
        self.cal = CAL * bjt_age_factor;
        self.set_cal(self.cal);
```

with

```rust
        self.age_factor = bjt_age_factor;
        self.update_bias();
```

(e) In `set_thermal`, replace `self.set_cal(self.cal); // Recalculate i0` with:

```rust
        self.update_bias(); // diode n_vt changed with temperature
```

`set_cutoff` (line 473-477) still reads `self.cal`, which is now the effective value. Do not change it.

- [ ] **Step 5: Run both tests to verify they pass**

```bash
cargo test -p plugin303 --release --lib analog_wear_scales_brightness
cargo test -p plugin303 --release --test ladder_brightness --test no_alloc
```

Expected: PASS for all.

- [ ] **Step 6: Commit**

```bash
git add plugin303/src/dsp/ladder_wdf.rs plugin303/tests/ladder_brightness.rs
git commit -m "fix(ladder): analog wear scales the tier brightness instead of replacing it

set_analog_wear wrote cal = 1.0 * age_factor, so High/Best played at 1.0
instead of CIRCUIT_BRIGHT (0.70), and Low lost LOW_BRIGHT (0.50) on the
plugin's first block. The ladder now keeps brightness and age factor apart and
uses their product."
```

---

### Task 3: Gates measure the voice the plugin plays

**Files:**
- Modify: `plugin303/tests/best_spectral.rs:23-36` (`render_best`), `:63-86` (`best_voice_spectral_match`), `:38-60` (`brightness_sweep` call site)
- Maybe modify: `plugin303/src/dsp/voice.rs` `LOW_LEVEL_TRIM` (about line 40)

**Interfaces:**
- Consumes: the fixed `LadderVcf` (Task 2).
- Produces: `render_best(sr, cutoff, reso, note, n, cal: Option<f64>)`. `None` uses the voice's own brightness.

- [ ] **Step 1: Make the Best gate render without a brightness override**

In `plugin303/tests/best_spectral.rs`, change `render_best` to take `cal: Option<f64>`:

```rust
fn render_best(sr: f32, cutoff: f32, reso: f32, note: u8, n: usize, cal: Option<f64>) -> Vec<f32> {
    let mut v = Voice::new(sr);
    v.update_clipper_params_immediate(common::bypassed_clipper());
    v.set_tier_immediate(Tier::Best);
    v.set_advanced_env(true);
    if let Some(cal) = cal {
        v.set_circuit_brightness(cal);
    }
    v.set_cutoff(cutoff);
    v.set_reso(reso);
    v.note_on(note);
    for _ in 0..48 {
        v.tick();
    }
    (0..n).map(|_| v.tick()).collect()
}
```

In `brightness_sweep`, change the call to `render_best(op.sr, op.cutoff, op.reso, op.note, gold.len(), Some(cal))`.

In `best_voice_spectral_match`, change the last argument from `BEST_BRIGHTNESS` to `None`. Delete the `BEST_BRIGHTNESS` const and replace the comment above `LSD_BUDGET_DB` with:

```rust
// F6.5: active Q9 resonance (F6.3) + re-tuned brightness (CIRCUIT_BRIGHT = 0.70)
// dropped the gold-power-weighted LSD from 4.42 → 3.87 dB. The gate renders
// the voice's own brightness, so it measures what the plugin plays.
```

- [ ] **Step 2: Run the gate**

```bash
cargo test -p plugin303 --release --test best_spectral -- --nocapture
```

Expected: PASS with `Best spectral match: LSD = 3.87 dB, corr = 0.979` (the same as the pre-fix gate, because it used to force 0.70 and the voice now plays 0.70). If LSD differs from the "before" gate number by more than 0.05 dB, stop: a second brightness path exists.

- [ ] **Step 3: Run the full suite**

```bash
cargo test -p plugin303 --release --no-fail-fast 2>&1 | grep -E "^test result|FAILED|panicked|LSD|vs Best"
```

Expected: no failures. High/Best now play darker (0.70 instead of 1.0). The gates most likely to move are `tier_levels::low_tracks_best_level`, `tier_calibration::test_voicing_parity_sweeps`, `tier_switch` and `tier_alignment`. Use the rules in Step 4 for any failure.

- [ ] **Step 4: Apply the decision rules for failing gates**

Compare each failure with the audit row "intended (0.50/0.70)" from the "before" file (sections C and D predict the post-fix numbers).

- `tier_levels::low_tracks_best_level` out of `±2.5 dB`: `LOW_LEVEL_TRIM` equalises Low's level to Best, and Best's level moved. Re-trim from the "intended" mean of section C: `LOW_LEVEL_TRIM_new = LOW_LEVEL_TRIM_old * 10^(-mean_dB / 20)`. Example: mean `+1.80 dB` and old trim `1.55` give `1.55 * 10^(-0.09) = 1.260`. Update the constant and its `// +x.x dB` comment (`20*log10(trim)`), then rerun `cargo test -p plugin303 --release --test tier_levels --test tier_switch`. If the four patches spread more than 5 dB, a single trim cannot fix them. Stop and report section C.
- `tier_calibration::test_voicing_parity_sweeps` over budget: do not change the budget. The Draft/Low polynomials were fitted against Best at `CIRCUIT_BRIGHT` (`vcf_sweep_helper` uses it), so the fix should make parity better, not worse. Stop and report section D before and after.
- `tier_switch` / `tier_alignment` failure: High and Best change in the same way, so a failure is not expected. Stop and report the assertion message.
- Any other failure: stop and report.

- [ ] **Step 5: Commit**

```bash
git add plugin303/tests/best_spectral.rs plugin303/src/dsp/voice.rs
git commit -m "test(best): spectral gate renders the voice's own brightness"
```

If `LOW_LEVEL_TRIM` changed, add to the commit body: `Re-trim LOW_LEVEL_TRIM <old> -> <new>: Best is darker at CIRCUIT_BRIGHT; mean Low-vs-Best level was <x> dB.`

---

### Task 4: Measure after the fix and compare

**Files:**
- Create: `eval/303/results/2026-10-18-brightness-audit-after.txt`
- Create: `eval/303/results/2026-10-18-brightness-audit.md`
- Modify: `CHANGELOG.md` (`## [0.1.0] - Unreleased` → `### Fixed`)

**Interfaces:**
- Consumes: the audit (Task 1), the fix (Task 2), the gate changes (Task 3).
- Produces: the comparison report. Nothing in code depends on it.

- [ ] **Step 1: Run the audit again and save the "after" output**

```bash
cargo test -p plugin303 --release --test brightness_audit -- --ignored --nocapture --test-threads=1 2>&1 \
  | grep -E "^(test [A-Za-z_]+ \.\.\. )?(==|intended|Voice|after|plugin|reset|Low|High|Best|Draft|voice|tests|cal )" \
  > eval/303/results/2026-10-18-brightness-audit-after.txt
```

Expected section A:

```
intended (age 0)                     low 0.5000  high 0.7000  best 0.7000
Voice::new                           low 0.5000  high 0.7000  best 0.7000
after set_tier_immediate Low/High/Best low 0.5000  high 0.7000  best 0.7000
plugin defaults (seed 1337, age 0)   low 0.5000  high 0.7000  best 0.7000
plugin defaults, age 50              low 0.4625  high 0.6475  best 0.6475
reset + plugin defaults              low 0.5000  high 0.7000  best 0.7000
```

Expected section B: the "voice state" row of each tier equals its intended-brightness "cal" row (Low = "cal 0.50", High/Best = "cal 0.70"). The "plugin defaults" row of each tier equals its "plugin defaults, cal <intended>" control row. The explicit-brightness rows ("cal X" and "plugin defaults, cal X") are identical to the "before" file, which shows the instrument did not change.

- [ ] **Step 2: Check the render fingerprints**

```bash
for t in draft low high best; do for w in 0 1; do
  ./target/release/render303 --note 57 --sr 48000 --secs 0.3 --cutoff 1000 --reso 85 \
    --wave $w --tier $t --out /tmp/bright_after_${t}_$w.f32
  cmp -s /tmp/bright_after_${t}_$w.f32 /tmp/bright_before_${t}_$w.f32 \
    && echo "$t/$w identical" || echo "$t/$w changed"
done; done | tee -a eval/303/results/2026-10-18-brightness-audit-after.txt
```

Run `cargo build --release -q -p plugin303 --bin render303` first. Expected: `draft` and `low` identical (`render303` passes `comp_seed 0`, so it never applies wear to Low, and Low stays at 0.50 before and after). `high` and `best` changed (1.0 → 0.70). If `/tmp/bright_before_*` is gone, compare against the `shasum` lines in the "before" file.

- [ ] **Step 3: Write the comparison report**

Create `eval/303/results/2026-10-18-brightness-audit.md` with this structure. Fill every cell from the two `.txt` files; leave no cell empty.

```markdown
# VCF brightness vs analog wear: before/after audit (2026-10-18)

Fix: `LadderVcf::set_analog_wear` scales the tier brightness instead of replacing it
(branch `fix/ladder-brightness-wear`). Raw output: `2026-10-18-brightness-audit-before.txt`,
`2026-10-18-brightness-audit-after.txt`. Command:
`cargo test -p plugin303 --release --test brightness_audit -- --ignored --nocapture --test-threads=1`.

## Effective brightness the plugin plays (first block, age 0)

| Tier | Intended | Before | After |
|---|---|---|---|
| Low | 0.50 | | |
| High | 0.70 | | |
| Best | 0.70 | | |

## Spectral match vs SPICE gold (voice state row, section B)

The "voice state" row changes brightness only, so it is the row compared here. The "plugin defaults" row is the full default patch (analog wear and all other default params); compare it with the "plugin defaults, cal <intended>" control row, not with the gold directly.

| Tier | LSD before | LSD after | corr before | corr after | RMS before | RMS after |
|---|---|---|---|---|---|---|
| Low | | | | | | |
| High | | | | | | |
| Best | | | | | | |

## Low vs Best level (section C, voice state row) and parity (section D, voice state row)

| Metric | Before | After | Gate |
|---|---|---|---|
| Low−Best level, mean dB | | | ±2.5 dB per patch |
| Low vs Best LSD / corr | | | ≤17.0 / ≥0.975 |
| Draft vs Best LSD / corr | | | ≤42.0 / ≥0.94 |

## Gates

| Gate | Before | After |
|---|---|---|
| best_spectral LSD / corr | | |
| low_spectral LSD / corr | | |
| full suite (passed / failed / ignored) | | |

## Verdict

<2-4 sentences: does the plugin now play the intended brightness; how far did
High/Best move toward the gold (LSD delta); was LOW_LEVEL_TRIM re-trimmed and to what.>
```

- [ ] **Step 4: Add the CHANGELOG entry**

Under `## [0.1.0] - Unreleased` → `### Fixed` in `CHANGELOG.md`, append (replace `<...>` with numbers from the report):

```markdown
- The High and Best tiers played their VCF at brightness 1.0 instead of the
  calibrated 0.70, and the Low tier lost its 0.50 brightness on the plugin's
  first block: analog wear (`LadderVcf::set_analog_wear`) replaced the tier
  brightness instead of scaling it. Wear now scales it. Best moves from
  <LSD before> to <LSD after> dB LSD against the SPICE gold. High and Best
  sound darker than before; sessions keep their settings.
```

- [ ] **Step 5: Final full suite**

```bash
cargo test -p plugin303 --release --no-fail-fast 2>&1 | grep -E "^test result" \
  | awk '{p+=$4; f+=$6; i+=$8} END {print "passed",p,"failed",f,"ignored",i}'
```

Expected: `failed 0`. Passed = the PR #72 count (228) + 2 new tests (`analog_wear_scales_brightness_instead_of_replacing_it`, `tier_brightness_survives_plugin_param_updates`) = 230. Ignored = 16 + 4 audit diagnostics = 20. Put the line into the report's "Gates" table.

- [ ] **Step 6: Commit**

```bash
git add eval/303/results/2026-10-18-brightness-audit-after.txt \
  eval/303/results/2026-10-18-brightness-audit.md CHANGELOG.md
git commit -m "docs(eval): before/after VCF brightness audit"
```

- [ ] **Step 7: Push and open the PR (stacked on #72)**

```bash
git push -u origin fix/ladder-brightness-wear
gh pr create --base refactor/quality-tier-names --title "fix(ladder): analog wear scales tier brightness" \
  --body-file eval/303/results/2026-10-18-brightness-audit.md
```

The body is the comparison report. Add one line at the top: `Stacked on #72. Retarget to main after #72 merges. Changes the High/Best sound (darker, closer to the SPICE gold).`
