//! Hand-coded pure-Rust LSTM(1, H)+FiLM+Dense(1) matching PyTorch nn.LSTM exactly.
//! Parses the self-describing `ba283-lstm-v1` JSON (see train/export.py).
//! forward() is zero-alloc: fixed `[f32; HIDDEN_MAX]` arrays, no Vec/heap in the audio path.
//! The actual hidden width `H` is read from the JSON (`<= HIDDEN_MAX`); the embedded placeholder is
//! 32-wide, the real Colab-trained export is 48-wide — both load into the same fixed storage.

use serde_json::Value;

/// Largest hidden width supported by the fixed-size storage. Covers the 32-wide placeholder and the
/// 48-wide trained model with headroom; bump (and rebuild) if a wider model is exported.
const HIDDEN_MAX: usize = 64;
const INPUT: usize = 2; // [audio, gain]
const GATES_MAX: usize = 4 * HIDDEN_MAX; // i, f, g, o stacked
const LSTM_IN_MAX: usize = 2; // film -> 1 col used, concat -> 2

/// LSTM(1|2, H) + optional FiLM + Dense(1) with fixed-size weight storage (sized to `HIDDEN_MAX`,
/// the actual width `self.hidden` used per call).
///
/// Gate ordering follows PyTorch `nn.LSTM` (`gate_order: "ifgo"`): the `4H` rows of the gate tensors
/// are input i = [0..H], forget f = [H..2H], cell g = [2H..3H], output o = [3H..4H].
///
/// FiLM conditioning (`conditioning: "film"`): the LSTM sees only the audio channel
/// (`lstm_input_size = 1`) and the gain channel drives a `Linear(1, 2H)` affine `(gamma, beta)`
/// applied to the LSTM output before the dense layer. Concat conditioning (`"concat"`): gain is the
/// 2nd LSTM input channel (`lstm_input_size = 2`) and there is no FiLM layer.
pub struct Ba283Model {
    w_ih: [[f32; LSTM_IN_MAX]; GATES_MAX], // 4H x (1|2); unused rows/cols stay 0
    w_hh: [[f32; HIDDEN_MAX]; GATES_MAX],  // 4H x H
    b_ih: [f32; GATES_MAX],                // 4H
    b_hh: [f32; GATES_MAX],                // 4H
    film_w: [f32; 2 * HIDDEN_MAX],         // 2H (the [2H,1] weight flattened)
    film_b: [f32; 2 * HIDDEN_MAX],         // 2H
    w_dense: [f32; HIDDEN_MAX],            // H
    b_dense: f32,
    h: [f32; HIDDEN_MAX], // hidden state
    c: [f32; HIDDEN_MAX], // cell state
    /// Actual hidden width parsed from the weights JSON (`<= HIDDEN_MAX`).
    hidden: usize,
    lstm_input_size: usize,
    film: bool,
    /// Training-time input pre-scale (`x[:,0] = dry * input_scale`). Documented in the de-norm
    /// contract; NOT applied inside `forward` (the LSTM weights already bake the trained domain).
    /// Exposed so the host chain can reason about the contract — see `dsp.rs`.
    input_scale: f32,
    /// Reference voltage the training target was divided by (`target = wet_volts / output_ref_v`).
    /// The host chain multiplies `forward()` output by this to recover volts.
    output_ref_v: f32,
}

#[inline(always)]
fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

impl Ba283Model {
    /// Construct from the JSON weights embedded at build time.
    ///
    /// The embedded weights are a build-time invariant; any shape/format
    /// mismatch panics with a clear message rather than failing silently.
    pub fn new_from_embedded_weights() -> Self {
        const WEIGHTS_JSON: &str = include_str!("../weights/ba283_lstm32.json");
        Self::from_json_str(WEIGHTS_JSON)
    }

    /// Construct from a weights JSON file at runtime (offline tools / A-B harnesses that need a
    /// different export than the embedded one, e.g. the 48-wide trained model). Panics on read or
    /// parse failure — callers are dev tools, not the realtime audio path.
    pub fn from_json_path(path: &str) -> Self {
        let s = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("weights: cannot read {path}: {e}"));
        Self::from_json_str(&s)
    }

    fn from_json_str(s: &str) -> Self {
        let doc: Value = serde_json::from_str(s).expect("embedded weights: invalid JSON");

        let format = doc["format"].as_str().expect("weights: missing `format`");
        assert_eq!(
            format, "ba283-lstm-v1",
            "weights: unexpected format `{format}`"
        );
        let input_size = doc["input_size"]
            .as_u64()
            .expect("weights: missing `input_size`");
        assert_eq!(
            input_size, INPUT as u64,
            "weights: input_size must be {INPUT}"
        );
        let hidden = doc["hidden_size"]
            .as_u64()
            .expect("weights: missing `hidden_size`") as usize;
        assert!(
            (1..=HIDDEN_MAX).contains(&hidden),
            "weights: hidden_size {hidden} out of range 1..={HIDDEN_MAX}"
        );
        let gates = 4 * hidden;
        let gate_order = doc["gate_order"]
            .as_str()
            .expect("weights: missing `gate_order`");
        assert_eq!(gate_order, "ifgo", "weights: gate_order must be `ifgo`");

        let conditioning = doc["conditioning"].as_str().unwrap_or("concat");
        let film = conditioning == "film";

        // De-norm contract constants (see train/dataset.py + data_gen/stimulus.py). Default to the
        // canonical values if an older weights file omits them.
        let input_scale = doc["input_scale"].as_f64().unwrap_or(2.0) as f32;
        let output_ref_v = doc["output_ref_v"].as_f64().unwrap_or(64.0) as f32;
        let lstm_input_size = doc["lstm_input_size"]
            .as_u64()
            .expect("weights: missing `lstm_input_size`") as usize;
        assert!(
            (1..=LSTM_IN_MAX).contains(&lstm_input_size),
            "weights: lstm_input_size {lstm_input_size} out of range 1..={LSTM_IN_MAX}"
        );

        let layers = doc["layers"]
            .as_array()
            .expect("weights: `layers` must be an array");
        let lstm = layers
            .iter()
            .find(|l| l["type"].as_str() == Some("lstm"))
            .expect("weights: missing `lstm` layer");
        let dense = layers
            .iter()
            .find(|l| l["type"].as_str() == Some("dense"))
            .expect("weights: missing `dense` layer");

        let mut model = Self {
            w_ih: [[0.0; LSTM_IN_MAX]; GATES_MAX],
            w_hh: [[0.0; HIDDEN_MAX]; GATES_MAX],
            b_ih: [0.0; GATES_MAX],
            b_hh: [0.0; GATES_MAX],
            film_w: [0.0; 2 * HIDDEN_MAX],
            film_b: [0.0; 2 * HIDDEN_MAX],
            w_dense: [0.0; HIDDEN_MAX],
            b_dense: 0.0,
            h: [0.0; HIDDEN_MAX],
            c: [0.0; HIDDEN_MAX],
            hidden,
            lstm_input_size,
            film,
            input_scale,
            output_ref_v,
        };

        // LSTM weight_ih: 4H x lstm_input_size
        let w_ih = lstm["weight_ih"].as_array().expect("lstm.weight_ih missing");
        assert_eq!(w_ih.len(), gates, "lstm.weight_ih rows must be {gates}");
        for (row, row_v) in w_ih.iter().enumerate() {
            let cols = row_v.as_array().expect("lstm.weight_ih row not an array");
            assert_eq!(
                cols.len(),
                lstm_input_size,
                "lstm.weight_ih row {row} must have {lstm_input_size} cols"
            );
            for (col, v) in cols.iter().enumerate() {
                model.w_ih[row][col] = as_f32(v, "lstm.weight_ih");
            }
        }

        // LSTM weight_hh: 4H x H
        let w_hh = lstm["weight_hh"].as_array().expect("lstm.weight_hh missing");
        assert_eq!(w_hh.len(), gates, "lstm.weight_hh rows must be {gates}");
        for (row, row_v) in w_hh.iter().enumerate() {
            let cols = row_v.as_array().expect("lstm.weight_hh row not an array");
            assert_eq!(
                cols.len(),
                hidden,
                "lstm.weight_hh row {row} must have {hidden} cols"
            );
            for (col, v) in cols.iter().enumerate() {
                model.w_hh[row][col] = as_f32(v, "lstm.weight_hh");
            }
        }

        // LSTM bias_ih / bias_hh: 4H each
        fill_vec(&lstm["bias_ih"], &mut model.b_ih, gates, "lstm.bias_ih");
        fill_vec(&lstm["bias_hh"], &mut model.b_hh, gates, "lstm.bias_hh");

        // FiLM layer: weight [2H, 1] -> flattened 2H, bias [2H].
        if film {
            let f = layers
                .iter()
                .find(|l| l["type"].as_str() == Some("film"))
                .expect("weights: conditioning=film but no `film` layer");
            let fw = f["weight"].as_array().expect("film.weight missing");
            assert_eq!(
                fw.len(),
                2 * hidden,
                "film.weight rows must be {}",
                2 * hidden
            );
            for (row, row_v) in fw.iter().enumerate() {
                let cols = row_v.as_array().expect("film.weight row not an array");
                assert_eq!(cols.len(), 1, "film.weight row {row} must have 1 col");
                model.film_w[row] = as_f32(&cols[0], "film.weight");
            }
            fill_vec(&f["bias"], &mut model.film_b, 2 * hidden, "film.bias");
        }

        // Dense weight: 1 x H
        let d_w = dense["weight"].as_array().expect("dense.weight missing");
        assert_eq!(d_w.len(), 1, "dense.weight must have 1 row");
        let d_row = d_w[0].as_array().expect("dense.weight row not an array");
        assert_eq!(
            d_row.len(),
            hidden,
            "dense.weight row must have {hidden} cols"
        );
        for (i, v) in d_row.iter().enumerate() {
            model.w_dense[i] = as_f32(v, "dense.weight");
        }

        // Dense bias: 1
        let d_b = dense["bias"].as_array().expect("dense.bias missing");
        assert_eq!(d_b.len(), 1, "dense.bias must have 1 element");
        model.b_dense = as_f32(&d_b[0], "dense.bias");

        model
    }

    /// Run one timestep. `x = [audio, gain]`. Zero-alloc: only fixed stack arrays.
    ///
    /// Implements PyTorch nn.LSTM exactly (gate_order `ifgo`):
    /// ```text
    /// i  = sigmoid(W_ii x + b_ii + W_hi h + b_hi)
    /// f  = sigmoid(W_if x + b_if + W_hf h + b_hf)
    /// g  = tanh   (W_ig x + b_ig + W_hg h + b_hg)
    /// o  = sigmoid(W_io x + b_io + W_ho h + b_ho)
    /// c' = f * c + i * g
    /// h' = o * tanh(c')
    /// ```
    /// FiLM mode then applies `z = (1 + gamma) * h' + beta` with
    /// `[gamma, beta] = film(gain)`; concat mode uses `z = h'`. Output is
    /// `y = W_dense z + b_dense`.
    pub fn forward(&mut self, x: &[f32; 2]) -> [f32; 1] {
        // LSTM input: film -> audio only (col 0); concat -> [audio, gain].
        let in0 = x[0];
        let in1 = if self.lstm_input_size > 1 { x[1] } else { 0.0 };
        let hidden = self.hidden;

        let mut new_h = [0.0f32; HIDDEN_MAX];
        for (k, h_slot) in new_h.iter_mut().enumerate().take(hidden) {
            // gate rows (stacked by H): i=[0..H], f=[H..2H], g=[2H..3H], o=[3H..4H]
            let i = sigmoid(self.gate(k, in0, in1));
            let f = sigmoid(self.gate(hidden + k, in0, in1));
            let g = self.gate(2 * hidden + k, in0, in1).tanh();
            let o = sigmoid(self.gate(3 * hidden + k, in0, in1));

            let c_new = f * self.c[k] + i * g;
            self.c[k] = c_new;
            *h_slot = o * c_new.tanh();
        }
        self.h = new_h;

        let g_in = x[1];
        let mut y = self.b_dense;
        for k in 0..hidden {
            let z = if self.film {
                let gamma = self.film_w[k] * g_in + self.film_b[k];
                let beta = self.film_w[hidden + k] * g_in + self.film_b[hidden + k];
                (1.0 + gamma) * self.h[k] + beta
            } else {
                self.h[k]
            };
            y += self.w_dense[k] * z;
        }

        [y]
    }

    /// Pre-activation gate value for row `row`: `W_i row · in + b_ih + W_h row · h + b_hh`.
    #[inline(always)]
    fn gate(&self, row: usize, in0: f32, in1: f32) -> f32 {
        let mut acc = self.b_ih[row] + self.b_hh[row];
        acc += self.w_ih[row][0] * in0;
        acc += self.w_ih[row][1] * in1;
        for (j, &hv) in self.h.iter().enumerate().take(self.hidden) {
            acc += self.w_hh[row][j] * hv;
        }
        acc
    }

    /// Zero the hidden and cell state.
    pub fn reset(&mut self) {
        self.h = [0.0; HIDDEN_MAX];
        self.c = [0.0; HIDDEN_MAX];
    }

    /// Training-time input pre-scale from the de-norm contract. NOT applied inside `forward`.
    #[inline]
    pub fn input_scale(&self) -> f32 {
        self.input_scale
    }

    /// Reference voltage to multiply `forward()` output by to recover volts (de-norm contract).
    #[inline]
    pub fn output_ref_v(&self) -> f32 {
        self.output_ref_v
    }
}

#[inline]
fn as_f32(v: &Value, field: &str) -> f32 {
    v.as_f64()
        .unwrap_or_else(|| panic!("{field}: non-numeric value")) as f32
}

fn fill_vec(v: &Value, dst: &mut [f32], len: usize, field: &str) {
    let arr = v
        .as_array()
        .unwrap_or_else(|| panic!("{field} missing or not an array"));
    assert_eq!(arr.len(), len, "{field} must have {len} elements");
    for (i, e) in arr.iter().enumerate() {
        dst[i] = as_f32(e, field);
    }
}
