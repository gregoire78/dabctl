// Ring buffer - thread-safe, matching the C++ RingBuffer<T> used in eti-cmdline

use std::collections::VecDeque;
use std::sync::{Mutex, Condvar};

pub struct RingBuffer<T> {
    inner: Mutex<VecDeque<T>>,
    capacity: usize,
    condvar: Condvar,
}

impl<T: Clone + Default> RingBuffer<T> {
    pub fn new(capacity: usize) -> Self {
        RingBuffer {
            inner: Mutex::new(VecDeque::with_capacity(capacity)),
            capacity,
            condvar: Condvar::new(),
        }
    }

    pub fn put_data(&self, data: &[T]) -> usize {
        let mut buf = self.inner.lock().unwrap();
        let available = self.capacity.saturating_sub(buf.len());
        let to_write = data.len().min(available);
        for item in &data[..to_write] {
            buf.push_back(item.clone());
        }
        self.condvar.notify_all();
        to_write
    }

    pub fn get_data(&self, out: &mut [T]) -> usize {
        let mut buf = self.inner.lock().unwrap();
        let to_read = out.len().min(buf.len());
        for item in out[..to_read].iter_mut() {
            *item = buf.pop_front().unwrap();
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

    pub fn flush(&self) {
        self.inner.lock().unwrap().clear();
    }
}
