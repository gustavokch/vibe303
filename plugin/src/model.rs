//! Hand-coded pure-Rust LSTM(1,32)+FiLM+Dense(1) matching PyTorch nn.LSTM exactly.
//! Parses the self-describing `ba283-lstm-v1` JSON (see train/export.py).
//! forward() is zero-alloc: fixed [f32; 32] arrays, no Vec/heap in the audio path.

use serde_json::Value;

const HIDDEN: usize = 32;
const INPUT: usize = 2; // [audio, gain]
const GATES: usize = 4 * HIDDEN; // 128 (i, f, g, o)
const LSTM_IN_MAX: usize = 2; // film -> 1 col used, concat -> 2

/// LSTM(1|2, 32) + optional FiLM + Dense(1) with fixed-size weight storage.
///
/// Gate ordering follows PyTorch `nn.LSTM` (`gate_order: "ifgo"`): the 128 rows
/// of the gate tensors are input i = [0..32], forget f = [32..64], cell g =
/// [64..96], output o = [96..128].
///
/// FiLM conditioning (`conditioning: "film"`): the LSTM sees only the audio
/// channel (`lstm_input_size = 1`) and the gain channel drives a `Linear(1, 2H)`
/// affine `(gamma, beta)` applied to the LSTM output before the dense layer.
/// Concat conditioning (`"concat"`): gain is the 2nd LSTM input channel
/// (`lstm_input_size = 2`) and there is no FiLM layer.
pub struct Ba283Model {
    w_ih: [[f32; LSTM_IN_MAX]; GATES], // 128 x (1|2); unused cols stay 0
    w_hh: [[f32; HIDDEN]; GATES],      // 128 x 32
    b_ih: [f32; GATES],                // 128
    b_hh: [f32; GATES],                // 128
    film_w: [f32; 2 * HIDDEN],         // 64 (the [64,1] weight flattened)
    film_b: [f32; 2 * HIDDEN],         // 64
    w_dense: [f32; HIDDEN],            // 32
    b_dense: f32,
    h: [f32; HIDDEN], // hidden state
    c: [f32; HIDDEN], // cell state
    lstm_input_size: usize,
    film: bool,
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
        let hidden_size = doc["hidden_size"]
            .as_u64()
            .expect("weights: missing `hidden_size`");
        assert_eq!(
            hidden_size, HIDDEN as u64,
            "weights: hidden_size must be {HIDDEN}"
        );
        let gate_order = doc["gate_order"]
            .as_str()
            .expect("weights: missing `gate_order`");
        assert_eq!(gate_order, "ifgo", "weights: gate_order must be `ifgo`");

        let conditioning = doc["conditioning"].as_str().unwrap_or("concat");
        let film = conditioning == "film";
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
            w_ih: [[0.0; LSTM_IN_MAX]; GATES],
            w_hh: [[0.0; HIDDEN]; GATES],
            b_ih: [0.0; GATES],
            b_hh: [0.0; GATES],
            film_w: [0.0; 2 * HIDDEN],
            film_b: [0.0; 2 * HIDDEN],
            w_dense: [0.0; HIDDEN],
            b_dense: 0.0,
            h: [0.0; HIDDEN],
            c: [0.0; HIDDEN],
            lstm_input_size,
            film,
        };

        // LSTM weight_ih: 128 x lstm_input_size
        let w_ih = lstm["weight_ih"].as_array().expect("lstm.weight_ih missing");
        assert_eq!(w_ih.len(), GATES, "lstm.weight_ih rows must be {GATES}");
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

        // LSTM weight_hh: 128 x 32
        let w_hh = lstm["weight_hh"].as_array().expect("lstm.weight_hh missing");
        assert_eq!(w_hh.len(), GATES, "lstm.weight_hh rows must be {GATES}");
        for (row, row_v) in w_hh.iter().enumerate() {
            let cols = row_v.as_array().expect("lstm.weight_hh row not an array");
            assert_eq!(
                cols.len(),
                HIDDEN,
                "lstm.weight_hh row {row} must have {HIDDEN} cols"
            );
            for (col, v) in cols.iter().enumerate() {
                model.w_hh[row][col] = as_f32(v, "lstm.weight_hh");
            }
        }

        // LSTM bias_ih / bias_hh: 128 each
        fill_vec(&lstm["bias_ih"], &mut model.b_ih, GATES, "lstm.bias_ih");
        fill_vec(&lstm["bias_hh"], &mut model.b_hh, GATES, "lstm.bias_hh");

        // FiLM layer: weight [2H, 1] -> flattened 64, bias [2H] = 64.
        if film {
            let f = layers
                .iter()
                .find(|l| l["type"].as_str() == Some("film"))
                .expect("weights: conditioning=film but no `film` layer");
            let fw = f["weight"].as_array().expect("film.weight missing");
            assert_eq!(
                fw.len(),
                2 * HIDDEN,
                "film.weight rows must be {}",
                2 * HIDDEN
            );
            for (row, row_v) in fw.iter().enumerate() {
                let cols = row_v.as_array().expect("film.weight row not an array");
                assert_eq!(cols.len(), 1, "film.weight row {row} must have 1 col");
                model.film_w[row] = as_f32(&cols[0], "film.weight");
            }
            fill_vec(&f["bias"], &mut model.film_b, 2 * HIDDEN, "film.bias");
        }

        // Dense weight: 1 x 32
        let d_w = dense["weight"].as_array().expect("dense.weight missing");
        assert_eq!(d_w.len(), 1, "dense.weight must have 1 row");
        let d_row = d_w[0].as_array().expect("dense.weight row not an array");
        assert_eq!(
            d_row.len(),
            HIDDEN,
            "dense.weight row must have {HIDDEN} cols"
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

        let mut new_h = [0.0f32; HIDDEN];
        for (k, h_slot) in new_h.iter_mut().enumerate() {
            // gate rows: i=[0..32], f=[32..64], g=[64..96], o=[96..128]
            let i = sigmoid(self.gate(k, in0, in1));
            let f = sigmoid(self.gate(HIDDEN + k, in0, in1));
            let g = self.gate(2 * HIDDEN + k, in0, in1).tanh();
            let o = sigmoid(self.gate(3 * HIDDEN + k, in0, in1));

            let c_new = f * self.c[k] + i * g;
            self.c[k] = c_new;
            *h_slot = o * c_new.tanh();
        }
        self.h = new_h;

        let g_in = x[1];
        let mut y = self.b_dense;
        for k in 0..HIDDEN {
            let z = if self.film {
                let gamma = self.film_w[k] * g_in + self.film_b[k];
                let beta = self.film_w[HIDDEN + k] * g_in + self.film_b[HIDDEN + k];
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
        for (j, &hv) in self.h.iter().enumerate() {
            acc += self.w_hh[row][j] * hv;
        }
        acc
    }

    /// Zero the hidden and cell state.
    pub fn reset(&mut self) {
        self.h = [0.0; HIDDEN];
        self.c = [0.0; HIDDEN];
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
