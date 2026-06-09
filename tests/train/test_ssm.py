# tests/train/test_ssm.py
import pytest
import torch

from train.factory import MODEL_TYPES, make_model
from train.ssm import BA283SSM

SSM_KINDS = ("s4d", "s6")


@pytest.mark.parametrize("kind", SSM_KINDS)
def test_forward_output_shape(kind):
    model = make_model(kind, hidden=16, num_layers=2, d_state=8)
    x = torch.randn(3, 64, 2)
    y, state = model(x)
    assert y.shape == (3, 64, 1)
    assert isinstance(state, tuple) and len(state) > 0
    # State must be a flat tuple of plain tensors (TBPTT .detach() contract).
    for s in state:
        assert isinstance(s, torch.Tensor)
        s.detach()


@pytest.mark.parametrize("kind", SSM_KINDS)
def test_state_round_trip_continues_sequence(kind):
    # Running the full sequence in one pass must equal running it in two halves
    # while threading the carried state across the split.
    torch.manual_seed(0)
    model = make_model(kind, hidden=12, num_layers=1, d_state=8)
    model.train(False)
    x = torch.randn(2, 40, 2)
    with torch.no_grad():
        full, _ = model(x)
        first, st = model(x[:, :20])
        second, _ = model(x[:, 20:], st)
    joined = torch.cat([first, second], dim=1)
    assert torch.allclose(full, joined, atol=1e-5), f"{kind} state did not thread"


@pytest.mark.parametrize("kind", SSM_KINDS)
def test_seed_deterministic(kind):
    torch.manual_seed(123)
    a = make_model(kind, hidden=12, num_layers=1, d_state=8)
    torch.manual_seed(123)
    b = make_model(kind, hidden=12, num_layers=1, d_state=8)
    x = torch.randn(1, 32, 2)
    with torch.no_grad():
        ya, _ = a(x)
        yb, _ = b(x)
    assert torch.equal(ya, yb)


@pytest.mark.parametrize("kind", SSM_KINDS)
def test_output_is_finite(kind):
    model = make_model(kind, hidden=16, num_layers=2, d_state=8)
    x = torch.zeros(1, 24, 2)
    x[..., 0] = 0.5
    with torch.no_grad():
        y, _ = model(x)
    assert torch.all(torch.isfinite(y))


@pytest.mark.parametrize("kind", SSM_KINDS)
def test_film_modulation_applied(kind):
    # FiLM is wired in: different gains must produce different outputs (the
    # gain channel is not ignored).
    torch.manual_seed(7)
    model = BA283SSM(kind, hidden=12, num_layers=1, d_state=8, conditioning="film")
    model.train(False)
    audio = torch.randn(1, 24, 1)
    lo = torch.cat([audio, torch.zeros(1, 24, 1)], dim=-1)
    hi = torch.cat([audio, torch.ones(1, 24, 1)], dim=-1)
    with torch.no_grad():
        ylo, _ = model(lo)
        yhi, _ = model(hi)
    assert not torch.allclose(ylo, yhi)


def test_concat_conditioning_runs():
    model = make_model("s4d", hidden=12, num_layers=1, d_state=8, conditioning="concat")
    x = torch.randn(2, 16, 2)
    y, _ = model(x)
    assert y.shape == (2, 16, 1)


def test_lstm_path_unchanged():
    # Factory must build the exact LSTM the loop always built (regression).
    from train.model import BA283LSTM

    m = make_model("lstm", hidden=32)
    assert isinstance(m, BA283LSTM)
    assert "lstm" in MODEL_TYPES


def test_unknown_model_type_raises():
    with pytest.raises(ValueError):
        make_model("transformer", hidden=16)
