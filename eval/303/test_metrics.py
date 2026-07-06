import numpy as np
from metrics import rms_dbfs

def test_rms_dbfs():
    sig = np.ones(100) * 0.5
    assert abs(rms_dbfs(sig) - (-6.02)) < 0.1
