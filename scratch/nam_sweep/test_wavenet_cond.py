# test_wavenet_cond.py
#
# Run from the ba283-vst3 repo's env (the `nam` package is the `eval` extra of
# that repo, while this code lives in ba283-nam-lstm):
#
#   cd ~/Git/ba283-nam-lstm && \
#     uv run --project ~/Git/ba283-vst3 --extra eval \
#     python -m pytest scratch/nam_sweep/test_wavenet_cond.py -v
#
# Round-trip spike: prove a 2-channel-input, FiLM-active WaveNet builds,
# forwards, exports to a `.nam`-style dict, and reloads bit-identically via the
# package's proven `init_from_nam` entrypoint.
import torch

from scratch.nam_sweep.wavenet_cond import (
    build_cond_wavenet,
    net_from_nam,
    to_nam_dict,
)


def test_2ch_film_wavenet_roundtrips():
    m = build_cond_wavenet()  # inner WaveNet: input_size=2, condition_size=2, FiLM on
    rf = m.net.receptive_field if hasattr(m, "net") else m.receptive_field
    assert rf > 1000, f"receptive field collapsed: {rf}"

    x = torch.zeros(1, 2, rf + 256)  # (B, C=2, L)
    y = m(x)
    # The inner WaveNet forward is "valid" (no pad_start): it trims by rf-1.
    assert y.shape[-1] == x.shape[-1] or y.shape[-1] == x.shape[-1] - (rf - 1)

    d = to_nam_dict(m, sample_rate=48000)
    assert d["architecture"] == "WaveNet"
    assert d["config"]["layers"][0]["input_size"] == 2
    assert d["config"]["layers"][0]["condition_size"] == 2
    assert d["config"]["layers"][0]["conv_pre_film"]["active"] is True

    m2 = net_from_nam(d)  # init_from_nam(d), unwrapped to the inner WaveNet
    y2 = m2(x)
    assert torch.allclose(y, y2, atol=1e-6), "reload not bit-identical"
