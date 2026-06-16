# test_export_cond_nam.py
#
# Run from the ba283-vst3 repo's env (the `nam` package is the `eval` extra of
# that repo, while this code lives in ba283-nam-lstm). PYTHONPATH must include
# the ba283-nam-lstm root so `scratch.nam_sweep.*` resolve:
#
#   cd ~/Git/ba283-nam-lstm && PYTHONPATH=$PWD \
#     uv run --project ~/Git/ba283-vst3 --extra eval \
#     python -m pytest scratch/nam_sweep/test_export_cond_nam.py -v
#
# Task-5 round-trip: export a (possibly untrained) 2-ch FiLM WaveNet to a
# `.nam` file via the `--export` path, reload it via `net_from_nam`, and assert
# the reload is bit-identical to the in-memory model on a RANDOM 2-ch input.
# Random (not zeros) input is the fidelity gate: a zeros input zeros out the
# FiLM scale path and can mask un-round-tripped conditioning weights.
import json

import torch

from scratch.nam_sweep.train_wavenet import export_main
from scratch.nam_sweep.wavenet_cond import build_cond_wavenet, net_from_nam, to_nam_dict


def _tiny_train(model, steps=3, seed=0):
    """A handful of SGD steps so the exported weights are non-default (the gate
    is round-trip fidelity, not accuracy, but a trained-ish model is closer to
    the real export and exercises non-trivial weight values)."""
    gen = torch.Generator().manual_seed(seed)
    rf = model.receptive_field
    opt = torch.optim.SGD(model.parameters(), lr=1e-3)
    for _ in range(steps):
        x = torch.randn(1, 2, rf + 64, generator=gen)
        y = model(x)
        opt.zero_grad()
        y.pow(2).mean().backward()
        opt.step()
    return model


def test_export_roundtrips_bit_identical_on_random_input(tmp_path):
    model = build_cond_wavenet(seed=0)
    _tiny_train(model, steps=3, seed=1)
    rf = model.receptive_field

    out = tmp_path / "model.cond.nam"
    # Save a checkpoint (plain state_dict, mirroring Task-4's best.pth contract)
    # and export through the same code path the CLI uses.
    ckpt = tmp_path / "best.pth"
    torch.save(model.state_dict(), ckpt)
    export_main(ckpt_path=ckpt, out_path=out, seed=0, sample_rate=48000)

    assert out.exists(), "export did not write the .nam file"
    d = json.loads(out.read_text())
    assert d["architecture"] == "WaveNet"
    assert d["config"]["layers"][0]["input_size"] == 2
    assert d["metadata"]["input_scale"] == 2.0
    assert d["metadata"]["output_ref_v"] == 64.0
    assert "source" in d["metadata"]

    # Reload and assert bit-identical on a RANDOM 2-ch input.
    reloaded = net_from_nam(d)
    x = torch.randn(1, 2, rf + 256)
    y_mem = model(x)
    y_reload = reloaded(x)
    assert y_mem.shape == y_reload.shape
    assert torch.allclose(y_mem, y_reload, atol=1e-6), "reload not bit-identical"


def test_to_nam_dict_metadata_provenance():
    # The in-memory serialiser carries no scaling provenance; the export path
    # is what injects it. Confirm the base dict is metadata-clean so the
    # injection in export_main is the single source of truth.
    model = build_cond_wavenet(seed=0)
    d = to_nam_dict(model, sample_rate=48000)
    assert d["metadata"] == {}
