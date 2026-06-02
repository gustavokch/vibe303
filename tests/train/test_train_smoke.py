from pathlib import Path

from train.train import train_main


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
