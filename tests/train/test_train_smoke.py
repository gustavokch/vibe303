from pathlib import Path

import torch

from train.train import train_main

_MANIFEST = Path("tests/fixtures/manifest_tiny.csv")


def test_smoke_run_creates_checkpoint(tmp_path):
    ckpt_dir = tmp_path / "ckpts"
    log_dir = tmp_path / "runs"
    train_main(
        train_manifest=Path("tests/fixtures/manifest_tiny.csv"),
        val_manifest=Path("tests/fixtures/manifest_tiny.csv"),
        epochs=1,
        batch_size=2,
        lr=5e-3,
        hidden=32,
        warmup_samples=512,
        window=2048,
        stride=2048,
        num_workers=0,
        checkpoint_dir=ckpt_dir,
        log_dir=log_dir,
        device="cpu",
        smoke=True,
    )
    assert (ckpt_dir / "best.pth").exists()


def _smoke_kwargs(ckpt_dir, log_dir, **overrides):
    kwargs = dict(
        train_manifest=_MANIFEST,
        val_manifest=_MANIFEST,
        epochs=1,
        batch_size=2,
        lr=5e-3,
        hidden=32,
        warmup_samples=512,
        window=2048,
        stride=2048,
        num_workers=0,
        checkpoint_dir=ckpt_dir,
        log_dir=log_dir,
        device="cpu",
        smoke=True,
    )
    kwargs.update(overrides)
    return kwargs


def test_resume_continues_from_checkpoint(tmp_path):
    ckpt_dir = tmp_path / "ckpts"
    log_dir = tmp_path / "runs"

    train_main(**_smoke_kwargs(ckpt_dir, log_dir, epochs=1))

    last_path = ckpt_dir / "last.pth"
    assert last_path.exists()
    bundle = torch.load(last_path, map_location="cpu", weights_only=True)
    for key in ("optimizer", "scheduler", "epoch"):
        assert key in bundle

    # Resume and run further; should complete without error.
    train_main(**_smoke_kwargs(ckpt_dir, log_dir, epochs=2, resume=True))
    assert (ckpt_dir / "best.pth").exists()


def test_seed_reproducible(tmp_path):
    ckpt_a = tmp_path / "a"
    ckpt_b = tmp_path / "b"

    train_main(**_smoke_kwargs(ckpt_a, tmp_path / "runs_a", seed=12345))
    train_main(**_smoke_kwargs(ckpt_b, tmp_path / "runs_b", seed=12345))

    state_a = torch.load(ckpt_a / "best.pth", map_location="cpu", weights_only=True)
    state_b = torch.load(ckpt_b / "best.pth", map_location="cpu", weights_only=True)

    assert state_a.keys() == state_b.keys()
    for key in state_a:
        assert torch.equal(state_a[key], state_b[key]), key
