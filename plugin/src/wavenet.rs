//! Hand-coded pure-Rust streaming WaveNet matching the `nam` 0.13 fork's FiLM-conditioned
//! `WaveNet` (`nam/models/wavenet/`) exactly, for the gain-conditioned BA283 export
//! (`exported_models/model.cond.nam`). `process()` is zero-alloc: all weight/state storage is heap-`Box`ed
//! ONCE in `new`/`from_json_str`; the audio path only indexes fixed `[f32; _]` scratch and the
//! pre-allocated ring buffers.
//!
//! Topology actually used by `model.cond.nam` (validated at parse time; the rest of the fork's
//! feature-set — gating, slimmable, post-/input-mixin-FiLM, head1x1 — is INACTIVE here and rejected
//! if seen): a single layer-array, `channels = bottleneck = 3`, condition/input size 2
//! (`[audio, gain]`), 23 dilated layers (kernels 21×6 + 2×15), per-layer `LeakyReLU(0.01)`, the
//! `conv_pre_film` FiLM hook and the `layer1x1` residual conv active, a per-array head rechannel
//! conv (3→1, kernel 16) and a trailing `head_scale`. Receptive field 6347.
//!
//! Per-layer forward (see `_layer_array.py::_Layer.forward`):
//! ```text
//! ci      = conv_pre_film(x, c)   // FiLM: film1x1(c) -> [scale(3), shift(3)]; ci = scale*x + shift
//! zconv   = dilated_conv(ci)      // Conv1d 3->3, kernel k, dilation d, bias
//! mix     = input_mixer(c)        // Conv1d 2->3, k=1, no bias
//! a       = LeakyReLU0.01(zconv + mix)
//! head_t  = a                     // summed across layers into the head input
//! x_next  = x + layer1x1(a)       // Conv1d 3->3, k=1, bias (residual)
//! ```
//! Output = `head_scale * head_rechannel(Σ head_t)`.
//!
//! Streaming is the standard causal "fast WaveNet": each layer keeps a ring buffer of its
//! residual-stream INPUT over its dilated span, a shared condition ring covers the largest tap span,
//! and a head ring covers the head-rechannel kernel. This is bit-equivalent to the offline valid-mode
//! forward with `RF-1` zeros of front-pad, so it adds ZERO latency. `reset()` zeros all rings (= the
//! all-zero pre-history).

use serde_json::Value;

/// Residual-stream channel width (`channels`) and bottleneck (`bottleneck`). Equal in this export.
const C: usize = 3;
/// Condition / model-input width (`[audio, gain]`).
const COND: usize = 2;
/// FiLM 1x1 output width: `scale(C)` followed by `shift(C)`.
const FILM_OUT: usize = 2 * C;
/// Head rechannel conv kernel (`config.layers[0].head.kernel_size`).
const HEAD_K: usize = 16;
/// LeakyReLU negative slope used by every layer.
const LEAKY: f32 = 0.01;

/// One dilated WaveNet layer: weights (parsed once) plus its residual-input ring buffer.
struct Layer {
    k: usize,
    d: usize,
    /// Dilated conv weight, indexed `[kk][out_c][in_c]` (PyTorch `(out, in, k)` re-laid for tap-major
    /// access). `kk` runs oldest→newest; tap `kk` reads the input `(k-1-kk)*d` samples back.
    conv_w: Box<[[[f32; C]; C]]>, // len k
    conv_b: [f32; C],
    /// Input-mixer 1x1 weight `[out_c][in_c]` (no bias).
    mix_w: [[f32; COND]; C],
    /// Residual 1x1 weight `[out_c][in_c]` + bias.
    l1x1_w: [[f32; C]; C],
    l1x1_b: [f32; C],
    /// FiLM 1x1 weight `[j][in_c]` (`j` in 0..2C) + bias; `j<C` are scales, `j>=C` are shifts.
    film_w: [[f32; COND]; FILM_OUT],
    film_b: [f32; FILM_OUT],
    /// Residual-input history, length `span = (k-1)*d + 1`. `pos` is the slot of the CURRENT sample.
    ring: Box<[[f32; C]]>,
    pos: usize,
}

impl Layer {
    #[inline]
    fn span(&self) -> usize {
        self.ring.len()
    }
}

/// Streaming FiLM-conditioned WaveNet (`model.cond.nam`).
pub struct WaveNet {
    /// Input rechannel 1x1 weight `[out_c][in_c]` (2→3, no bias).
    rechannel_w: [[f32; COND]; C],
    layers: Vec<Layer>,
    /// Head rechannel conv weight `[kk][in_c]` (out=1), `kk` oldest→newest; bias; output `* head_scale`.
    head_w: Box<[[f32; C]; HEAD_K]>,
    head_b: f32,
    head_scale: f32,
    /// Shared condition history `[audio*input_scale, gain]`, length = max layer span. `cpos` = current.
    cring: Box<[[f32; COND]]>,
    cpos: usize,
    /// Head-input history (Σ of per-layer `head_t`), length `HEAD_K`. `hpos` = current.
    hring: Box<[[f32; C]; HEAD_K]>,
    hpos: usize,
    receptive_field: usize,
    input_scale: f32,
    output_ref_v: f32,
    sample_rate: u32,
}

/// Sequential cursor over the flat `weights` array (PyTorch `export_weights` order).
struct Cursor<'a> {
    w: &'a [f32],
    i: usize,
}
impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> &'a [f32] {
        let s = &self.w[self.i..self.i + n];
        self.i += n;
        s
    }
    fn take1(&mut self) -> f32 {
        let v = self.w[self.i];
        self.i += 1;
        v
    }
}

#[inline]
fn require(cond: bool, msg: &str) {
    assert!(cond, "model.cond.nam: {msg}");
}

fn active(v: &Value) -> bool {
    v.get("active").and_then(Value::as_bool).unwrap_or(false)
}

impl WaveNet {
    /// Load the embedded conditioned WaveNet (`exported_models/model.cond.nam`).
    pub fn new_from_embedded() -> Self {
        // Single-sourced from the canonical export so the embedded blob can never drift from
        // `exported_models/` (path is relative to this source file: plugin/src → repo root).
        const NAM_JSON: &str = include_str!("../../exported_models/model.cond.nam");
        Self::from_json_str(NAM_JSON)
    }

    /// Parse a `.nam` (nam fork export) into the streaming engine. Panics with a clear message if the
    /// file is not the validated active topology — the engine is deliberately scoped to what this
    /// export needs rather than re-implementing the whole fork.
    pub fn from_json_str(s: &str) -> Self {
        let doc: Value = serde_json::from_str(s).expect("model.cond.nam: invalid JSON");
        require(
            doc["architecture"].as_str() == Some("WaveNet"),
            "architecture must be WaveNet",
        );
        let meta = &doc["metadata"];
        let cond = &meta["condition"];
        // Audio channel is scaled by `audio_input_scale` before the net (trained on `dry * scale`);
        // fall back to the top-level `input_scale` for older exports.
        let input_scale = cond["audio_input_scale"]
            .as_f64()
            .or_else(|| meta["input_scale"].as_f64())
            .unwrap_or(2.0) as f32;
        let output_ref_v = meta["output_ref_v"].as_f64().unwrap_or(64.0) as f32;
        let sample_rate = doc["sample_rate"].as_u64().unwrap_or(48_000) as u32;
        require(
            cond["gain_channel"].as_u64().unwrap_or(1) == 1,
            "gain_channel must be 1",
        );

        let config = &doc["config"];
        require(
            config["head"].is_null(),
            "top-level head must be null (head is the per-array rechannel)",
        );
        let head_scale = config["head_scale"]
            .as_f64()
            .expect("missing head_scale") as f32;
        let arrays = config["layers"]
            .as_array()
            .expect("config.layers must be an array");
        require(arrays.len() == 1, "exactly one layer-array is supported");
        let la = &arrays[0];

        require(
            la["channels"].as_u64() == Some(C as u64),
            "channels must be 3",
        );
        require(
            la["bottleneck"].as_u64() == Some(C as u64),
            "bottleneck must equal channels (3)",
        );
        require(
            la["input_size"].as_u64() == Some(COND as u64)
                && la["condition_size"].as_u64() == Some(COND as u64),
            "input_size and condition_size must be 2",
        );
        // Active-feature gate: only conv_pre_film + layer1x1 are supported; everything else off.
        require(active(&la["conv_pre_film"]), "conv_pre_film must be active");
        require(
            la["conv_pre_film"]["shift"].as_bool() != Some(false),
            "conv_pre_film must use shift",
        );
        require(active(&la["layer1x1"]), "layer1x1 must be active");
        for key in [
            "head1x1",
            "conv_post_film",
            "input_mixin_pre_film",
            "input_mixin_post_film",
            "activation_pre_film",
            "activation_post_film",
            "layer1x1_post_film",
            "head1x1_post_film",
        ] {
            require(!active(&la[key]), &format!("{key} must be inactive"));
        }
        require(la["slimmable"].is_null(), "slimmable must be null");
        for g in la["gating_mode"].as_array().into_iter().flatten() {
            require(g.as_str() == Some("none"), "gating_mode must be none");
        }
        for a in la["secondary_activation"].as_array().into_iter().flatten() {
            require(a.is_null(), "secondary_activation must be null");
        }
        for a in la["activation"].as_array().into_iter().flatten() {
            require(
                a["type"].as_str() == Some("LeakyReLU")
                    && (a["negative_slope"].as_f64().unwrap_or(LEAKY as f64) - LEAKY as f64).abs()
                        < 1e-9,
                "activation must be LeakyReLU(0.01)",
            );
        }

        let kernels: Vec<usize> = la["kernel_sizes"]
            .as_array()
            .expect("missing kernel_sizes")
            .iter()
            .map(|v| v.as_u64().unwrap() as usize)
            .collect();
        let dilations: Vec<usize> = la["dilations"]
            .as_array()
            .expect("missing dilations")
            .iter()
            .map(|v| v.as_u64().unwrap() as usize)
            .collect();
        require(
            kernels.len() == dilations.len() && !kernels.is_empty(),
            "kernel_sizes and dilations must align and be non-empty",
        );
        let head = &la["head"];
        require(
            head["out_channels"].as_u64() == Some(1)
                && head["kernel_size"].as_u64() == Some(HEAD_K as u64),
            "head must be out=1, kernel=16",
        );

        // ---- Walk the flat weight blob in export order ----
        let weights: Vec<f32> = doc["weights"]
            .as_array()
            .expect("missing weights")
            .iter()
            .map(|v| v.as_f64().expect("weight not a number") as f32)
            .collect();
        let mut cur = Cursor { w: &weights, i: 0 };

        // rechannel: Conv1d(2->3, k=1, bias=false). PyTorch weight (out, in, 1) row-major.
        let mut rechannel_w = [[0.0f32; COND]; C];
        {
            let w = cur.take(C * COND);
            for (oc, row) in rechannel_w.iter_mut().enumerate() {
                for (ic, val) in row.iter_mut().enumerate() {
                    *val = w[oc * COND + ic];
                }
            }
        }

        let mut layers = Vec::with_capacity(kernels.len());
        for (&k, &d) in kernels.iter().zip(&dilations) {
            // conv: (out=C, in=C, k); store tap-major [kk][oc][ic].
            let cw = cur.take(C * C * k);
            let mut conv_w = vec![[[0.0f32; C]; C]; k].into_boxed_slice();
            for oc in 0..C {
                for ic in 0..C {
                    for kk in 0..k {
                        conv_w[kk][oc][ic] = cw[(oc * C + ic) * k + kk];
                    }
                }
            }
            let cb = cur.take(C);
            let mut conv_b = [0.0f32; C];
            conv_b.copy_from_slice(cb);

            // input_mixer: (out=C, in=COND, 1), no bias.
            let mw = cur.take(C * COND);
            let mut mix_w = [[0.0f32; COND]; C];
            for (oc, row) in mix_w.iter_mut().enumerate() {
                for (ic, val) in row.iter_mut().enumerate() {
                    *val = mw[oc * COND + ic];
                }
            }

            // layer1x1: (out=C, in=C, 1) + bias.
            let lw = cur.take(C * C);
            let mut l1x1_w = [[0.0f32; C]; C];
            for (oc, row) in l1x1_w.iter_mut().enumerate() {
                for (ic, val) in row.iter_mut().enumerate() {
                    *val = lw[oc * C + ic];
                }
            }
            let lb = cur.take(C);
            let mut l1x1_b = [0.0f32; C];
            l1x1_b.copy_from_slice(lb);

            // conv_pre_film._film: Conv1d(COND -> 2C, 1) + bias.
            let fw = cur.take(FILM_OUT * COND);
            let mut film_w = [[0.0f32; COND]; FILM_OUT];
            for (j, row) in film_w.iter_mut().enumerate() {
                for (ic, val) in row.iter_mut().enumerate() {
                    *val = fw[j * COND + ic];
                }
            }
            let fb = cur.take(FILM_OUT);
            let mut film_b = [0.0f32; FILM_OUT];
            film_b.copy_from_slice(fb);

            let span = (k - 1) * d + 1;
            layers.push(Layer {
                k,
                d,
                conv_w,
                conv_b,
                mix_w,
                l1x1_w,
                l1x1_b,
                film_w,
                film_b,
                ring: vec![[0.0f32; C]; span].into_boxed_slice(),
                pos: 0,
            });
        }

        // head_rechannel: Conv1d(in=C -> out=1, kernel=16) + bias. weight (1, C, 16).
        let hw = cur.take(C * HEAD_K);
        let mut head_w = Box::new([[0.0f32; C]; HEAD_K]);
        for ic in 0..C {
            for kk in 0..HEAD_K {
                head_w[kk][ic] = hw[ic * HEAD_K + kk];
            }
        }
        let head_b = cur.take1();
        // Trailing head_scale stored in the blob (must match config within tolerance).
        let blob_scale = cur.take1();
        require(
            (blob_scale - head_scale).abs() < 1e-4,
            "trailing head_scale must match config.head_scale",
        );
        require(
            cur.i == weights.len(),
            "weight blob length did not match the parsed topology",
        );

        // Receptive field: 1 + Σ (k-1)*d + (head_k - 1).
        let rf = 1
            + kernels
                .iter()
                .zip(&dilations)
                .map(|(&k, &d)| (k - 1) * d)
                .sum::<usize>()
            + (HEAD_K - 1);
        let max_span = layers.iter().map(Layer::span).max().unwrap_or(1);

        WaveNet {
            rechannel_w,
            layers,
            head_w,
            head_b,
            head_scale,
            cring: vec![[0.0f32; COND]; max_span].into_boxed_slice(),
            cpos: 0,
            hring: Box::new([[0.0f32; C]; HEAD_K]),
            hpos: 0,
            receptive_field: rf,
            input_scale,
            output_ref_v,
            sample_rate,
        }
    }

    #[inline]
    pub fn output_ref_v(&self) -> f32 {
        self.output_ref_v
    }
    #[inline]
    pub fn input_scale(&self) -> f32 {
        self.input_scale
    }
    #[inline]
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
    #[inline]
    pub fn receptive_field(&self) -> usize {
        self.receptive_field
    }
    #[inline]
    pub fn num_layers(&self) -> usize {
        self.layers.len()
    }

    /// Zero all history (= the all-zero pre-history the offline forward front-pads with).
    pub fn reset(&mut self) {
        for layer in &mut self.layers {
            for slot in layer.ring.iter_mut() {
                *slot = [0.0; C];
            }
            layer.pos = 0;
        }
        for slot in self.cring.iter_mut() {
            *slot = [0.0; COND];
        }
        self.cpos = 0;
        *self.hring = [[0.0; C]; HEAD_K];
        self.hpos = 0;
    }

    /// Run one causal sample. `audio` is the pre-amp-driven input in the plugin's dBFS convention
    /// (the `input_scale` is applied here); `gain` is the FiLM conditioning value in 0..1. Returns the
    /// NORMALIZED net output (the caller de-norms via `output_ref_v`). Zero-alloc.
    #[inline]
    pub fn process(&mut self, audio: f32, gain: f32) -> f32 {
        let cin = [audio * self.input_scale, gain];
        // Push the current condition into the shared ring.
        let cspan = self.cring.len();
        self.cring[self.cpos] = cin;

        // x = rechannel(input).
        let mut x = [0.0f32; C];
        for (oc, slot) in x.iter_mut().enumerate() {
            let mut s = 0.0;
            for ic in 0..COND {
                s += self.rechannel_w[oc][ic] * cin[ic];
            }
            *slot = s;
        }

        let mut head_sum = [0.0f32; C];
        for layer in &mut self.layers {
            let (k, d, span) = (layer.k, layer.d, layer.ring.len());
            // Current residual-stream input goes into this layer's ring at `pos`.
            layer.ring[layer.pos] = x;

            // Dilated conv with per-tap FiLM. Tap `kk` reads `(k-1-kk)*d` samples back.
            let mut z = layer.conv_b;
            for kk in 0..k {
                let off = (k - 1 - kk) * d;
                let xt = layer.ring[(layer.pos + span - off) % span];
                let ct = self.cring[(self.cpos + cspan - off) % cspan];
                // FiLM: film1x1(c) -> [scale(C), shift(C)]; filmed = scale*x + shift.
                let mut fi = [0.0f32; C];
                for (ic, fslot) in fi.iter_mut().enumerate() {
                    let mut scale = layer.film_b[ic];
                    let mut shift = layer.film_b[C + ic];
                    for jc in 0..COND {
                        scale += layer.film_w[ic][jc] * ct[jc];
                        shift += layer.film_w[C + ic][jc] * ct[jc];
                    }
                    *fslot = scale * xt[ic] + shift;
                }
                let wk = &layer.conv_w[kk];
                for (oc, zslot) in z.iter_mut().enumerate() {
                    let mut s = 0.0;
                    for ic in 0..C {
                        s += wk[oc][ic] * fi[ic];
                    }
                    *zslot += s;
                }
            }
            // input_mixer(c) on the current condition, then LeakyReLU.
            for (oc, zslot) in z.iter_mut().enumerate() {
                let mut s = 0.0;
                for ic in 0..COND {
                    s += layer.mix_w[oc][ic] * cin[ic];
                }
                *zslot += s;
                if *zslot < 0.0 {
                    *zslot *= LEAKY;
                }
            }
            let a = z; // post-activation == head term

            for (hc, hval) in head_sum.iter_mut().enumerate() {
                *hval += a[hc];
            }
            // Residual: x += layer1x1(a).
            for (oc, xslot) in x.iter_mut().enumerate() {
                let mut s = layer.l1x1_b[oc];
                for ic in 0..C {
                    s += layer.l1x1_w[oc][ic] * a[ic];
                }
                *xslot += s;
            }

            layer.pos = (layer.pos + 1) % span;
        }
        self.cpos = (self.cpos + 1) % cspan;

        // Head rechannel conv over the last HEAD_K head sums.
        self.hring[self.hpos] = head_sum;
        let mut hout = self.head_b;
        for kk in 0..HEAD_K {
            let off = HEAD_K - 1 - kk;
            let ht = self.hring[(self.hpos + HEAD_K - off) % HEAD_K];
            for ic in 0..C {
                hout += self.head_w[kk][ic] * ht[ic];
            }
        }
        self.hpos = (self.hpos + 1) % HEAD_K;

        hout * self.head_scale
    }
}
