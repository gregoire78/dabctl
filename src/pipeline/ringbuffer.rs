// Capacity-bounded ring buffer backed by a `Mutex<VecDeque<T>>`.
// Non-blocking on both producer and consumer sides: `put_data` silently drops
// items that would exceed capacity, and `get_data` returns whatever is available.
// Equivalent to the C++ `RingBuffer<T>` used in eti-cmdline.

use std::collections::VecDeque;
use std::sync::Mutex;

pub struct RingBuffer<T> {
    inner: Mutex<VecDeque<T>>,
    capacity: usize,
}

impl<T: Clone> RingBuffer<T> {
    pub fn new(capacity: usize) -> Self {
        RingBuffer {
            inner: Mutex::new(VecDeque::with_capacity(capacity)),
            capacity,
        }
    }

    /// Write up to `data.len()` items into the buffer.
    ///
    /// Items that would exceed `capacity` are silently dropped. Returns the
    /// number of items actually written.
    pub fn put_data(&self, data: &[T]) -> usize {
        let mut buf = self.inner.lock().unwrap();
        let available = self.capacity.saturating_sub(buf.len());
        let to_write = data.len().min(available);
        buf.extend(data[..to_write].iter().cloned());
        to_write
    }

    /// Read up to `out.len()` items from the buffer without blocking.
    ///
    /// Returns the number of items copied; may be less than `out.len()` when
    /// fewer items are buffered.
    pub fn get_data(&self, out: &mut [T]) -> usize {
        let mut buf = self.inner.lock().unwrap();
        let to_read = out.len().min(buf.len());
        for (dst, src) in out[..to_read].iter_mut().zip(buf.drain(..to_read)) {
            *dst = src;
        }
        to_read
    }

    pub fn available_read(&self) -> usize {
        self.inner.lock().unwrap().len()
    }

    pub fn available_write(&self) -> usize {
        let buf = self.inner.lock().unwrap();
        self.capacity.saturating_sub(buf.len())
    }

    /// Drop up to `count` oldest buffered items.
    ///
    /// Mirrors DABstar's skip-data helper for consumers that need to fast-forward
    /// without copying samples out first.
    pub fn skip_data(&self, count: usize) -> usize {
        let mut buf = self.inner.lock().unwrap();
        let skipped = count.min(buf.len());
        buf.drain(..skipped);
        skipped
    }

    /// Return the current fill state as a percentage of the configured capacity.
    pub fn fill_state_in_percent(&self) -> f32 {
        if self.capacity == 0 {
            return 0.0;
        }
        let buf = self.inner.lock().unwrap();
        buf.len() as f32 * 100.0 / self.capacity as f32
    }

    pub fn flush(&self) {
        self.inner.lock().unwrap().clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_buffer_is_empty() {
        let rb: RingBuffer<u8> = RingBuffer::new(100);
        assert_eq!(rb.available_read(), 0);
        assert_eq!(rb.available_write(), 100);
    }

    #[test]
    fn put_then_get_preserves_order_and_values() {
        let rb: RingBuffer<u8> = RingBuffer::new(100);
        assert_eq!(rb.put_data(&[1, 2, 3, 4, 5]), 5);
        assert_eq!(rb.available_read(), 5);
        let mut out = vec![0u8; 5];
        assert_eq!(rb.get_data(&mut out), 5);
        assert_eq!(out, [1, 2, 3, 4, 5]);
        assert_eq!(rb.available_read(), 0);
    }

    #[test]
    fn put_data_drops_items_that_exceed_capacity() {
        let rb: RingBuffer<u8> = RingBuffer::new(3);
        assert_eq!(rb.put_data(&[1, 2, 3, 4, 5]), 3);
        assert_eq!(rb.available_read(), 3);
    }

    #[test]
    fn get_data_returns_partial_result_when_buffer_is_short() {
        let rb: RingBuffer<u8> = RingBuffer::new(100);
        rb.put_data(&[1, 2]);
        let mut out = vec![0u8; 5];
        assert_eq!(rb.get_data(&mut out), 2);
        assert_eq!(&out[..2], &[1, 2]);
    }

    #[test]
    fn flush_empties_the_buffer() {
        let rb: RingBuffer<u8> = RingBuffer::new(100);
        rb.put_data(&[1, 2, 3]);
        assert_eq!(rb.available_read(), 3);
        rb.flush();
        assert_eq!(rb.available_read(), 0);
    }

    #[test]
    fn skip_data_discards_oldest_items() {
        let rb: RingBuffer<u8> = RingBuffer::new(8);
        rb.put_data(&[10, 11, 12, 13, 14]);

        assert_eq!(rb.skip_data(2), 2);
        assert_eq!(rb.available_read(), 3);

        let mut out = [0u8; 3];
        assert_eq!(rb.get_data(&mut out), 3);
        assert_eq!(out, [12, 13, 14]);
    }

    #[test]
    fn fill_state_percent_reflects_capacity_usage() {
        let rb: RingBuffer<u8> = RingBuffer::new(4);
        assert_eq!(rb.fill_state_in_percent(), 0.0);
        rb.put_data(&[1, 2]);
        assert!((rb.fill_state_in_percent() - 50.0).abs() < f32::EPSILON);
    }
}
