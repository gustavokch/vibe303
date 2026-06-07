# tests/train/test_model.py
import torch

from train.model import BA283LSTM


def test_forward_output_shape():
    model = BA283LSTM(hidden=32)
    x = torch.randn(4, 4096, 2)
    y, (h, c) = model(x)
    assert y.shape == (4, 4096, 1)
    assert h.shape == (1, 4, 32)
    assert c.shape == (1, 4, 32)


def test_param_count_in_expected_range():
    # 1-layer LSTM-32 + FiLM(1->64) + Dense(32->1):
    #   lstm 4*(32*1 + 32*32 + 32 + 32) = 4352, film 64*1+64 = 128, dense 33 -> 4513.
    model = BA283LSTM(hidden=32)
    total = sum(p.numel() for p in model.parameters())
    assert 4000 < total < 5000


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
    model = BA283LSTM(hidden=32)
    x = torch.randn(2, 1024, 2)
    h0 = torch.zeros(1, 2, 32)
    c0 = torch.zeros(1, 2, 32)
    y, (h1, c1) = model(x, (h0, c0))
    assert y.shape == (2, 1024, 1)
    assert h1.shape == (1, 2, 32)
    assert c1.shape == (1, 2, 32)


def test_output_is_finite():
    model = BA283LSTM(hidden=32)
    x = torch.zeros(1, 16, 2)
    x[..., 0] = 0.5
    with torch.no_grad():
        y, _ = model(x)
    assert torch.all(torch.isfinite(y))
