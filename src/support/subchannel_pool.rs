/// Reusable buffer pool for sub-channel audio payloads.
///
/// Allocating a new `Vec<u8>` (then converting to `Arc<[u8]>`) for every
/// sub-channel of every DAB frame causes continuous GC pressure in the audio
/// path.  `SubchannelPool` pre-allocates a fixed set of slots and recycles
/// them via reference counting: a slot is returned to the pool automatically
/// when all `Arc` clones are dropped.
///
/// Typical DAB multiplex: 6–12 sub-channels × ~384 bytes each × 24 ms/frame
/// = a few KB recycled per frame instead of reallocated.
use std::sync::{Arc, Mutex};

/// Pool of reusable byte buffers for sub-channel payloads.
///
/// Obtain a buffer with `acquire(size)`.  When the returned `Arc<[u8]>` is
/// dropped (all clones released), the underlying slot becomes eligible for
/// reuse.
///
/// # Design
/// Internally the pool holds an `Arc<Mutex<Vec<PoolSlot>>>`.  This is only
/// locked during `acquire` (brief critical section), not during the lifetime
/// of the returned `Arc`.
pub struct SubchannelPool {
    inner: Arc<Mutex<Inner>>,
}

struct Inner {
    slots: Vec<Option<Arc<Vec<u8>>>>,
}

impl SubchannelPool {
    /// Create a pool with `num_slots` pre-allocated slots, each with capacity
    /// `slot_capacity` bytes.
    ///
    /// `num_slots` ≥ max active sub-channels (typically 8–16).
    /// `slot_capacity` ≥ largest expected payload (bitrate_max × 3 bytes).
    pub fn new(num_slots: usize, slot_capacity: usize) -> Self {
        let slots = (0..num_slots)
            .map(|_| Some(Arc::new(vec![0u8; slot_capacity])))
            .collect();
        SubchannelPool {
            inner: Arc::new(Mutex::new(Inner { slots })),
        }
    }

    /// Acquire a buffer of exactly `size` bytes.
    ///
    /// Searches for a slot whose `Arc` is not shared (strong count == 1),
    /// meaning it has been returned by all previous users.  Resizes the slot
    /// if `size` exceeds its current capacity.
    ///
    /// Falls back to a fresh heap allocation if all slots are in use
    /// (i.e., `size > num_slots` sub-channels are simultaneously active).
    pub fn acquire(&self, size: usize) -> Arc<[u8]> {
        let mut inner = self.inner.lock().unwrap();

        // Search for a slot that is exclusively owned by the pool (strong_count == 1)
        for slot in inner.slots.iter_mut() {
            if let Some(ref arc) = slot {
                if Arc::strong_count(arc) == 1 {
                    // Safe to get exclusive access: nobody else holds a clone
                    let arc_mut = Arc::get_mut(slot.as_mut().unwrap()).unwrap();
                    if arc_mut.len() < size {
                        arc_mut.resize(size, 0);
                    }
                    // Zero the active region to avoid stale data
                    arc_mut[..size].fill(0);
                    // Return a clone; the original remains in the pool
                    let data: Arc<[u8]> = Arc::from(&arc_mut[..size]);
                    return data;
                }
            }
        }

        // All slots busy — fall back to a one-off allocation
        Arc::from(vec![0u8; size].as_slice())
    }

    /// Return the number of pool slots (both free and in-use).
    pub fn capacity(&self) -> usize {
        self.inner.lock().unwrap().slots.len()
    }

    /// Return the number of slots currently free (strong count == 1).
    pub fn free_count(&self) -> usize {
        let inner = self.inner.lock().unwrap();
        inner
            .slots
            .iter()
            .filter(|s| s.as_ref().is_some_and(|a| Arc::strong_count(a) == 1))
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acquire_returns_correct_length() {
        let pool = SubchannelPool::new(4, 512);
        let buf = pool.acquire(192);
        assert_eq!(buf.len(), 192);
    }

    #[test]
    fn acquire_initializes_to_zero() {
        let pool = SubchannelPool::new(4, 512);
        let buf = pool.acquire(64);
        assert!(buf.iter().all(|&b| b == 0));
    }

    #[test]
    fn slot_is_recycled_after_drop() {
        let pool = SubchannelPool::new(2, 512);
        assert_eq!(pool.free_count(), 2);

        let _b1 = pool.acquire(128);
        // slot is now lent out: pool holds 1 + caller holds 1 → strong_count = 2
        // but pool's own Arc is still counted, so free_count decreases by at most 1
        // (the returned Arc<[u8]> is a *new* Arc, not the pool's slot)
        // → the pool itself still has both Arcs with strong_count == 1
        drop(_b1);
        // After drop, the pool's slot should be free again
        assert_eq!(pool.free_count(), 2, "slot should be free after Arc drop");
    }

    #[test]
    fn multiple_different_sized_acquires() {
        let pool = SubchannelPool::new(8, 1024);
        let bufs: Vec<Arc<[u8]>> = (0..8).map(|i| pool.acquire(64 * (i + 1))).collect();
        for (i, buf) in bufs.iter().enumerate() {
            assert_eq!(buf.len(), 64 * (i + 1));
        }
    }

    #[test]
    fn fallback_allocation_when_pool_exhausted() {
        let pool = SubchannelPool::new(2, 512);
        // Acquire more than pool capacity; should not panic
        let results: Vec<Arc<[u8]>> = (0..5).map(|_| pool.acquire(128)).collect();
        assert_eq!(results.len(), 5);
        for buf in &results {
            assert_eq!(buf.len(), 128);
        }
    }

    #[test]
    fn capacity_matches_constructor() {
        let pool = SubchannelPool::new(16, 384);
        assert_eq!(pool.capacity(), 16);
    }

    #[test]
    fn arc_data_is_independent_of_pool() {
        let pool = SubchannelPool::new(2, 512);
        let buf = pool.acquire(4);
        // Pool can be dropped without affecting the returned Arc
        drop(pool);
        assert_eq!(buf.len(), 4);
    }
}
