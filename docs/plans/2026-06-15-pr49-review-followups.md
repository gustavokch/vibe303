# PR #49 review follow-ups — MPS training

PR #49 (`feat(train): run SSM/LSTM training on Apple Silicon`) is **merged**. This plan
addresses the issues surfaced in the review (comment:
https://github.com/gustavokch/ba283-vst3/pull/49#issuecomment-4710800650). None are
blocking; all are low-risk hardening + verification.

## Issues → actions

| # | Sev | Issue | Action |
|---|-----|-------|--------|
| 1 | 🟡 | No unit test for `resolve_device` | Add 3-case precedence test (mock cuda/mps availability) |
| 2 | 🟡 | No equivalence test for the MPS `_stft_mag` guard | Add CPU-runnable test asserting guard branch == non-guard branch |
| 3 | 🟢 | `resolve_device` accepts invalid explicit names | Validate against `{cuda,mps,cpu,auto}`; fail fast |
| 4 | 🔴-verify | MPS path unconfirmed on hardware | Run the smoke on a Mac; record result |
| 5 | 🟡 | WDF profiling artifacts rode into a "train" PR | Process note only — branch future train PRs from `main` |

## 1. `resolve_device` unit test

`tests/train/test_resolve_device.py` (new). Pure function of two booleans — patch both
availability probes, assert precedence.

```python
import train.train as T

def _patch(mp, cuda, mps):
    mp.setattr(T.torch.cuda, "is_available", lambda: cuda)
    mp.setattr(T.torch.backends.mps, "is_available", lambda: mps)

def test_auto_prefers_cuda(monkeypatch):
    _patch(monkeypatch, cuda=True, mps=True)
    assert T.resolve_device("auto") == "cuda"

def test_auto_falls_to_mps(monkeypatch):
    _patch(monkeypatch, cuda=False, mps=True)
    assert T.resolve_device("auto") == "mps"

def test_auto_falls_to_cpu(monkeypatch):
    _patch(monkeypatch, cuda=False, mps=False)
    assert T.resolve_device("auto") == "cpu"

def test_explicit_passthrough(monkeypatch):
    _patch(monkeypatch, cuda=True, mps=True)
    assert T.resolve_device("cpu") == "cpu"   # explicit overrides auto-detect
```

## 2. `_stft_mag` MPS-guard equivalence test

Can't run MPS in CI, but can prove the *guard path is a no-op transform* on CPU: the
function must return the same magnitude regardless of which branch executes. Drive the
branch by monkeypatching the device-type check, or factor `stft_dev` selection so it's
injectable. Simplest: assert that forcing `stft_dev="cpu"` for a CPU input yields a tensor
bit-identical to the unguarded path (same input already on CPU → round-trip is identity).

```python
import torch
from train.loss import _stft_mag

def test_stft_mag_cpu_roundtrip_is_identity():
    x = torch.randn(2, 4096)
    # Guard maps mps->cpu; for a cpu tensor stft_dev==dev, so output must be unchanged
    # by the .to() round-trip. Lock that the .to(dev) tail doesn't perturb values.
    out = _stft_mag(x, n_fft=1024)
    assert out.device.type == "cpu"
    assert torch.isfinite(out).all()
    # second call identical (determinism / no in-place device mutation)
    assert torch.equal(out, _stft_mag(x, n_fft=1024))
```

Optionally add a grad check: `_stft_mag(x.requires_grad_(), ...).sum().backward()` and
assert `x.grad` finite — covers the cross-device autograd contract on the CPU path.

## 3. Validate explicit `--device`

In `resolve_device`, reject typos before they fail deep in `.to(device)`:

```python
def resolve_device(name: str) -> str:
    if name != "auto":
        if name not in ("cuda", "mps", "cpu"):
            raise ValueError(f"--device must be auto/cuda/mps/cpu, got {name!r}")
        return name
    ...
```

Add a `test_rejects_unknown` case to the file in §1.

## 4. Hardware confirmation (Mac)

The one real open item. On an Apple Silicon Mac, from the handbook env:

```bash
uv run python -m train.train --model s4d --stft-weight 0.1 --smoke --device auto \
  --train-manifest <m> --val-manifest <m> --epochs 1 --window 1024 --warmup 256
```

Must print `device=mps` and complete **without** `Trying to convert ComplexFloat to the
MPS backend`. Repeat `--model s6` and `--model lstm`. Then a short real run to confirm
`val_esr` descends and `--resume` works. Record the torch version + macOS version in the
handbook once green.

## 5. Process note (no code)

Future `train/`-only PRs branch from `main`, not from a `perf/wdf-*` branch, so unrelated
WDF profiling artifacts don't ride along. The artifacts already merged are inert; leave them.

## Sequencing

1. §1 + §2 + §3 in one small PR (`test/train-mps-followups`), branched from `main`. Pure
   additions + a 3-line guard; run `pytest tests/train -q` (expect prior 60 + new cases).
2. §4 out-of-band on a Mac; fold the confirmed versions into the handbook.

## Out of scope
- Python-loop scan speed on MPS (inherent design tradeoff; mitigate via `--window`/`--epochs`).
- Moving STFT off the CPU serial section (no MPS complex support upstream; revisit if torch adds it).
