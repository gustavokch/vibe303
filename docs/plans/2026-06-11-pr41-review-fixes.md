# Plan: PR #41 review fixes

Date: 2026-06-11
Branch: `feat/trafo-rk4-core`
Source: self-review of PR #41 (the PR #40 trafo-core follow-up).

PR #41 reviewed **APPROVE**. The three substantive changes (P1 reseed + sign-preserving
clamp, P2 dead-snapshot removal, P3 docs) are all correct/safe — verified against the code,
not just the description. Only two non-blocking items surfaced. Both are `--features
trafo-core`-only; default builds are untouched.

## Issues

| # | Sev | Item | File |
|---|-----|------|------|
| R1 | LOW · cleanup | Reseed block duplicated verbatim in two sites | `wdf.rs:2087`, `wdf.rs:3578` |
| R2 | INFO · test | Reseed regression test is one-sided (pins state, not the click) | `tests/test_trafo.rs` |

## R1 — extract `ja_reseed()` helper

The six-line "clean origin" reseed (`ja_m/ja_h_prev/ja_phi_prev/ja_vm_prev = 0`,
`ja_delta = 1.0`, `ja_i_dev = 0`) is copy-pasted in `set_trafo` (inactive→active edge) and
`process_host_sample` (warmup→audio edge). A future `ja_*` field added to one site but not the
other would desync the two "clean core" definitions silently.

**Fix:** add a single feature-gated method and call it from both.

```rust
/// Reset the JA core to a clean origin (M=0, no flux/field/voltage history, +δ, no lagged
/// deviation). Called on both the warmup→audio and the inactive→active (knob re-enable) edges
/// so the core never integrates the next trapezoidal step against a stale operating point.
#[cfg(feature = "trafo-core")]
#[inline]
fn ja_reseed(&mut self) {
    self.ja_m = 0.0;
    self.ja_h_prev = 0.0;
    self.ja_phi_prev = 0.0;
    self.ja_vm_prev = 0.0;
    self.ja_delta = 1.0;
    self.ja_i_dev = 0.0;
}
```

- `set_trafo`: replace the 6-line block with `self.ja_reseed();`.
- `process_host_sample`: same.
- Keep the existing explanatory comments at each call site (they document *why* the edge
  reseeds; the helper documents *what* a clean core is).

**Verify:** `cargo test --features trafo-core` (the reseed test + bit-exact-at-0 unchanged);
`cargo check` both feature states; confirm `test_wdf_noalloc::set_trafo` still alloc-free
(helper is `#[inline]`, no allocation).

## R2 — tighten the reseed regression test (optional)

`trafo_reenable_reseeds_core_no_flux_jump` asserts `(H,B) == (0,0)` post-edge + bounded output.
It does not bound the *discontinuity* itself, so a hypothetical future change that reseeds to a
clean origin yet still steps the flux audibly would pass.

**Fix (optional):** capture the linear output level just before the re-enable edge and assert the
first post-re-enable sample doesn't jump more than a small bound above it. Low value (the `(0,0)`
assertion already pins the root cause); do only if R1 lands and the test is being touched anyway.

## Sequencing

R1 is a 1-commit mechanical refactor with no behavior change — safe to land directly on
`feat/trafo-rk4-core` (or fold into PR #41 if still open). R2 is optional polish; bundle with R1
or skip. Neither blocks merge.

## Resolution (2026-06-11) — both addressed

- **R1 done.** Extracted `fn ja_reseed(&mut self)` (feature-gated, `#[inline]`); `set_trafo` and
  `process_host_sample` both call it. Single source of truth for the clean origin.
- **R2 done — and it caught a real gap.** The first attempt (bound the output jump at the
  re-enable edge vs the linear path) was wrong: at `os>1` only the first *subsample* injects zero
  deviation, so the decimated host output legitimately carries nonlinearity rebuilt over later
  subsamples — the bound fired on legitimate signal, not a click. More importantly, it surfaced
  that the original `(H,B)==(0,0)` assertion proves `ja_m`/`ja_phi_prev` reseeded but **not**
  `ja_i_dev` — which is the actual click source (the lagged current injected on N_PA's RHS).
  Added a feature-gated `ja_i_dev()` test accessor and now assert the hot drive leaves it non-zero,
  then the re-enable zeros it. Directly tests the click-fix mechanism, no fragile output threshold.

Verify: `cargo test --features trafo-core` (test_trafo 7 pass, test_wdf_noalloc 4 pass incl.
alloc-free `set_trafo`), `cargo check` no-feature clean.
