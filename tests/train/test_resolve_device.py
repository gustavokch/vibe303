"""Unit tests for ``train.train.resolve_device`` backend precedence.

``resolve_device`` is a pure function of two availability probes
(``torch.cuda.is_available`` / ``torch.backends.mps.is_available``); patching
both locks the ``auto`` precedence (cuda -> mps -> cpu) and the explicit-name
pass-through without needing real CUDA/MPS hardware.
"""

import pytest

import train.train as T


def _patch(monkeypatch, *, cuda: bool, mps: bool) -> None:
    monkeypatch.setattr(T.torch.cuda, "is_available", lambda: cuda)
    monkeypatch.setattr(T.torch.backends.mps, "is_available", lambda: mps)


def test_auto_prefers_cuda(monkeypatch):
    _patch(monkeypatch, cuda=True, mps=True)
    assert T.resolve_device("auto") == "cuda"


def test_auto_falls_to_mps(monkeypatch):
    _patch(monkeypatch, cuda=False, mps=True)
    assert T.resolve_device("auto") == "mps"


def test_auto_falls_to_cpu(monkeypatch):
    _patch(monkeypatch, cuda=False, mps=False)
    assert T.resolve_device("auto") == "cpu"


def test_explicit_overrides_autodetect(monkeypatch):
    # An explicit name wins even when a faster backend is available.
    _patch(monkeypatch, cuda=True, mps=True)
    assert T.resolve_device("cpu") == "cpu"
    assert T.resolve_device("mps") == "mps"
    assert T.resolve_device("cuda") == "cuda"


def test_rejects_unknown_name():
    with pytest.raises(ValueError, match="--device must be"):
        T.resolve_device("mpz")
