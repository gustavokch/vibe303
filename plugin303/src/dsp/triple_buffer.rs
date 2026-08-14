use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicU8, Ordering};

/// Lock-free, wait-free Triple Buffer for Single Producer / Single Consumer (Worker -> Audio thread).
/// Guarantees zero allocation, zero locking, and wait-free reads on the audio thread.
pub struct TripleBuffer<T> {
    buffers: [UnsafeCell<T>; 3],
    clean: AtomicU8,
    read_idx: AtomicU8,
    write_idx: AtomicU8,
}

unsafe impl<T: Send> Send for TripleBuffer<T> {}
unsafe impl<T: Sync> Sync for TripleBuffer<T> {}

impl<T: Clone> TripleBuffer<T> {
    pub fn new(initial: T) -> Self {
        Self {
            buffers: [
                UnsafeCell::new(initial.clone()),
                UnsafeCell::new(initial.clone()),
                UnsafeCell::new(initial),
            ],
            clean: AtomicU8::new(0),
            read_idx: AtomicU8::new(1),
            write_idx: AtomicU8::new(2),
        }
    }

    /// Read the latest published buffer. Wait-free and allocation-free.
    /// Safe to call on the real-time audio thread.
    #[inline]
    pub fn read(&self) -> &T {
        let clean_val = self.clean.load(Ordering::Acquire);
        if (clean_val & 0b100) != 0 {
            let current_read = self.read_idx.load(Ordering::Relaxed);
            let prev = self.clean.swap(current_read, Ordering::AcqRel);
            if (prev & 0b100) != 0 {
                self.read_idx.store(prev & 0b011, Ordering::Release);
            }
        }

        let idx = self.read_idx.load(Ordering::Acquire) as usize;
        unsafe { &*self.buffers[idx].get() }
    }

    /// Write a new buffer. Lock-free. Called by the background worker thread.
    pub fn write(&self, value: T) {
        let current_write = self.write_idx.load(Ordering::Relaxed) as usize;
        unsafe {
            *self.buffers[current_write].get() = value;
        }
        let prev = self
            .clean
            .swap((current_write as u8) | 0b100, Ordering::AcqRel);
        self.write_idx.store(prev & 0b011, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_triple_buffer_basic() {
        let tb = TripleBuffer::new(10);
        assert_eq!(*tb.read(), 10);
        tb.write(20);
        assert_eq!(*tb.read(), 20);
        tb.write(30);
        tb.write(40);
        assert_eq!(*tb.read(), 40);
    }

    #[test]
    fn test_triple_buffer_threaded() {
        let tb = Arc::new(TripleBuffer::new(0));
        let tb_writer = Arc::clone(&tb);

        let writer = thread::spawn(move || {
            for i in 1..=10000 {
                tb_writer.write(i);
            }
        });

        let mut last_seen = 0;
        while last_seen < 10000 {
            let val = *tb.read();
            assert!(val >= last_seen, "val {} < last_seen {}", val, last_seen);
            last_seen = val;
            if writer.is_finished() && last_seen == 10000 {
                break;
            }
        }

        writer.join().unwrap();
        assert_eq!(*tb.read(), 10000);
    }
}
