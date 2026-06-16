# tests/train/test_model.py
import torch

from train.model import BA283LSTM


def test_forward_output_shape():
    # NAM-style default: 3 layers, hidden 18. State is (num_layers, batch, hidden).
    model = BA283LSTM()
    x = torch.randn(4, 4096, 2)
    y, (h, c) = model(x)
    assert y.shape == (4, 4096, 1)
    assert h.shape == (3, 4, 18)
    assert c.shape == (3, 4, 18)


def test_param_count_in_expected_range():
    # NAM-style 3-layer LSTM-18 + FiLM(1->36) + Dense(18->1) + learned h0/c0:
    #   lstm l0 4*(18*1 + 18*18 + 18 + 18) = 1440; l1,l2 each 4*(18*18 + 18*18 + 36)
    #   = 2736 -> 5472; film 36*1+36 = 72; dense 19; h0/c0 = 3*18*2 = 108. ~7111.
    model = BA283LSTM()
    total = sum(p.numel() for p in model.parameters())
    assert 6000 < total < 8500


def test_state_dict_keys_match_rtneural_expectations():
    model = BA283LSTM(hidden=32)
    keys = set(model.state_dict().keys())
    required = {
        "lstm.weight_ih_l0",
        "lstm.weight_hh_l0",
        "lstm.bias_ih_l0",
        "lstm.bias_hh_l0",
        "dense.weight",
        "dense.bias",
    }
    assert required <= keys, f"missing keys: {required - keys}"


def test_initial_state_can_be_passed():
    # State carries (num_layers, batch, hidden) per the 3-layer default.
    model = BA283LSTM()
    x = torch.randn(2, 1024, 2)
    h0 = torch.zeros(3, 2, 18)
    c0 = torch.zeros(3, 2, 18)
    y, (h1, c1) = model(x, (h0, c0))
    assert y.shape == (2, 1024, 1)
    assert h1.shape == (3, 2, 18)
    assert c1.shape == (3, 2, 18)


def test_learned_initial_state_trains():
    # h0/c0 are trainable parameters and appear in state_dict.
    model = BA283LSTM()
    keys = set(model.state_dict())
    assert {"h0", "c0"} <= keys
    assert model.h0.requires_grad and model.c0.requires_grad
    assert model.h0.shape == (3, 1, 18)


def test_output_is_finite():
    model = BA283LSTM(hidden=32)
    x = torch.zeros(1, 16, 2)
    x[..., 0] = 0.5
    with torch.no_grad():
        y, _ = model(x)
    assert torch.all(torch.isfinite(y))
