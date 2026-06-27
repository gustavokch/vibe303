"""F6.1 gate: the committed ngspice resonance fixture must exist and carry the
VCF_MAP §11 fc-tracking shape (peak rises with bias, envelope 532 Hz→2.7 kHz)."""
import json
import pathlib


def test_reso_gold_tracks_bias():
    g = json.loads((pathlib.Path(__file__).parent / "vcf303_reso_gold.json").read_text())
    pts = g["peak_hz_by_itail_ua"]  # {"16": .., "32": .., "64": .., "128": ..}
    seq = [pts["16"], pts["32"], pts["64"], pts["128"]]
    assert all(b > a for a, b in zip(seq, seq[1:])), "peak must rise with bias"
    assert 400 < pts["16"] < 700 and 2000 < pts["128"] < 3500  # §11 envelope
