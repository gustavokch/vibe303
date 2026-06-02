# train/train.py
"""TBPTT training loop for the BA283 GRU emulator.

A warmup region primes the GRU hidden state under ``no_grad``, then
backpropagation runs only over the train region using the carried (detached)
hidden state. The objective is the composite ESR + MSE pre-emphasis loss.

The loop is hardened for unattended multi-hour runs: gradient clipping guards
against GRU divergence, non-finite batches are skipped, checkpoints are written
atomically, and ``last.pth`` carries a full resume bundle so a crash does not
discard hours of compute.
"""

from __future__ import annotations

import argparse
import math
import os
import warnings
from pathlib import Path

import torch
from torch.utils.tensorboard import SummaryWriter

from train.dataset import make_loader
from train.loss import composite_loss, esr
from train.model import BA283GRU


def _atomic_save(state: object, path: Path) -> None:
    """Save ``state`` to ``path`` atomically (tmp file + ``os.replace``).

    Writing to a sibling temp file and then renaming means a kill mid-write
    leaves either the old file or the new one intact, never a truncated file.
    """
    tmp = path.with_suffix(path.suffix + ".tmp")
    torch.save(state, tmp)
    os.replace(tmp, path)


def train_main(
    *,
    train_manifest: Path,
    val_manifest: Path,
    epochs: int,
    batch_size: int,
    lr: float,
    hidden: int,
    warmup_samples: int,
    window: int,
    stride: int,
    num_workers: int,
    checkpoint_dir: Path,
    log_dir: Path,
    device: str,
    smoke: bool = False,
    clip_norm: float = 5.0,
    resume: bool = False,
    seed: int = 0xBA283,
    patience: int = 10,
) -> None:
    torch.manual_seed(seed)

    train_loader = make_loader(
        train_manifest,
        batch_size=batch_size,
        window=window,
        stride=stride,
        num_workers=num_workers,
        shuffle=True,
    )
    val_loader = make_loader(
        val_manifest,
        batch_size=batch_size,
        window=window,
        stride=stride,
        num_workers=num_workers,
        shuffle=False,
        drop_last=False,
    )

    model = BA283GRU(hidden=hidden).to(device)
    optimizer = torch.optim.Adam(model.parameters(), lr=lr)
    scheduler = torch.optim.lr_scheduler.CosineAnnealingLR(optimizer, T_max=epochs)
    writer = SummaryWriter(log_dir=str(log_dir))

    checkpoint_dir.mkdir(parents=True, exist_ok=True)
    best_path = checkpoint_dir / "best.pth"
    last_path = checkpoint_dir / "last.pth"

    best_esr = float("inf")
    epochs_since_improvement = 0
    start_epoch = 0

    # --- Resume from last.pth if requested and available ---
    if resume and last_path.exists():
        ckpt = torch.load(last_path, map_location=device, weights_only=True)
        model.load_state_dict(ckpt["model"])
        optimizer.load_state_dict(ckpt["optimizer"])
        scheduler.load_state_dict(ckpt["scheduler"])
        best_esr = ckpt["best_esr"]
        epochs_since_improvement = ckpt["epochs_since_improvement"]
        start_epoch = ckpt["epoch"] + 1

    try:
        for e in range(start_epoch, epochs):
            # --- Train ---
            model.train()
            loss_sum = 0.0
            loss_count = 0
            for batch_idx, (x, y) in enumerate(train_loader):
                x = x.to(device)
                y = y.to(device)

                warmup_x = x[:, :warmup_samples]
                train_x = x[:, warmup_samples:]
                train_y = y[:, warmup_samples:]

                with torch.no_grad():
                    _, h = model(warmup_x)
                h = h.detach()

                pred, _ = model(train_x, h)
                loss = composite_loss(pred, train_y)

                if not torch.isfinite(loss):
                    warnings.warn(
                        f"non-finite training loss at epoch {e} batch "
                        f"{batch_idx}; skipping batch",
                        RuntimeWarning,
                        stacklevel=2,
                    )
                    if smoke and batch_idx >= 1:
                        break
                    continue

                optimizer.zero_grad()
                loss.backward()
                torch.nn.utils.clip_grad_norm_(model.parameters(), max_norm=clip_norm)
                optimizer.step()

                loss_sum += loss.item()
                loss_count += 1
                if smoke and batch_idx >= 1:
                    break

            # --- Validation ---
            model.train(False)
            preds: list[torch.Tensor] = []
            targets: list[torch.Tensor] = []
            with torch.no_grad():
                for batch_idx, (x, y) in enumerate(val_loader):
                    x = x.to(device)
                    y = y.to(device)

                    warmup_x = x[:, :warmup_samples]
                    train_x = x[:, warmup_samples:]
                    train_y = y[:, warmup_samples:]

                    _, h = model(warmup_x)
                    pred, _ = model(train_x, h)

                    preds.append(pred)
                    targets.append(train_y)
                    if smoke and batch_idx >= 1:
                        break

            all_pred = torch.cat(preds, dim=0)
            all_target = torch.cat(targets, dim=0)
            val_esr = esr(all_pred, all_target).item()

            scheduler.step()

            mean_train_loss = loss_sum / loss_count if loss_count else float("nan")
            writer.add_scalar("train/loss", mean_train_loss, e)
            writer.add_scalar("val/esr", val_esr, e)

            improved = math.isfinite(val_esr) and val_esr < best_esr
            if improved:
                best_esr = val_esr
                epochs_since_improvement = 0
                _atomic_save(model.state_dict(), checkpoint_dir / f"epoch_{e:03d}.pth")
                _atomic_save(model.state_dict(), best_path)
            else:
                epochs_since_improvement += 1

            # Resume bundle: richer than best.pth, written every epoch.
            _atomic_save(
                {
                    "epoch": e,
                    "model": model.state_dict(),
                    "optimizer": optimizer.state_dict(),
                    "scheduler": scheduler.state_dict(),
                    "best_esr": best_esr,
                    "epochs_since_improvement": epochs_since_improvement,
                },
                last_path,
            )

            if not improved and epochs_since_improvement >= patience:
                break
    finally:
        writer.close()

    # --- Restore best weights into the in-memory model (matches export). ---
    if best_path.exists():
        model.load_state_dict(
            torch.load(best_path, map_location=device, weights_only=True)
        )


def main() -> None:
    parser = argparse.ArgumentParser(description="Train the BA283 GRU emulator.")
    parser.add_argument("--train-manifest", type=Path, required=True)
    parser.add_argument("--val-manifest", type=Path, required=True)
    parser.add_argument("--epochs", type=int, default=200)
    parser.add_argument("--batch", type=int, default=64)
    parser.add_argument("--lr", type=float, default=5e-3)
    parser.add_argument("--hidden", type=int, default=32)
    parser.add_argument("--warmup", type=int, default=1024)
    parser.add_argument("--window", type=int, default=4096)
    parser.add_argument("--stride", type=int, default=2048)
    parser.add_argument("--workers", type=int, default=6)
    parser.add_argument("--checkpoint-dir", type=Path, default=Path("checkpoints/"))
    parser.add_argument("--log-dir", type=Path, default=Path("runs/"))
    parser.add_argument("--device", type=str, default="cpu")
    parser.add_argument("--smoke", action="store_true")
    parser.add_argument("--clip-norm", type=float, default=5.0)
    parser.add_argument("--resume", action="store_true")
    parser.add_argument("--seed", type=int, default=0xBA283)
    parser.add_argument("--patience", type=int, default=10)
    args = parser.parse_args()

    train_main(
        train_manifest=args.train_manifest,
        val_manifest=args.val_manifest,
        epochs=args.epochs,
        batch_size=args.batch,
        lr=args.lr,
        hidden=args.hidden,
        warmup_samples=args.warmup,
        window=args.window,
        stride=args.stride,
        num_workers=args.workers,
        checkpoint_dir=args.checkpoint_dir,
        log_dir=args.log_dir,
        device=args.device,
        smoke=args.smoke,
        clip_norm=args.clip_norm,
        resume=args.resume,
        seed=args.seed,
        patience=args.patience,
    )


if __name__ == "__main__":
    main()
