# train/train.py
"""TBPTT training loop for the BA283 GRU emulator.

A warmup region primes the GRU hidden state under ``no_grad``, then
backpropagation runs only over the train region using the carried (detached)
hidden state. The objective is the composite ESR + MSE pre-emphasis loss.
"""

from __future__ import annotations

import argparse
from pathlib import Path

import torch
from torch.utils.tensorboard import SummaryWriter

from train.dataset import make_loader
from train.loss import composite_loss, esr
from train.model import BA283GRU

_PATIENCE = 10


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
) -> None:
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

    best_esr = float("inf")
    epochs_since_improvement = 0

    for e in range(epochs):
        # --- Train ---
        model.train()
        train_loss = 0.0
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

            optimizer.zero_grad()
            loss.backward()
            optimizer.step()

            train_loss = loss.item()
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

        writer.add_scalar("train/loss", train_loss, e)
        writer.add_scalar("val/esr", val_esr, e)

        if val_esr < best_esr:
            best_esr = val_esr
            epochs_since_improvement = 0
            torch.save(model.state_dict(), checkpoint_dir / f"epoch_{e:03d}.pth")
            torch.save(model.state_dict(), checkpoint_dir / "best.pth")
        else:
            epochs_since_improvement += 1
            if epochs_since_improvement >= _PATIENCE:
                break

    writer.close()


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
    )


if __name__ == "__main__":
    main()
