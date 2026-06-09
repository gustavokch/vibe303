# PR #34 (S4D/S6 SSM) — review follow-ups

Plan to address issues surfaced in the PR #34 review. All are **non-blocking**;
none require holding the merge. Ordered by value. Items 1, 2, 4 are worth doing
on this branch; 3, 5, 6, 7 are cheap polish / optional.

Branch: `feat/ssm-models`. Verify each change with
`uv run pytest tests/train tests/eval -q` + `uv run ruff check`.

---

## 1. Checkpoint architecture metadata (robustness) — DO

**Problem:** `_atomic_save(model.state_dict())` saves a bare tensor dict. The eval
harness must be hand-fed `model_type/hidden/d_state/d_conv/num_layers/head/film_kind`.
A wrong combo is only caught if `load_state_dict` happens to hit a shape/key
mismatch; a same-shape-different-meaning combo mis-scores silently. More knobs
now → higher chance of a quiet mismatch.

**Fix:**
- In `train/train.py`, alongside `best.pth`, write a sibling `arch.json` capturing
  the construction kwargs (`model_type`, `hidden`, `num_layers`, `d_state`,
  `d_conv`, `expand`, `head`, `film_kind`, `conditioning`).
- In `eval/ablation/run_checkpoint_eval.py::load_model`, if an `arch.json` sits
  next to the checkpoint, load it and **cross-check** against the passed kwargs;
  raise a clear `ValueError` on mismatch instead of leaning on `load_state_dict`.
  Keep the explicit-kwargs path working when no sidecar exists (back-compat with
  existing LSTM checkpoints).
- Decide: hard-require match, or warn-and-prefer-sidecar. Recommend hard-require
  with an `--ignore-arch-meta` escape hatch.

**Test:** new test in `tests/eval` — save a tiny SSM checkpoint + arch.json, assert
`load_model` with a mismatched `d_state` raises; with matching kwargs loads.

**Effort:** ~M (touches save + load + one test).

---

## 2. Dropout placement matches LSTM (fidelity) — DO

**Problem:** `BA283SSM.forward` applies `self.dropout` after *every* layer
including the last, so the final representation feeding FiLM/readout is dropped.
`nn.LSTM` applies dropout *between* layers only, never after the last — and this
SSM is A/B'd directly against that LSTM.

**Fix (pick one, document the choice):**
- (preferred) In `train/ssm.py::BA283SSM.forward`, skip dropout after the final
  layer — apply only when `i < num_layers - 1`. Mirrors `nn.LSTM` exactly.
- or: leave as-is but add a comment stating the intentional divergence.

**Test:** extend `tests/train/test_ssm.py` — with `dropout>0` and `model.train()`,
two forward passes differ (dropout active); with `num_layers=1` + `dropout>0`,
output in eval matches a `dropout=0` model (no dangling final-layer drop).

**Effort:** ~S.

---

## 3. `--d-state` default for s6 (surprising default) — OPTIONAL

**Problem:** CLI `--d-state` defaults to 64 for all models; `S6Layer`'s own default
is 16 and Mamba-typical is 16. A bare `--model s6` builds a 4× heavier state than
intended. (Notebook sets s6→16 explicitly, so only the bare CLI is affected.)

**Fix (pick one):**
- Resolve a per-model default in `train.train`/`factory`: if `--d-state` not
  explicitly passed and `model==s6`, use 16. (Detect "not passed" via
  `argparse` default sentinel `None`.)
- or: just document on the `--d-state` help string that 16 is the s6-typical
  value and 64 suits s4d.

**Effort:** ~S. Lowest value; doc-only is acceptable.

---

## 4. SSM backward/optimizer-step test (test gap) — DO

**Problem:** All four SSM tests are forward-only. NaN-free training was only
verified by ad-hoc CLI smoke (per PR body), nothing locks it.

**Fix:** add to `tests/train/test_ssm.py`, parametrized over `("s4d","s6")`:
- build a small model, run one TBPTT-shaped step (warmup under `no_grad` →
  detach state → forward train chunk → `composite_loss` → `backward` →
  `clip_grad_norm_` → `optimizer.step()`);
- assert all grads finite, loss finite, and at least one param changed.

Keeps it CPU-fast (tiny dims, short window).

**Effort:** ~S.

---

## 5. `BA283SSM` default `num_layers=2` (cosmetic) — OPTIONAL

Align the class default to `1` to match `BA283LSTM` and `make_model`. Harmless
today (factory always passes it explicitly). One-line change in `train/ssm.py`.

**Effort:** trivial.

---

## 6. S4D `A_im` init seeding (nitpick) — OPTIONAL

`A_im = pi*arange(N)`; with `dt` up to `1e-1` the top modes start at
`dt·A_im≈19 rad` (several wraps) — effectively aliased at init (still trainable,
output finite). Consider a scaled / `2π·linspace`-style seed so the highest modes
start better-conditioned. Only worth it if S4D underperforms and init is suspected.

**Effort:** ~S. Defer unless S4D results motivate it.

---

## 7. Python time-loop scan perf (acknowledged) — DEFER

O(L) Python loop over the window (+ warmup loop). Already documented; fine for a
research A/B. This is the thing to replace (vectorized/associative scan, or the
Rust streaming kernel) **before** any plugin port — explicitly out of scope for
this PR. No action now; track under the export/Rust follow-up the PR already
lists.

---

## Suggested sequencing
1. Land items **2, 4, 5** together (small, same files, fast tests) — cheap fidelity + coverage.
2. Land item **1** (metadata) as its own commit — biggest robustness win, own test.
3. **3, 6** as doc/polish if time; **7** stays deferred to the export effort.

Each commit: `uv run pytest tests/train tests/eval -q && uv run ruff check`.
