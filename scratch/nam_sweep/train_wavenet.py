"""Stateless training loop for the 2-channel, FiLM-conditioned WaveNet.

`train/train.py` is LSTM-specific (stateful h0/c0, ``model(x) -> (pred, state)``,
in-graph burn-in for TBPTT). A WaveNet is *stateless* with a fixed receptive-field
latency, so it gets its own loop -- but reuses the dataset (``train.dataset``), the
loss (``train.loss``), and the finite-guard discipline of ``train.train``.

The model comes from Task 1 (``wavenet_cond.build_cond_wavenet``): the inner
``_wavenet.WaveNet`` with ``input_size = condition_size = 2``, FiLM active, and
``receptive_field`` (RF) = 6347. Its forward is VALID (no pad_start):

    forward: (B, 2, L) -> (B, 1, L - (RF - 1))

so the output is already trimmed by RF-1 on the FRONT. ``loss_region`` therefore
drops the target's first RF-1 samples to time-align it with that valid tail (see
its docstring). It also handles the pad_start case (output length == input length)
should a future net use it, by trimming both fronts.

CRITICAL -- window MUST exceed RF.  The dataset's default ``window=4096`` is
SMALLER than RF=6347, so a 4096-sample input yields ZERO valid output. Train with
``window = RF + n_out`` where ``n_out`` is the per-window supervised length (default
``n_out = 4096`` -> ``window = 10443``). ``--stride`` defaults to ``n_out`` so
windows tile the supervised region without overlap. This RF-vs-window relationship
is the #1 footgun of this loop; ``train_main`` asserts ``window > RF``.

Run (the ``nam`` package lives in ba283-vst3's evaluation extra; PYTHONPATH must
include the ba283-nam-lstm root so ``scratch.nam_sweep`` / ``train`` resolve):

    cd ~/Git/ba283-nam-lstm && PYTHONPATH=$PWD \
      uv run --project ~/Git/ba283-vst3 --extra eval \
      python scratch/nam_sweep/train_wavenet.py --device cpu --epochs 1 --smoke
"""

from __future__ import annotations

import argparse
import math
import os
import time
import warnings
from pathlib import Path

import torch
from torch.utils.tensorboard import SummaryWriter

from scratch.nam_sweep.wavenet_cond import build_cond_wavenet
from train.dataset import make_loader
from train.loss import composite_loss, esr

# Defaults for the freshly-rebuilt 9-gain ds48 manifests (relative to repo root).
_DEFAULT_TRAIN = Path("scratch/nam_sweep/ds48/train.csv")
_DEFAULT_VAL = Path("scratch/nam_sweep/ds48/val.csv")


def resolve_device(name: str) -> str:
    """Resolve ``--device`` to a concrete backend string (mirrors train.train)."""
    if name != "auto":
        if name not in ("cuda", "mps", "cpu"):
            raise ValueError(f"--device must be 'auto'/'cuda'/'mps'/'cpu', got {name!r}")
        return name
    if torch.cuda.is_available():
        return "cuda"
    if torch.backends.mps.is_available():
        return "mps"
    return "cpu"


def _atomic_save(state: object, path: Path) -> None:
    """Save ``state`` to ``path`` atomically (tmp file + ``os.replace``)."""
    tmp = path.with_suffix(path.suffix + ".tmp")
    torch.save(state, tmp)
    os.replace(tmp, path)


def loss_region(
    pred: torch.Tensor, target: torch.Tensor, rf: int
) -> tuple[torch.Tensor, torch.Tensor]:
    """Align ``pred`` and ``target`` to a common, warmup-free region.

    The WaveNet has a receptive-field latency: ``pred[..., t]`` depends on the
    input samples ``[t, t + rf - 1]``, so the first ``rf - 1`` output positions
    have only partial context (the WaveNet analogue of the LSTM burn-in). Two
    forward modes are handled:

    * VALID (no pad_start, what the inner ``_wavenet.WaveNet`` uses): forward maps
      length ``L`` -> ``L - (rf - 1)``. ``pred`` is therefore ALREADY trimmed by
      ``rf - 1`` on the front; we only trim the target's FIRST ``rf - 1`` samples
      so it lines up with that valid tail. ``pred`` is returned untouched.
    * pad_start (output length == target length): the first ``rf - 1`` outputs are
      warmup; trim BOTH fronts by ``rf - 1``.

    Mode is inferred from the lengths: equal lengths -> pad_start, else valid.
    Returns ``(pred_r, target_r)``, equal length, on the same time base.
    """
    pred_len = pred.shape[-1]
    tgt_len = target.shape[-1]
    if pred_len == tgt_len:
        # pad_start mode: drop warmup from both.
        return pred[..., rf - 1:], target[..., rf - 1:]
    # VALID mode: pred is already the trimmed tail; align target to it.
    if tgt_len - pred_len != rf - 1:
        raise ValueError(
            f"loss_region: cannot align pred (len {pred_len}) to target "
            f"(len {tgt_len}) with rf={rf}; expected target == pred + (rf - 1) "
            f"(valid) or target == pred (pad_start)"
        )
    return pred, target[..., rf - 1:]


def _to_loss_layout(t_bct: torch.Tensor) -> torch.Tensor:
    """(B, C, T) -> (B, T, C); ``train.loss`` slices time on dim 1, channel dim 2."""
    return t_bct.transpose(1, 2).contiguous()


def _run_batch(model, x_bw2, y_bw1, rf, device):
    """Forward one batch and return loss-layout (pred, target) over the loss region.

    ``x_bw2`` is ``(B, window, 2)`` and ``y_bw1`` is ``(B, window, 1)`` from the
    dataset. The WaveNet wants channel-first ``(B, 2, window)``; the target is made
    channel-first ``(B, 1, window)`` so ``loss_region`` trims it on the time axis.
    Returns ``(pred_loss, tgt_loss)`` both shaped ``(B, T_out, 1)`` for
    ``composite_loss`` / ``esr``.
    """
    x = x_bw2.to(device).transpose(1, 2)  # (B, 2, window)
    y = y_bw1.to(device).transpose(1, 2)  # (B, 1, window)
    pred = model(x)  # (B, 1, window - (rf - 1)) in valid mode
    pred_r, tgt_r = loss_region(pred, y, rf)
    return _to_loss_layout(pred_r), _to_loss_layout(tgt_r)


def train_main(
    *,
    train_manifest: Path,
    val_manifest: Path,
    epochs: int,
    batch_size: int,
    lr: float,
    window: int,
    stride: int,
    num_workers: int,
    checkpoint_dir: Path,
    log_dir: Path,
    device: str,
    seed: int,
    clip_norm: float = 5.0,
    weight_decay: float = 0.0,
    stft_weight: float = 0.0,
    smoke: bool = False,
    smoke_steps: int = 200,
) -> None:
    torch.manual_seed(seed)
    model = build_cond_wavenet(seed=seed).to(device)
    rf = model.receptive_field

    # The #1 footgun: a window <= RF yields ZERO valid output (the whole window is
    # receptive-field warmup). The supervised length is window - (rf - 1) > 0.
    if window <= rf:
        raise ValueError(
            f"window ({window}) must exceed the receptive field ({rf}); the "
            f"valid output length (window - (rf - 1)) would be <= 1. Use "
            f"window = rf + n_out (e.g. {rf + 4096})."
        )

    pin_memory = str(device).startswith("cuda")
    train_loader = make_loader(
        train_manifest,
        batch_size=batch_size,
        window=window,
        stride=stride,
        num_workers=num_workers,
        shuffle=True,
        drop_last=True,
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

    optimizer = torch.optim.Adam(model.parameters(), lr=lr, weight_decay=weight_decay)
    scheduler = torch.optim.lr_scheduler.CosineAnnealingLR(optimizer, T_max=max(1, epochs))
    writer = SummaryWriter(log_dir=str(log_dir))

    checkpoint_dir.mkdir(parents=True, exist_ok=True)
    best_path = checkpoint_dir / "best.pth"

    best_esr = float("inf")
    global_step = 0
    run_start = time.monotonic()
    print(f"device={device}  rf={rf}  window={window}  n_out={window - (rf - 1)}", flush=True)

    try:
        for e in range(epochs):
            model.train()
            loss_sum = 0.0
            loss_count = 0
            for batch_idx, (x, y) in enumerate(train_loader):
                pred_loss, tgt_loss = _run_batch(model, x, y, rf, device)
                loss = composite_loss(pred_loss, tgt_loss, stft_weight=stft_weight)

                # --- Non-finite loss guard (train.train:231-242). Volt-domain
                # targets + ESR denominators blew up the Colab run; skip the batch.
                if not torch.isfinite(loss):
                    warnings.warn(
                        f"non-finite training loss at epoch {e} batch {batch_idx}; "
                        f"skipping batch",
                        RuntimeWarning,
                        stacklevel=2,
                    )
                    optimizer.zero_grad(set_to_none=True)
                    if smoke and global_step + 1 >= smoke_steps:
                        break
                    continue

                optimizer.zero_grad()
                loss.backward()
                # --- Non-finite grad guard (train.train:244-262). clip_grad_norm_
                # does NOT protect against an already-inf gradient (inf*0 = NaN),
                # so drop the step explicitly if the total norm is non-finite.
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
                    if smoke and global_step + 1 >= smoke_steps:
                        break
                    continue
                optimizer.step()

                loss_sum += loss.item()
                loss_count += 1
                writer.add_scalar("train/loss", loss.item(), global_step)
                global_step += 1

                if smoke and global_step % 25 == 0:
                    print(
                        f"  [smoke] step {global_step:4d}  loss={loss.item():.5f}",
                        flush=True,
                    )
                if smoke and global_step >= smoke_steps:
                    break

            # --- Validation: val ESR over the same valid-region alignment. ---
            model.eval()
            preds: list[torch.Tensor] = []
            targets: list[torch.Tensor] = []
            with torch.no_grad():
                for batch_idx, (x, y) in enumerate(val_loader):
                    pred_loss, tgt_loss = _run_batch(model, x, y, rf, device)
                    preds.append(pred_loss)
                    targets.append(tgt_loss)
                    if smoke and batch_idx >= 1:
                        break

            val_esr = float("nan")
            if preds:
                all_pred = torch.cat(preds, dim=0)
                all_target = torch.cat(targets, dim=0)
                val_esr = esr(all_pred, all_target).item()

            scheduler.step()
            mean_train_loss = loss_sum / loss_count if loss_count else float("nan")
            writer.add_scalar("train/epoch_loss", mean_train_loss, e)
            writer.add_scalar("val/esr", val_esr, e)

            improved = math.isfinite(val_esr) and val_esr < best_esr
            if improved:
                best_esr = val_esr
                _atomic_save(model.state_dict(), best_path)

            elapsed = time.monotonic() - run_start
            print(
                f"[{elapsed:6.1f}s] epoch {e:3d}  "
                f"train_loss={mean_train_loss:.5f}  val_esr={val_esr:.5f}  "
                f"best={best_esr:.5f}{'  *' if improved else ''}",
                flush=True,
            )

            if smoke and global_step >= smoke_steps:
                break
    finally:
        writer.close()

    if best_path.exists():
        model.load_state_dict(
            torch.load(best_path, map_location=device, weights_only=True)
        )


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Train the 2-channel FiLM-conditioned BA283 WaveNet."
    )
    parser.add_argument("--train-manifest", type=Path, default=_DEFAULT_TRAIN)
    parser.add_argument("--val-manifest", type=Path, default=_DEFAULT_VAL)
    parser.add_argument("--epochs", type=int, default=200)
    parser.add_argument("--batch", type=int, default=16)
    parser.add_argument("--lr", type=float, default=4e-3)
    # window = rf + n_out. n_out is the supervised length per window; window is
    # derived from it unless --window is given explicitly. RF=6347, so n_out=4096
    # -> window=10443. See module docstring (#1 footgun).
    parser.add_argument(
        "--n-out", type=int, default=4096,
        help="supervised samples per window; window = receptive_field + n_out",
    )
    parser.add_argument(
        "--window", type=int, default=None,
        help="override the input window length (must exceed the receptive field)",
    )
    parser.add_argument(
        "--stride", type=int, default=None,
        help="window hop in samples (default = n_out: non-overlapping supervision)",
    )
    parser.add_argument("--workers", type=int, default=0)
    parser.add_argument("--checkpoint-dir", type=Path, default=Path("checkpoints_wavenet/"))
    parser.add_argument("--log-dir", type=Path, default=Path("runs_wavenet/"))
    parser.add_argument(
        "--device", type=str, default="auto",
        help="'auto' (cuda->mps->cpu) or an explicit 'cuda'/'mps'/'cpu'",
    )
    parser.add_argument("--seed", type=int, default=0xBA283)
    parser.add_argument("--clip-norm", type=float, default=5.0)
    parser.add_argument("--weight-decay", type=float, default=0.0)
    parser.add_argument(
        "--stft-weight", type=float, default=0.0,
        help="weight of the MR-STFT loss term (0 = pure pre-emphasised ESR; "
        "STFT is MPS-fussy, keep low/0 unless it helps)",
    )
    parser.add_argument("--smoke", action="store_true", help="short finite-loss smoke run")
    parser.add_argument("--smoke-steps", type=int, default=200)
    args = parser.parse_args()

    # The WaveNet RF is a fixed property of the Task-1 config (6347); derive the
    # window from n_out so the supervised region is non-empty by construction.
    rf = build_cond_wavenet(seed=args.seed).receptive_field
    window = args.window if args.window is not None else rf + args.n_out
    stride = args.stride if args.stride is not None else args.n_out

    device = resolve_device(args.device)
    train_main(
        train_manifest=args.train_manifest,
        val_manifest=args.val_manifest,
        epochs=args.epochs,
        batch_size=args.batch,
        lr=args.lr,
        window=window,
        stride=stride,
        num_workers=args.workers,
        checkpoint_dir=args.checkpoint_dir,
        log_dir=args.log_dir,
        device=device,
        seed=args.seed,
        clip_norm=args.clip_norm,
        weight_decay=args.weight_decay,
        stft_weight=args.stft_weight,
        smoke=args.smoke,
        smoke_steps=args.smoke_steps,
    )


if __name__ == "__main__":
    main()
