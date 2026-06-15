"""Equivalence + autograd tests for the MPS guard in ``train.loss._stft_mag``.

CI has no MPS device, so the guard's *mps* branch cannot run here. What CAN be
proven on CPU is the contract the guard relies on: the CPU<->device round-trip
(``x.to(stft_dev)`` ... ``spec.abs().to(dev)``) is value-preserving and keeps
autograd intact. On a CPU tensor ``stft_dev == dev`` so the round-trip is an
identity; these tests lock that the ``.to()`` plumbing added for MPS does not
perturb values or break the backward pass on the unchanged CPU/CUDA path.
"""

import torch

from train.loss import _stft_mag, mrstft


def test_stft_mag_cpu_is_finite_and_deterministic():
    x = torch.randn(2, 4096)
    out = _stft_mag(x, n_fft=1024)
    assert out.device.type == "cpu"
    assert torch.isfinite(out).all()
    # Re-running must give a bit-identical result: no in-place device mutation,
    # no nondeterministic round-trip.
    assert torch.equal(out, _stft_mag(x, n_fft=1024))


def test_stft_mag_grad_flows_across_roundtrip():
    # The MPS fix relies on autograd crossing the device boundary. On CPU the
    # boundary is a no-op, but this still exercises that the .to()/abs()/clamp
    # chain is differentiable and yields finite gradients.
    x = torch.randn(2, 4096, requires_grad=True)
    _stft_mag(x, n_fft=1024).sum().backward()
    assert x.grad is not None
    assert torch.isfinite(x.grad).all()


def test_mrstft_finite_and_differentiable():
    pred = torch.randn(2, 3072, 1, requires_grad=True)
    target = torch.randn(2, 3072, 1)
    loss = mrstft(pred, target)
    assert torch.isfinite(loss)
    loss.backward()
    assert torch.isfinite(pred.grad).all()
