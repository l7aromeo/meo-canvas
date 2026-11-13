//! [`MemoryStore`]: entries in this process, least recently used first out.

use std::{
    collections::{BTreeMap, HashMap},
    num::NonZeroU64,
    sync::{Mutex, MutexGuard},
};

use super::{
    DEFAULT_MAX_ENTRY_BYTES, ENTRY_OVERHEAD_BYTES, ImageCacheEntry, ImageStore,
    StoreError, StoreStats, lock,
};

/// Entries in memory, evicted least recently used first once they pass
/// `max_bytes`.
///
/// Requires the `net` feature. Lost when the process ends; see [`DiskStore`]
/// for entries that survive a restart.
///
/// ```
/// use std::{num::NonZeroU64, sync::Arc};
///
/// use meo_canvas_core::image_cache::{ImageCache, MemoryStore};
///
/// // At start-up, once: 256 MiB, and no entry over 8 MiB.
/// let max = NonZeroU64::new(256 * 1024 * 1024).expect("not zero");
/// let store = MemoryStore::with_max_entry_bytes(max, 8 * 1024 * 1024)?;
/// let cache = Arc::new(ImageCache::new(store));
/// # Ok::<(), meo_canvas_core::image_cache::StoreError>(())
/// ```
///
/// # What it guarantees
///
/// - **Size.** An entry costs its body, its key and
///   [`ENTRY_OVERHEAD_BYTES`](super::ENTRY_OVERHEAD_BYTES), so many tiny
///   entries cannot grow it past `max_bytes` while their bodies stay under it.
///   The total never passes `max_bytes`.
/// - **Eviction.** Storing an entry evicts the least recently used ones until
///   it fits. A lookup or a store makes an entry the most recently used. The
///   entry being stored is never the one evicted.
/// - **Refusal.** A body over `max_entry_bytes`, or an entry whose cost passes
///   `max_bytes`, is not stored: `set` returns `Ok`, and the render draws the
///   bytes it fetched.
/// - **Accounting.** Replacing an entry adjusts the total by the difference;
///   `delete` and `clear` subtract exactly what was added, and deleting twice
///   is harmless. Nothing outside the store can reach its map, so the total
///   cannot drift.
/// - **Threads.** One lock guards the map and the total together. A panic on
///   another thread while it held the lock leaves the store usable: every
///   update completes before the lock is released.
/// - **Bytes.** Entries share their bytes as `Arc<[u8]>`, which nothing can
///   modify, so a hit costs no copy.
///
/// It does not react to the process's memory pressure: `max_bytes` is the
/// whole contract.
///
/// [`DiskStore`]: super::DiskStore
#[derive(Debug)]
pub struct MemoryStore {
    max_bytes: u64,
    max_entry_bytes: u64,
    inner: Mutex<Inner>,
}

#[derive(Debug, Default)]
struct Inner {
    /// Each key's entry, its cost, and its place in `order`.
    entries: HashMap<String, (ImageCacheEntry, u64, u64)>,
    /// Keys by when they were last used, oldest first.
    order: BTreeMap<u64, String>,
    /// The next place in `order`.
    clock: u64,
    /// The sum of every entry's cost.
    total: u64,
    stored: u64,
    evicted: u64,
}

impl Inner {
    /// Makes `key` the most recently used.
    fn touch(&mut self, key: &str) {
        let place = self.clock;
        self.clock += 1;
        if let Some((_, _, held)) = self.entries.get_mut(key) {
            self.order.remove(held);
            *held = place;
            self.order.insert(place, key.to_owned());
        }
    }

    fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
        self.total = 0;
    }

    fn remove(&mut self, key: &str) -> bool {
        match self.entries.remove(key) {
            Some((_, cost, place)) => {
                self.order.remove(&place);
                self.total -= cost;
                true
            }
            None => false,
        }
    }
}

impl MemoryStore {
    /// A store holding up to `max_bytes`, keeping bodies up to the smaller of
    /// `max_bytes` and
    /// [`DEFAULT_MAX_ENTRY_BYTES`](super::DEFAULT_MAX_ENTRY_BYTES).
    #[must_use]
    pub fn new(max_bytes: NonZeroU64) -> Self {
        Self {
            max_bytes: max_bytes.get(),
            max_entry_bytes: DEFAULT_MAX_ENTRY_BYTES.min(max_bytes.get()),
            inner: Mutex::new(Inner::default()),
        }
    }

    /// A store holding up to `max_bytes`, keeping bodies up to
    /// `max_entry_bytes`.
    ///
    /// # Errors
    ///
    /// A [`StoreError`] when `max_entry_bytes` is above `max_bytes`: refused
    /// rather than clamped, so numbers that disagree are found where they
    /// were written.
    pub fn with_max_entry_bytes(
        max_bytes: NonZeroU64,
        max_entry_bytes: u64,
    ) -> Result<Self, StoreError> {
        if max_entry_bytes > max_bytes.get() {
            return Err(StoreError::new(format!(
                "max_entry_bytes ({max_entry_bytes}) is above max_bytes ({})",
                max_bytes.get()
            )));
        }
        Ok(Self {
            max_entry_bytes,
            ..Self::new(max_bytes)
        })
    }

    fn inner(&self) -> MutexGuard<'_, Inner> {
        lock(&self.inner)
    }
}

/// What an entry costs the store.
fn cost(key: &str, entry: &ImageCacheEntry) -> u64 {
    entry.bytes.len() as u64 + key.len() as u64 + ENTRY_OVERHEAD_BYTES
}

impl ImageStore for MemoryStore {
    fn get(&self, key: &str) -> Result<Option<ImageCacheEntry>, StoreError> {
        let mut inner = self.inner();
        let found = inner.entries.get(key).map(|(entry, ..)| entry.clone());
        if found.is_some() {
            inner.touch(key);
        }
        drop(inner);
        Ok(found)
    }

    fn set(
        &self,
        key: &str,
        entry: &ImageCacheEntry,
    ) -> Result<(), StoreError> {
        let cost = cost(key, entry);
        if entry.bytes.len() as u64 > self.max_entry_bytes
            || cost > self.max_bytes
        {
            return Ok(());
        }
        let mut inner = self.inner();
        inner.remove(key);
        while inner.total + cost > self.max_bytes {
            let Some((_, oldest)) = inner.order.pop_first() else {
                break;
            };
            if let Some((_, cost, _)) = inner.entries.remove(&oldest) {
                inner.total -= cost;
                inner.evicted += 1;
            }
        }
        let place = inner.clock;
        inner.clock += 1;
        inner.order.insert(place, key.to_owned());
        inner
            .entries
            .insert(key.to_owned(), (entry.clone(), cost, place));
        inner.total += cost;
        inner.stored += 1;
        drop(inner);
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<(), StoreError> {
        self.inner().remove(key);
        Ok(())
    }

    fn clear(&self) -> Result<(), StoreError> {
        self.inner().clear();
        Ok(())
    }

    fn stats(&self) -> StoreStats {
        let inner = self.inner();
        StoreStats {
            stored: inner.stored,
            evicted: inner.evicted,
            bytes: inner.total,
            entries: inner.entries.len() as u64,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        num::NonZeroU64,
        sync::Arc,
        time::{Duration, UNIX_EPOCH},
    };

    use super::{ENTRY_OVERHEAD_BYTES, MemoryStore};
    use crate::image_cache::{ImageCacheEntry, ImageStore};

    fn entry(body: usize) -> ImageCacheEntry {
        ImageCacheEntry {
            bytes: Arc::from(vec![7_u8; body]),
            etag: None,
            last_modified: None,
            cache_control: None,
            expires: None,
            pragma: None,
            date: None,
            age: None,
            stored_at: UNIX_EPOCH,
            expires_at: UNIX_EPOCH + Duration::from_secs(60),
            stale_if_error_until: UNIX_EPOCH + Duration::from_secs(60),
            revalidate_always: false,
        }
    }

    fn bytes(n: u64) -> NonZeroU64 {
        NonZeroU64::new(n).unwrap_or(NonZeroU64::MIN)
    }

    #[test]
    fn max_entry_bytes_above_max_bytes_is_refused() {
        let Err(error) = MemoryStore::with_max_entry_bytes(bytes(100), 101)
        else {
            unreachable!("max_entry_bytes above max_bytes was accepted");
        };
        assert!(error.to_string().contains("above max_bytes"), "{error}");
        assert!(MemoryStore::with_max_entry_bytes(bytes(100), 100).is_ok());
    }

    #[test]
    fn many_tiny_entries_are_bounded_by_the_overhead() {
        // A three-byte key and a one-byte body cost `4 + overhead`. Without
        // the overhead all hundred would fit in the 400 bytes they hold.
        let each = 4 + ENTRY_OVERHEAD_BYTES;
        let store = MemoryStore::new(bytes(4 * each));
        for n in 0..100 {
            store
                .set(&format!("k{n:02}"), &entry(1))
                .unwrap_or_else(|error| unreachable!("stored: {error}"));
        }
        let stats = store.stats();
        assert_eq!((stats.entries, stats.bytes), (4, 4 * each));
        assert_eq!(stats.evicted, 96);
    }

    #[test]
    fn replacing_an_entry_adjusts_the_total_by_the_difference() {
        let store = MemoryStore::new(bytes(10_000));
        store
            .set("a", &entry(100))
            .unwrap_or_else(|error| unreachable!("stored: {error}"));
        store
            .set("a", &entry(40))
            .unwrap_or_else(|error| unreachable!("stored: {error}"));
        let stats = store.stats();
        assert_eq!(
            (stats.entries, stats.bytes),
            (1, 40 + 1 + ENTRY_OVERHEAD_BYTES)
        );
        store
            .delete("a")
            .unwrap_or_else(|error| unreachable!("deleted: {error}"));
        store
            .delete("a")
            .unwrap_or_else(|error| unreachable!("deleted twice: {error}"));
        assert_eq!((store.stats().entries, store.stats().bytes), (0, 0));
    }

    #[test]
    fn a_get_makes_the_entry_the_newest() {
        let one = 100 + 1 + ENTRY_OVERHEAD_BYTES;
        let store = MemoryStore::new(bytes(2 * one));
        store
            .set("a", &entry(100))
            .unwrap_or_else(|error| unreachable!("stored: {error}"));
        store
            .set("b", &entry(100))
            .unwrap_or_else(|error| unreachable!("stored: {error}"));
        assert!(
            store
                .get("a")
                .unwrap_or_else(|error| unreachable!("read: {error}"))
                .is_some()
        );
        store
            .set("c", &entry(100))
            .unwrap_or_else(|error| unreachable!("stored: {error}"));
        assert!(
            store
                .get("a")
                .unwrap_or_else(|error| unreachable!("read: {error}"))
                .is_some(),
            "a was used last"
        );
        assert!(
            store
                .get("b")
                .unwrap_or_else(|error| unreachable!("read: {error}"))
                .is_none(),
            "b was oldest"
        );
        assert!(
            store
                .get("c")
                .unwrap_or_else(|error| unreachable!("read: {error}"))
                .is_some()
        );
    }

    #[test]
    fn an_entry_over_the_limits_is_not_stored() {
        let store = MemoryStore::with_max_entry_bytes(bytes(10_000), 50)
            .unwrap_or_else(|error| unreachable!("valid: {error}"));
        store
            .set("small", &entry(10))
            .unwrap_or_else(|error| unreachable!("stored: {error}"));
        store
            .set("big", &entry(51))
            .unwrap_or_else(|error| unreachable!("refused quietly: {error}"));
        assert!(
            store
                .get("big")
                .unwrap_or_else(|error| unreachable!("read: {error}"))
                .is_none()
        );
        assert!(
            store
                .get("small")
                .unwrap_or_else(|error| unreachable!("read: {error}"))
                .is_some(),
            "nothing evicted"
        );
        // Within `max_entry_bytes` but past `max_bytes` once its cost counts.
        let tight = MemoryStore::new(bytes(60));
        tight
            .set("k", &entry(50))
            .unwrap_or_else(|error| unreachable!("refused quietly: {error}"));
        assert_eq!(tight.stats().entries, 0);
    }

    #[test]
    fn clear_empties_the_total() {
        let store = MemoryStore::new(bytes(10_000));
        store
            .set("a", &entry(10))
            .unwrap_or_else(|error| unreachable!("stored: {error}"));
        store
            .set("b", &entry(10))
            .unwrap_or_else(|error| unreachable!("stored: {error}"));
        store
            .clear()
            .unwrap_or_else(|error| unreachable!("cleared: {error}"));
        let stats = store.stats();
        assert_eq!((stats.entries, stats.bytes), (0, 0));
        store
            .set("a", &entry(10))
            .unwrap_or_else(|error| unreachable!("stored: {error}"));
        assert_eq!(store.stats().bytes, 10 + 1 + ENTRY_OVERHEAD_BYTES);
    }

    #[test]
    fn a_panic_while_the_lock_is_held_leaves_the_store_usable() {
        let store = Arc::new(MemoryStore::new(bytes(10_000)));
        store
            .set("a", &entry(10))
            .unwrap_or_else(|error| unreachable!("stored: {error}"));
        let held = Arc::clone(&store);
        let panicked = std::thread::spawn(move || {
            let _guard = held.inner.lock();
            std::panic::resume_unwind(Box::new(
                "a render panicking with the store's lock held",
            ));
        })
        .join();
        assert!(panicked.is_err());
        assert!(store.inner.is_poisoned(), "the lock is poisoned");
        assert!(
            store
                .get("a")
                .unwrap_or_else(|error| unreachable!("read: {error}"))
                .is_some()
        );
        store
            .set("b", &entry(10))
            .unwrap_or_else(|error| unreachable!("stored: {error}"));
        assert_eq!(store.stats().entries, 2);
    }
}
