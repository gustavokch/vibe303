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
