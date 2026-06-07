import numpy as np
import torch

from eval.ablation.run_checkpoint_eval import load_model
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
