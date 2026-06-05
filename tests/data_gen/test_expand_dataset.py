import importlib.util
from pathlib import Path

import numpy as np

_SCRIPT = Path(__file__).resolve().parents[2] / "scripts" / "expand_dataset.py"
_spec = importlib.util.spec_from_file_location("expand_dataset", _SCRIPT)
expand = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(expand)


def test_build_dense_covers_levels_and_is_bounded():
    segs = expand.build_dense(duration_scale=0.01)
    # dense sweep level grid present, including the loudest (clipping) level that
    # the old by-stimulus split exiled to val/test.
    assert "sweep_m1" in segs and "sweep_m24" in segs
    sweep_levels = sorted(int(k.split("_m")[1]) for k in segs if k.startswith("sweep_m"))
    assert len(sweep_levels) >= 8
    # every segment stays within the 0.5 V SPICE input ceiling, mono float32.
    for name, sig in segs.items():
        assert sig.ndim == 1 and sig.dtype == np.float32, name
        assert float(np.max(np.abs(sig))) <= 0.5 + 1e-6, name


def test_build_dense_names_unique_and_varied():
    segs = expand.build_dense(duration_scale=0.01)
    kinds = {k.split("_")[0].rstrip("0123456789") for k in segs}
    # multisine is intentionally excluded (collapses ngspice's gear timestep)
    assert {"sweep", "pink", "twotone", "impulse"} <= kinds
    assert not any(k.startswith("multisine") for k in segs)
