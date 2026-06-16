# test_train_wavenet.py
#
# Run from the ba283-vst3 repo's env (the `nam` package is the `eval` extra of
# that repo, while this code lives in ba283-nam-lstm). PYTHONPATH must include
# the ba283-nam-lstm root so `scratch.nam_sweep.*` and `train.*` resolve:
#
#   cd ~/Git/ba283-nam-lstm && PYTHONPATH=$PWD \
#     uv run --project ~/Git/ba283-vst3 --extra eval \
#     python -m pytest scratch/nam_sweep/test_train_wavenet.py -v
#
# Unit-tests the only unit-testable piece of the WaveNet training loop: the
# `loss_region` helper that aligns the model's VALID-mode output with the target.
import torch

from scratch.nam_sweep.train_wavenet import loss_region
from scratch.nam_sweep.wavenet_cond import build_cond_wavenet


def test_loss_region_aligns_valid_output_to_target_tail():
    # The inner WaveNet is VALID mode (no pad_start): forward maps
    # (B, 2, L) -> (B, 1, L - (rf - 1)). So pred is ALREADY trimmed by rf-1 on
    # the FRONT. `loss_region` must drop the target's first rf-1 samples so both
    # tensors are the same length and time-aligned (pred[t] corresponds to the
    # input window ending at target[t + rf - 1]).
    model = build_cond_wavenet(seed=0)
    rf = model.receptive_field
    assert rf == 6347, f"RF pinned by the model is {rf}, expected 6347"

    L = rf + 512  # window must exceed rf or the valid output is empty
    x = torch.randn(2, 2, L)
    pred = model(x)  # (B, 1, L - (rf - 1)) == (B, 1, 513)
    assert pred.shape[-1] == L - (rf - 1)

    target = torch.randn(2, 1, L)  # full-length target, channel-first
    pred_r, tgt_r = loss_region(pred, target, rf)

    # Both end up length L - (rf - 1) and equal length.
    assert pred_r.shape == tgt_r.shape
    assert pred_r.shape[-1] == L - (rf - 1)
    # pred is returned untouched (already valid); target trimmed on the FRONT.
    assert torch.equal(pred_r, pred)
    assert torch.equal(tgt_r, target[..., rf - 1:])


def test_loss_region_handles_pad_start_equal_length():
    # If a forward ever returns output == input length (pad_start mode), the
    # first rf-1 samples are warmup; `loss_region` then trims BOTH fronts by
    # rf-1. Exercise that branch directly with synthetic equal-length tensors.
    rf = 6347
    L = rf + 512
    pred = torch.randn(2, 1, L)  # same length as target -> pad_start branch
    target = torch.randn(2, 1, L)
    pred_r, tgt_r = loss_region(pred, target, rf)

    assert pred_r.shape == tgt_r.shape
    assert pred_r.shape[-1] == L - (rf - 1)
    assert torch.equal(pred_r, pred[..., rf - 1:])
    assert torch.equal(tgt_r, target[..., rf - 1:])
