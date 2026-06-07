# tests/train/test_export.py
import json

from train.export import export_rtneural_json
from train.model import BA283LSTM


def test_exported_json_has_expected_layers(tmp_path):
    model = BA283LSTM(hidden=32)
    out = tmp_path / "ba283.json"
    export_rtneural_json(model, out, meta={"sample_rate": 96000, "input_scale": 2.0})
    text = out.read_text().lower()
    assert "lstm" in text
    assert "dense" in text


def test_exported_film_layer_present_with_shapes(tmp_path):
    model = BA283LSTM(hidden=32)
    out = tmp_path / "ba283.json"
    export_rtneural_json(model, out, meta={"sample_rate": 96000})
    doc = json.loads(out.read_text())
    assert doc["format"] == "ba283-lstm-v1"
    assert doc["input_size"] == 2 and doc["lstm_input_size"] == 1
    film = next(layer for layer in doc["layers"] if layer["type"] == "film")
    assert film["shapes"] == {"weight": [64, 1], "bias": [64]}
    assert len(film["weight"]) == 64 and len(film["weight"][0]) == 1
    assert len(film["bias"]) == 64


def test_exported_concat_has_no_film_and_lstm_input_2(tmp_path):
    model = BA283LSTM(hidden=32, conditioning="concat")
    out = tmp_path / "ba283.json"
    export_rtneural_json(model, out, meta={"sample_rate": 96000})
    doc = json.loads(out.read_text())
    assert doc["conditioning"] == "concat"
    assert doc["lstm_input_size"] == 2
    assert not any(layer["type"] == "film" for layer in doc["layers"])


def test_exported_lstm_layer_shapes_are_4h(tmp_path):
    # LSTM has 4 gates -> weight/bias rows are 4H (vs the old GRU's 3H).
    model = BA283LSTM(hidden=32)
    out = tmp_path / "ba283.json"
    export_rtneural_json(model, out, meta={"sample_rate": 96000})
    doc = json.loads(out.read_text())
    assert doc["gate_order"] == "ifgo"
    lstm = next(layer for layer in doc["layers"] if layer["type"] == "lstm")
    assert lstm["shapes"]["weight_ih"] == [128, 1]
    assert lstm["shapes"]["weight_hh"] == [128, 32]
    assert lstm["shapes"]["bias_ih"] == [128]
    assert lstm["shapes"]["bias_hh"] == [128]


def test_exported_weights_byte_count_in_expected_range(tmp_path):
    model = BA283LSTM(hidden=32)
    out = tmp_path / "ba283.json"
    export_rtneural_json(model, out, meta={"sample_rate": 96000, "input_scale": 2.0})
    data = json.loads(out.read_text())
    flat_total = sum(_count_floats(v) for v in _walk(data))
    # 4 gates: weight_ih (4H x 1) + weight_hh (4H x H) + bias_ih (4H) + bias_hh
    # (4H) + film (2H x 1) + film bias (2H) + dense (H) + dense bias (1).
    expected_total = 4 * (32 * 1) + 4 * (32 * 32) + 4 * 32 + 4 * 32 + (64 + 64) + 32 + 1
    assert flat_total >= expected_total


def _walk(node):
    if isinstance(node, list):
        yield node
        for v in node:
            yield from _walk(v)
    elif isinstance(node, dict):
        for v in node.values():
            yield from _walk(v)


def _count_floats(node):
    if isinstance(node, list):
        if node and all(isinstance(x, (int, float)) for x in node):
            return len(node)
        return sum(_count_floats(v) for v in node)
    return 0
