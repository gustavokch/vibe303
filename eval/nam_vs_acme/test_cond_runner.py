"""
Task-6 Step-1 test: prove ``run_nam.NamModel`` loads and runs a *conditioned*
(2-channel FiLM) WaveNet ``.nam`` and that the gain channel is live.

Contract under test
--------------------
* ``NamModel(p).run(dry, gain=g)`` builds the 2-ch input ``[dry*scale, g]``,
  front-pads by ``RF-1`` (VALID-mode net), forwards, and returns a finite
  output of ``len(dry)``.
* Different ``g`` yields a different output (conditioning is live, not ignored).
* The legacy ``SlimmableContainer`` ``model.nam`` still runs via ``run(dry)``
  with ``gain`` defaulting to ``None`` (back-compat, no Julia/ACME needed).

The conditioned model is built directly via the nam-lstm scratch package
(``build_cond_wavenet`` + ``export_main``) into a temp ``.nam`` so the test is
self-contained and does NOT require the long Task-7 training run.  It also does
NOT require Julia/ACME.
"""
from __future__ import annotations

import sys
from pathlib import Path

import numpy as np
import pytest

from eval.nam_vs_acme.run_nam import DEFAULT_NAM, NamModel

# The conditioned-model builder lives in the sibling ba283-nam-lstm repo; add
# its root to sys.path so ``scratch.nam_sweep.*`` resolves.  Skip the
# conditioned tests cleanly if that repo isn't checked out next to this one.
_NAM_LSTM_ROOT = Path.home() / "Git/ba283-nam-lstm"
if str(_NAM_LSTM_ROOT) not in sys.path:
    sys.path.insert(0, str(_NAM_LSTM_ROOT))

try:
    import torch  # noqa: E402
    from scratch.nam_sweep.train_wavenet import export_main  # noqa: E402
    from scratch.nam_sweep.wavenet_cond import build_cond_wavenet  # noqa: E402

    _HAVE_COND_BUILDER = True
except Exception:  # pragma: no cover - environment without the nam-lstm repo
    _HAVE_COND_BUILDER = False

_requires_builder = pytest.mark.skipif(
    not _HAVE_COND_BUILDER,
    reason="ba283-nam-lstm scratch package (build_cond_wavenet/export_main) unavailable",
)


@pytest.fixture(scope="module")
def cond_nam(tmp_path_factory) -> Path:
    """Build a tiny conditioned WaveNet, run a few SGD steps so the FiLM/
    conditioning weights are non-degenerate, export it to a temp ``.nam``."""
    seed = 0
    model = build_cond_wavenet(seed=seed)
    rf = model.receptive_field
    # A handful of SGD steps so the gain channel actually influences the output
    # (random-init weights already differ across gains, but training makes the
    # "different gain -> different output" gate robust to chance cancellation).
    gen = torch.Generator().manual_seed(1)
    opt = torch.optim.SGD(model.parameters(), lr=1e-3)
    for _ in range(3):
        x = torch.randn(1, 2, rf + 64, generator=gen)
        opt.zero_grad()
        model(x).pow(2).mean().backward()
        opt.step()
    model.train(False)

    out_dir = tmp_path_factory.mktemp("cond_nam")
    ckpt = out_dir / "best.pth"
    torch.save(model.state_dict(), ckpt)
    nam_path = out_dir / "model.cond.nam"
    export_main(ckpt_path=ckpt, out_path=nam_path, seed=seed, sample_rate=48000)
    return nam_path


@_requires_builder
def test_conditioned_runs_finite_and_length(cond_nam):
    model = NamModel(cond_nam)
    assert model._conditioned, "model should be detected as conditioned (input_size==2)"
    rf = model._receptive_field
    rng = np.random.default_rng(0)
    dry = rng.standard_normal(rf + 1000).astype(np.float64) * 0.3

    wet, thr = model.run(dry, gain=0.2)
    assert wet.shape == (len(dry),), "output length must equal len(dry)"
    assert np.all(np.isfinite(wet)), "output must be finite"
    assert thr == pytest.approx(0.2), "conditioned path echoes the gain as threshold"


@_requires_builder
def test_conditioning_is_live(cond_nam):
    """Different gains must yield different outputs (the gain channel reaches
    the FiLM hooks)."""
    model = NamModel(cond_nam)
    rf = model._receptive_field
    rng = np.random.default_rng(7)
    dry = rng.standard_normal(rf + 1000).astype(np.float64) * 0.3

    wet_lo, _ = model.run(dry, gain=0.2)
    wet_hi, _ = model.run(dry, gain=0.8)
    diff = float(np.max(np.abs(wet_hi - wet_lo)))
    assert diff > 0.0, "gain=0.2 and gain=0.8 produced identical output (conditioning dead)"


@_requires_builder
def test_conditioned_requires_gain(cond_nam):
    model = NamModel(cond_nam)
    dry = np.zeros(model._receptive_field + 100, dtype=np.float64)
    with pytest.raises(ValueError):
        model.run(dry)  # gain=None must be rejected for the conditioned path


@pytest.mark.skipif(not DEFAULT_NAM.exists(),
                    reason="legacy SlimmableContainer model.nam not present")
def test_legacy_slimmable_backcompat():
    """The legacy SlimmableContainer model still runs via run(dry) with gain
    defaulting to None (no Julia/ACME involved)."""
    model = NamModel(DEFAULT_NAM)
    assert model.arch == "SlimmableContainer"
    assert not model._conditioned
    rng = np.random.default_rng(3)
    dry = (rng.standard_normal(8000) * 0.3).astype(np.float64)
    wet, thr = model.run(dry)  # legacy call: no gain
    assert wet.shape == (len(dry),)
    assert np.all(np.isfinite(wet))
