# train/train.py
"""TBPTT training loop for the BA283 GRU emulator.

A warmup region primes the GRU hidden state under ``no_grad``, then
backpropagation runs only over the train region using the carried (detached)
hidden state. The objective is the composite ESR + MSE pre-emphasis loss.

The loop is hardened for unattended multi-hour runs: gradient clipping bounds
GRU divergence and any batch with a non-finite loss OR non-finite gradient is
dropped before the optimizer step (so an inf gradient can never be turned into a
NaN parameter that poisons the whole run), checkpoints are written atomically,
and ``last.pth`` carries a full resume bundle so a crash does not discard hours
of compute.
"""

from __future__ import annotations

import argparse
import datetime
import json
import math
import os
import time
import warnings
from pathlib import Path

import torch
from torch.utils.tensorboard import SummaryWriter

from train.dataset import make_loader
from train.factory import ARCH_FILENAME, arch_spec, make_model, resolve_d_state
from train.loss import composite_loss, esr


def resolve_device(name: str) -> str:
    """Resolve ``--device`` to a concrete backend string.

    ``"auto"`` picks the fastest available backend: CUDA (Colab/dGPU) -> MPS
    (Apple Silicon) -> CPU. Any explicit name (``"cuda"``/``"mps"``/``"cpu"``)
    is returned unchanged so Colab and CI keep pinning the device directly. An
    unrecognized name is rejected here so a typo (e.g. ``"mpz"``) fails fast
    instead of surfacing as an opaque error deep in a later ``.to(device)``.
    """
    if name != "auto":
        if name not in ("cuda", "mps", "cpu"):
            raise ValueError(
                f"--device must be 'auto'/'cuda'/'mps'/'cpu', got {name!r}"
            )
        return name
    if torch.cuda.is_available():
        return "cuda"
    if torch.backends.mps.is_available():
        return "mps"
    return "cpu"


def _atomic_save(state: object, path: Path) -> None:
    """Save ``state`` to ``path`` atomically (tmp file + ``os.replace``).

    Writing to a sibling temp file and then renaming means a kill mid-write
    leaves either the old file or the new one intact, never a truncated file.
    """
    tmp = path.with_suffix(path.suffix + ".tmp")
    torch.save(state, tmp)
    os.replace(tmp, path)


def _fmt_elapsed(seconds: float) -> str:
    """Format an elapsed duration (seconds since run start T) as ``+HH:MM:SS``."""
    total = int(seconds)
    h, rem = divmod(total, 3600)
    m, s = divmod(rem, 60)
    return f"+{h:02d}:{m:02d}:{s:02d}"


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
    conditioning: str = "film",
    num_layers: int = 1,
    dropout: float = 0.0,
    head: str = "linear",
    film_kind: str = "linear",
    weight_decay: float = 0.0,
    stft_weight: float = 0.0,
    balance_by_kind: bool = False,
    model_type: str = "lstm",
    d_state: int = 64,
    d_conv: int = 4,
    expand: int = 2,
) -> None:
    torch.manual_seed(seed)

    # pin_memory only accelerates host->CUDA transfers; on MPS/CPU it warns and
    # wastes work. ``device`` here is already resolved (see ``resolve_device``).
    pin_memory = str(device).startswith("cuda")

    train_loader = make_loader(
        train_manifest,
        batch_size=batch_size,
        window=window,
        stride=stride,
        num_workers=num_workers,
        shuffle=True,
        balance_by_kind=balance_by_kind,
        pin_memory=pin_memory,
    )
    val_loader = make_loader(
        val_manifest,
        batch_size=batch_size,
        window=window,
        stride=stride,
        num_workers=num_workers,
        shuffle=False,
        drop_last=False,
        pin_memory=pin_memory,
    )

    model = make_model(
        model_type,
        hidden=hidden,
        conditioning=conditioning,
        num_layers=num_layers,
        dropout=dropout,
        head=head,
        film_kind=film_kind,
        d_state=d_state,
        d_conv=d_conv,
        expand=expand,
    ).to(device)
    optimizer = torch.optim.Adam(model.parameters(), lr=lr, weight_decay=weight_decay)
    scheduler = torch.optim.lr_scheduler.CosineAnnealingLR(optimizer, T_max=epochs)
    writer = SummaryWriter(log_dir=str(log_dir))

    checkpoint_dir.mkdir(parents=True, exist_ok=True)
    best_path = checkpoint_dir / "best.pth"
    last_path = checkpoint_dir / "last.pth"
    # Record the architecture next to the weights so the eval harness can
    # reconstruct (and cross-check) the model without being hand-fed every knob.
    (checkpoint_dir / ARCH_FILENAME).write_text(
        json.dumps(
            arch_spec(
                model_type,
                hidden=hidden,
                conditioning=conditioning,
                num_layers=num_layers,
                head=head,
                film_kind=film_kind,
                d_state=d_state,
                d_conv=d_conv,
                expand=expand,
            ),
            indent=2,
        )
    )

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

    # Wall-clock start T (header) + monotonic reference for per-epoch deltas.
    run_start = time.monotonic()
    t_zero = datetime.datetime.now()
    print(f"training start T={t_zero:%Y-%m-%d %H:%M:%S}", flush=True)

    try:
        for e in range(start_epoch, epochs):
            epoch_start = time.monotonic()
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
                    _, state = model(warmup_x)
                # LSTM state is a (h, c) tuple; detach both to truncate BPTT.
                state = tuple(s.detach() for s in state)

                pred, _ = model(train_x, state)
                loss = composite_loss(pred, train_y, stft_weight=stft_weight)

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
                # clip_grad_norm_ does NOT protect against an already inf/NaN
                # gradient: total_norm becomes inf, clip_coef -> 0, and inf*0 = NaN
                # -> the step would poison every parameter permanently (NaN never
                # recovers; every later loss is then non-finite and skipped). So
                # guard explicitly: if any gradient is non-finite, drop the batch.
                total_norm = torch.nn.utils.clip_grad_norm_(
                    model.parameters(), max_norm=clip_norm
                )
                if not torch.isfinite(total_norm):
                    warnings.warn(
                        f"non-finite gradient at epoch {e} batch {batch_idx}; "
                        f"skipping step",
                        RuntimeWarning,
                        stacklevel=2,
                    )
                    optimizer.zero_grad(set_to_none=True)
                    if smoke and batch_idx >= 1:
                        break
                    continue
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

            # Per-epoch curve to stdout (tensorboard alone is invisible on Colab):
            # the trajectory distinguishes "still descending" from "plateaued /
            # collapsed" and shows whether best.pth is an early checkpoint.
            print(
                f"[{_fmt_elapsed(time.monotonic() - run_start)}] "
                f"epoch {e:3d}  train_loss={mean_train_loss:.4f}  "
                f"val_esr={val_esr:.4f}  best={best_esr:.4f}"
                f"{'  *' if improved else ''}  "
                f"no_improve={epochs_since_improvement}  "
                f"dt={time.monotonic() - epoch_start:.1f}s",
                flush=True,
            )

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
    # 5e-3: the arvidf/GuitarML LSTM recipe. NOTE: the earlier GRU exploded its
    # BPTT gradient at 5e-3 over the ~3072-step train window (bursts of
    # non-finite gradients ~epoch 10, wasted batches); 1e-3 was stable for it.
    # The LSTM's gating is steadier and the non-finite loss/grad guards +
    # clip_norm drop any blown batch, but watch early epochs for the same burst.
    parser.add_argument("--lr", type=float, default=5e-3)
    parser.add_argument("--hidden", type=int, default=32)
    parser.add_argument("--warmup", type=int, default=1024)
    parser.add_argument("--window", type=int, default=4096)
    parser.add_argument("--stride", type=int, default=2048)
    parser.add_argument("--workers", type=int, default=6)
    parser.add_argument("--checkpoint-dir", type=Path, default=Path("checkpoints/"))
    parser.add_argument("--log-dir", type=Path, default=Path("runs/"))
    parser.add_argument(
        "--device",
        type=str,
        default="auto",
        help="'auto' (cuda->mps->cpu) or an explicit 'cuda'/'mps'/'cpu'",
    )
    parser.add_argument("--smoke", action="store_true")
    parser.add_argument("--clip-norm", type=float, default=5.0)
    parser.add_argument("--resume", action="store_true")
    parser.add_argument("--seed", type=int, default=0xBA283)
    parser.add_argument("--patience", type=int, default=10)
    parser.add_argument(
        "--conditioning",
        choices=("film", "concat"),
        default="film",
        help="bias conditioning: 'film' (affine modulation) or 'concat' (input channel)",
    )
    # --- Redesign knobs (all default to the shipped v1 behaviour) ---
    parser.add_argument("--layers", type=int, default=1, help="LSTM stack depth")
    parser.add_argument(
        "--dropout", type=float, default=0.0, help="inter-layer LSTM dropout (needs --layers > 1)"
    )
    parser.add_argument(
        "--head",
        choices=("linear", "mlp"),
        default="linear",
        help="output readout: 'linear' (Dense) or 'mlp' (Dense->tanh->Dense)",
    )
    parser.add_argument(
        "--film",
        choices=("linear", "mlp"),
        default="linear",
        help="FiLM gain map: 'linear' or 'mlp' (Linear->tanh->Linear); film conditioning only",
    )
    parser.add_argument(
        "--weight-decay", type=float, default=0.0, help="Adam L2 weight decay"
    )
    parser.add_argument(
        "--stft-weight",
        type=float,
        default=0.0,
        help="weight of the multi-resolution STFT loss term (0 = pure ESR)",
    )
    parser.add_argument(
        "--balance-by-kind",
        action="store_true",
        help="weighted sampling to de-skew stim_kind imbalance (e.g. music dominance)",
    )
    # --- Model selection (LSTM baseline vs state-space models) ---
    parser.add_argument(
        "--model",
        choices=("lstm", "s4d", "s6"),
        default="lstm",
        help="architecture: 'lstm' (shipped baseline), 's4d' or 's6' (state-space)",
    )
    parser.add_argument(
        "--d-state",
        type=int,
        default=None,
        help="SSM state size N (s4d/s6 only); default 64 for s4d, 16 for s6",
    )
    parser.add_argument(
        "--d-conv", type=int, default=4, help="S6 causal-conv kernel (s6 only)"
    )
    parser.add_argument(
        "--expand", type=int, default=2, help="S6 inner expansion factor (s6 only)"
    )
    args = parser.parse_args()

    device = resolve_device(args.device)
    print(f"device={device}", flush=True)

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
        device=device,
        smoke=args.smoke,
        clip_norm=args.clip_norm,
        resume=args.resume,
        seed=args.seed,
        patience=args.patience,
        conditioning=args.conditioning,
        num_layers=args.layers,
        dropout=args.dropout,
        head=args.head,
        film_kind=args.film,
        weight_decay=args.weight_decay,
        stft_weight=args.stft_weight,
        balance_by_kind=args.balance_by_kind,
        model_type=args.model,
        d_state=resolve_d_state(args.model, args.d_state),
        d_conv=args.d_conv,
        expand=args.expand,
    )


if __name__ == "__main__":
    main()
