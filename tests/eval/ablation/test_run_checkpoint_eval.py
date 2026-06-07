import numpy as np
import torch

from eval.ablation.run_checkpoint_eval import load_model, make_predict
from train.dataset import INPUT_SCALE
from train.model import BA283GRU


def test_load_model_roundtrips_state(tmp_path):
    src = BA283GRU(hidden=8)
    ckpt = tmp_path / "best.pth"
    torch.save(src.state_dict(), ckpt)

    loaded = load_model(ckpt, hidden=8)

    assert loaded.hidden == 8
    assert loaded.conditioning == "film"
    assert not loaded.training  # eval mode
    src_state = src.state_dict()
    loaded_state = loaded.state_dict()
    assert set(src_state) == set(loaded_state)
    for k in src_state:
        assert torch.equal(src_state[k], loaded_state[k])


class _FakeModel:
    """Records the forward input tensor and echoes its audio column."""

    def __init__(self):
        self.seen = None

    def __call__(self, x, h=None):
        self.seen = x
        return x[..., :1], None


def test_make_predict_scales_input_and_trims_warmup():
    warmup = 5
    fake = _FakeModel()
    predict = make_predict(fake, input_scale=INPUT_SCALE, warmup=warmup)

    dry = np.linspace(-0.3, 0.3, 20, dtype=np.float64)
    gain = 0.5
    out = predict(dry, gain)

    x = fake.seen
    assert x.shape == (1, 20, 2)
    np.testing.assert_allclose(x[0, :, 0].numpy(), dry * INPUT_SCALE, rtol=1e-6)
    np.testing.assert_allclose(x[0, :, 1].numpy(), gain, rtol=1e-6)
    assert out.ndim == 1
    assert len(out) == len(dry) - warmup
    # echo of scaled audio, post-warmup
    np.testing.assert_allclose(out, (dry * INPUT_SCALE)[warmup:], rtol=1e-6)
