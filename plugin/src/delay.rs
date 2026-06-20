// plugin/src/delay.rs
pub struct ShortDelay<const N: usize> {
    buf: [f32; N],
    idx: usize,
}

impl<const N: usize> ShortDelay<N> {
    pub const fn new() -> Self {
        Self {
            buf: [0.0; N],
            idx: 0,
        }
    }

    pub fn push_pop(&mut self, x: f32) -> f32 {
        let out = self.buf[self.idx];
        self.buf[self.idx] = x;
        self.idx = (self.idx + 1) % N;
        out
    }

    pub fn reset(&mut self) {
        self.buf = [0.0; N];
        self.idx = 0;
    }
}

impl<const N: usize> Default for ShortDelay<N> {
    fn default() -> Self {
        Self::new()
    }
}

/// Fixed-capacity delay with a RUNTIME-settable length. Backs the dry / WDF-out
/// alignment paths whose delay is the `SrcBridge` latency — which varies per host
/// SR but is fixed for the duration of playback. Capacity `MAX` is the compile-time
/// upper bound (allocation-free); `len` is set at `prepare` and must be `<= MAX`.
pub struct RuntimeDelay<const MAX: usize> {
    buf: [f32; MAX],
    len: usize,
    idx: usize,
}

impl<const MAX: usize> RuntimeDelay<MAX> {
    pub const fn new() -> Self {
        Self {
            buf: [0.0; MAX],
            len: 0,
            idx: 0,
        }
    }

    /// Set the active delay (host samples). Clamped to `MAX`; clears state. A `len > MAX` would
    /// silently under-delay the dry/WDF-out path while the host still reports the unclamped
    /// `bridge_latency()` (PDC vs internal alignment diverge), so assert the capacity holds in debug
    /// — `MAX` (`OS_LATENCY_SAMPLES`) is sized above every standard-rate bridge latency.
    pub fn set_len(&mut self, len: usize) {
        debug_assert!(len <= MAX, "RuntimeDelay len {len} exceeds capacity {MAX}");
        self.len = len.min(MAX);
        self.reset();
    }

    #[inline]
    pub fn push_pop(&mut self, x: f32) -> f32 {
        if self.len == 0 {
            return x;
        }
        let out = self.buf[self.idx];
        self.buf[self.idx] = x;
        self.idx = (self.idx + 1) % self.len;
        out
    }

    pub fn reset(&mut self) {
        self.buf = [0.0; MAX];
        self.idx = 0;
    }
}

impl<const MAX: usize> Default for RuntimeDelay<MAX> {
    fn default() -> Self {
        Self::new()
    }
}
