//! Hand-coded pure-Rust GRU(2,32)+Dense(1) matching PyTorch nn.GRU exactly.
//! Parses the self-describing `ba283-gru-v1` JSON (see train/export.py).
//! forward() is zero-alloc: fixed [f32; 32] arrays, no Vec/heap in the audio path.

use serde_json::Value;

const HIDDEN: usize = 32;
const INPUT: usize = 2;
const GATES: usize = 3 * HIDDEN; // 96

/// GRU(2,32) + Dense(1) with fixed-size weight storage.
///
/// Gate ordering follows PyTorch `nn.GRU` (`gate_order: "rzn"`): the 96 rows of
/// the gate tensors are reset r = [0..32], update z = [32..64], new n = [64..96].
pub struct Ba283Model {
    w_ih: [[f32; INPUT]; GATES],  // 96x2
    w_hh: [[f32; HIDDEN]; GATES], // 96x32
    b_ih: [f32; GATES],           // 96
    b_hh: [f32; GATES],           // 96
    w_dense: [f32; HIDDEN],       // 32
    b_dense: f32,
    h: [f32; HIDDEN], // hidden state
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
        const WEIGHTS_JSON: &str = include_str!("../weights/ba283_gru32.json");
        Self::from_json_str(WEIGHTS_JSON)
    }

    fn from_json_str(s: &str) -> Self {
        let doc: Value = serde_json::from_str(s).expect("embedded weights: invalid JSON");

        let format = doc["format"].as_str().expect("weights: missing `format`");
        assert_eq!(
            format, "ba283-gru-v1",
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
        assert_eq!(gate_order, "rzn", "weights: gate_order must be `rzn`");

        let layers = doc["layers"]
            .as_array()
            .expect("weights: `layers` must be an array");
        let gru = layers
            .iter()
            .find(|l| l["type"].as_str() == Some("gru"))
            .expect("weights: missing `gru` layer");
        let dense = layers
            .iter()
            .find(|l| l["type"].as_str() == Some("dense"))
            .expect("weights: missing `dense` layer");

        let mut model = Self {
            w_ih: [[0.0; INPUT]; GATES],
            w_hh: [[0.0; HIDDEN]; GATES],
            b_ih: [0.0; GATES],
            b_hh: [0.0; GATES],
            w_dense: [0.0; HIDDEN],
            b_dense: 0.0,
            h: [0.0; HIDDEN],
        };

        // GRU weight_ih: 96 x 2
        let w_ih = gru["weight_ih"].as_array().expect("gru.weight_ih missing");
        assert_eq!(w_ih.len(), GATES, "gru.weight_ih rows must be {GATES}");
        for (row, row_v) in w_ih.iter().enumerate() {
            let cols = row_v.as_array().expect("gru.weight_ih row not an array");
            assert_eq!(
                cols.len(),
                INPUT,
                "gru.weight_ih row {row} must have {INPUT} cols"
            );
            for (col, v) in cols.iter().enumerate() {
                model.w_ih[row][col] = as_f32(v, "gru.weight_ih");
            }
        }

        // GRU weight_hh: 96 x 32
        let w_hh = gru["weight_hh"].as_array().expect("gru.weight_hh missing");
        assert_eq!(w_hh.len(), GATES, "gru.weight_hh rows must be {GATES}");
        for (row, row_v) in w_hh.iter().enumerate() {
            let cols = row_v.as_array().expect("gru.weight_hh row not an array");
            assert_eq!(
                cols.len(),
                HIDDEN,
                "gru.weight_hh row {row} must have {HIDDEN} cols"
            );
            for (col, v) in cols.iter().enumerate() {
                model.w_hh[row][col] = as_f32(v, "gru.weight_hh");
            }
        }

        // GRU bias_ih / bias_hh: 96 each
        fill_vec(&gru["bias_ih"], &mut model.b_ih, GATES, "gru.bias_ih");
        fill_vec(&gru["bias_hh"], &mut model.b_hh, GATES, "gru.bias_hh");

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

    /// Run one timestep. `x = [audio, bias]`. Zero-alloc: only fixed stack arrays.
    ///
    /// Implements PyTorch nn.GRU exactly:
    /// ```text
    /// r = sigmoid(W_ir x + b_ir + W_hr h + b_hr)
    /// z = sigmoid(W_iz x + b_iz + W_hz h + b_hz)
    /// n = tanh(W_in x + b_in + r * (W_hn h + b_hn))
    /// h' = (1 - z) * n + z * h
    /// y  = W_dense h' + b_dense
    /// ```
    pub fn forward(&mut self, x: &[f32; 2]) -> [f32; 1] {
        let mut r = [0.0f32; HIDDEN];
        let mut z = [0.0f32; HIDDEN];
        let mut n = [0.0f32; HIDDEN];

        for i in 0..HIDDEN {
            // reset gate r: rows [0..32]
            let row_r = i;
            let ih_r = self.w_ih[row_r][0] * x[0] + self.w_ih[row_r][1] * x[1] + self.b_ih[row_r];
            let mut hh_r = self.b_hh[row_r];
            for (j, &hv) in self.h.iter().enumerate() {
                hh_r += self.w_hh[row_r][j] * hv;
            }
            r[i] = sigmoid(ih_r + hh_r);

            // update gate z: rows [32..64]
            let row_z = HIDDEN + i;
            let ih_z = self.w_ih[row_z][0] * x[0] + self.w_ih[row_z][1] * x[1] + self.b_ih[row_z];
            let mut hh_z = self.b_hh[row_z];
            for (j, &hv) in self.h.iter().enumerate() {
                hh_z += self.w_hh[row_z][j] * hv;
            }
            z[i] = sigmoid(ih_z + hh_z);
        }

        for i in 0..HIDDEN {
            // new gate n: rows [64..96]. r multiplies the ENTIRE hh term incl b_hn.
            let row_n = 2 * HIDDEN + i;
            let ih_n = self.w_ih[row_n][0] * x[0] + self.w_ih[row_n][1] * x[1] + self.b_ih[row_n];
            let mut hh_n = self.b_hh[row_n];
            for (j, &hv) in self.h.iter().enumerate() {
                hh_n += self.w_hh[row_n][j] * hv;
            }
            n[i] = (ih_n + r[i] * hh_n).tanh();
        }

        let mut y = self.b_dense;
        for i in 0..HIDDEN {
            let h_new = (1.0 - z[i]) * n[i] + z[i] * self.h[i];
            self.h[i] = h_new;
            y += self.w_dense[i] * h_new;
        }

        [y]
    }

    /// Zero the hidden state.
    pub fn reset(&mut self) {
        self.h = [0.0; HIDDEN];
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
