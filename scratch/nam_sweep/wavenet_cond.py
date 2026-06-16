"""
2-channel-input, FiLM-active WaveNet builder (conditioning spike).

Goal: a WaveNet whose input is 2 channels -- the dry audio plus a per-sample
gain/condition channel -- with FiLM modulation active so the condition channel
modulates features. This de-risks the conditioned-sweep training effort: it
proves such a model builds, forwards on (B, 2, L), exports to a `.nam`-style
dict, and reloads bit-identically.

Recipe discovered by inspecting the installed `nam` fork (neural-amp-modeler
0.13.0, the `eval` extra of ba283-vst3):

  * The ONLY reliable constructor is ``nam.models.init_from_nam(full_dict)``;
    it dispatches on ``architecture`` and (for WaveNet) builds via
    ``_init_wavenet`` -> ``_WaveNet.init_from_config`` -> ``import_weights``.
    ``init_from_nam`` returns the *public wrapper* ``_wavenet_wrapper.WaveNet``
    (a ``BaseNet``). That wrapper's ``forward`` is single-channel only: it pads
    the start and asserts a 1-channel output, so it CANNOT forward a (B, 2, L)
    tensor. The object that supports 2-channel I/O -- and that training/export
    use -- is the *inner* ``_wavenet.WaveNet`` at ``wrapper._net``. Its
    ``forward`` is a valid (no-pad) conv: (B, Cx, L) -> (B, Cy, L - (RF - 1)),
    and it exposes ``receptive_field`` / ``export_config`` / ``export_weights``
    / ``import_weights`` directly. So we build via ``init_from_nam`` then return
    ``._net``.

  * Base config: the shipped ``model.nam`` is a ``SlimmableContainer`` whose
    ``config.submodels[0].model`` is a complete WaveNet ``.nam`` dict (the
    proven-good config, RF = 6347, ``slimmable = None``). We deep-copy it and:
        - set every layer's ``input_size`` and ``condition_size`` to 2;
        - activate ``conv_pre_film`` on every layer (FiLM on the layer input,
          conditioned on the 2-channel condition tensor).
    The first layer's ``input_size`` widens the input rechannel conv (1 -> 2),
    so the model genuinely consumes a 2-channel input. With ``condition_dsp``
    absent, the inner WaveNet sets condition ``c = x`` (the full 2-channel
    input), so the gain channel reaches every FiLM hook.

  * Weights-array problem: changing ``input_size`` / ``condition_size`` and
    adding FiLM changes the layer shapes, so the old flat weights array no
    longer fits. Resolution (the spike's deliverable): drop ``weights``, probe
    the required flat length by building once with an over-long zero array
    (``import_weights`` only consumes the prefix it needs), read
    ``export_weights().numel()``, then build for real with a freshly-initialised
    array of exactly that length. Training fills these weights; here small
    random values make the round-trip non-degenerate.
"""

import copy
import json
from pathlib import Path

import torch
from nam.models import init_from_nam

REF_NAM = Path.home() / "Git/ba283-nam-lstm/exported_models/model.nam"

# Over-long zero array used only to probe the required flat-weight length.
# `import_weights` indexes a prefix of the array, so any length >= the true
# count works; this is generously above the ~2.4k a single submodel needs.
_PROBE_LEN = 200_000


def _cond_config() -> dict:
    """Deep-copy submodel 0's WaveNet `.nam` dict and lift it to 2-ch + FiLM.

    Returns a complete `.nam`-style dict (version / architecture / metadata /
    config) with ``weights`` removed -- the caller supplies a correctly-sized
    weights array.
    """
    sm = json.loads(REF_NAM.read_text())["config"]["submodels"][0]["model"]
    cfg = copy.deepcopy(sm)  # {version, architecture, metadata, config, weights, ...}
    for layer in cfg["config"]["layers"]:
        layer["input_size"] = 2
        layer["condition_size"] = 2
        layer["conv_pre_film"] = {"active": True, "shift": True, "groups": 1}
    cfg.pop("weights", None)  # shapes changed; supplied separately
    return cfg


def _weights_numel(cfg: dict) -> int:
    """Build once with an over-long zero weights array to learn the exact
    flat-weight count for this (2-ch, FiLM-on) config."""
    probe = copy.deepcopy(cfg)
    probe["weights"] = [0.0] * _PROBE_LEN
    n = int(net_from_nam(probe).export_weights().shape[0])
    # Drain guard: the probe array must NOT be fully consumed, or the true count
    # could exceed _PROBE_LEN and `import_weights` would silently truncate. A
    # strict inequality proves the prefix was long enough for this config.
    assert n < _PROBE_LEN, (
        f"probe array fully drained (consumed {n} >= _PROBE_LEN={_PROBE_LEN}); "
        f"the weight count may be truncated -- raise _PROBE_LEN"
    )
    return n


def net_from_nam(nam_dict: dict):
    """Build via the proven ``init_from_nam`` entrypoint and return the inner
    ``_wavenet.WaveNet`` (the multi-channel-capable net), unwrapping the public
    wrapper's ``._net``."""
    model = init_from_nam(copy.deepcopy(nam_dict))
    return model._net if hasattr(model, "_net") else model


def build_cond_wavenet(seed: int = 0):
    """Build a 2-channel-input, FiLM-active inner WaveNet with freshly
    initialised weights.

    :param seed: RNG seed for the placeholder weights (training overwrites them).
    :return: the inner ``_wavenet.WaveNet`` (``receptive_field`` ~= 6347,
        ``forward``: (B, 2, L) -> (B, 1, L - (RF - 1))).
    """
    cfg = _cond_config()
    n = _weights_numel(cfg)
    gen = torch.Generator().manual_seed(seed)
    cfg["weights"] = (0.05 * torch.randn(n, generator=gen)).tolist()
    return net_from_nam(cfg)


def to_nam_dict(model, sample_rate: float = 48000) -> dict:
    """Serialise an inner WaveNet (or wrapper) to a complete `.nam`-style dict
    that ``init_from_nam`` can reload."""
    export_config = (
        model.export_config()
        if hasattr(model, "export_config")
        else model.net.export_config()
    )
    export_weights = (
        model.export_weights()
        if hasattr(model, "export_weights")
        else model.net.export_weights()
    )
    return {
        "version": "0.7.0",
        "architecture": "WaveNet",
        "sample_rate": sample_rate,
        "metadata": {},
        "config": export_config,
        "weights": export_weights.tolist(),
    }
