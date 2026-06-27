# Plan — PR #2 review follow-ups

Addresses the findings from the M1 walking-skeleton review (PR #2). All findings
were minor; nothing blocks the M1 merge. This plan splits them into **quick wins
to land now** and **tracking items deferred to their planned milestones**.

## Quick wins (land in this branch or a small follow-up)

### 1. Remove the unused `nih_plug_egui` dependency
- **Why:** No `editor()`/egui reference exists in `plugin303/src/`. The dep pulls
  `ab_glyph`, `owned_ttf_parser`, font/render crates — most of the +2998-line
  `Cargo.lock` churn and extra build time, for zero current use.
- **How:**
  - Delete the `nih_plug_egui = { … }` line from `plugin303/Cargo.toml`.
  - `cargo update -p plugin303` / rebuild so `Cargo.lock` drops the egui subtree.
  - Re-add it (ideally behind a `gui` feature) when the GUI milestone starts.
- **Verify:** `cargo build -p plugin303` clean; `Cargo.lock` diff shrinks
  substantially; `cargo test -p plugin303` still 10/10.

### 2. Derive `Default` for `Plugin303Params`
- **Why:** Manual `impl Default for Plugin303Params {}` on an empty struct trips
  clippy `derivable_impls` (the warning called out in the PR body).
- **How:** Replace the manual impl with `#[derive(Params, Default)]` on the struct;
  drop the `impl Default` block in `plugin303/src/params.rs`.
- **Verify:** `cargo clippy -p plugin303` no longer emits the `derivable_impls`
  warning.

## Tracking items (deferred — already on the roadmap)

### 3. `NoteOff` + velocity handling
- **Status:** `process()` matches only `NoteOn { note, .. }`; velocity dropped, no
  gate-off. AD env auto-decays so a held note is audible and roughly matches the
  303 decay-env behavior — acceptable for M1.
- **Plan:** Fold into **M4/M6** (accent, glide, envelopes). When the gated/ADSR
  envelope lands, wire `NoteEvent::NoteOff` to release and thread velocity into
  accent/VCA. No change now beyond a code comment noting the deferral.

### 4. Broaden the alloc gate to the real RT path
- **Status:** `no_alloc.rs` guards `voice.tick` only; `process()` (event drain +
  `buffer.iter_samples()`) is not covered.
- **Plan:** When a host/buffer test harness exists (or via a nih-plug test buffer),
  wrap a synthetic `process()` block in `assert_no_alloc`. Track as a small DSP-test
  task; low priority since the per-sample core is already gated and the wrapper
  layers are nih-plug-owned.

## Sequencing
- Items **1** and **2** are independent, ~5 min each → do both now, one commit
  (`chore(303): drop unused egui dep + derive params Default`).
- Items **3** and **4** require no code change now — leave a one-line `// deferred:`
  comment at each site referencing the milestone, so the intent is greppable.

## Acceptance
- `cargo test -p plugin303` → 10/10.
- `cargo clippy -p plugin303` → no `derivable_impls` warning.
- `Cargo.lock` egui subtree removed.
