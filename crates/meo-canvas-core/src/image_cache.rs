//! Fetched images kept across renders: an [`ImageCache`] holding the policy and
//! an [`ImageStore`] holding the bytes, either [`MemoryStore`], [`DiskStore`]
//! or one of the caller's own.
//!
//! Requires the `net` feature, since only a build that fetches has anything
//! to cache. Without a cache a render fetches every URL source, every time;
//! with one, a render reuses what an earlier render fetched for as long as the
//! origin's caching headers allow, and revalidates after that.
//!
//! ```
//! use std::{num::NonZeroU64, sync::Arc};
//!
//! use meo_canvas_core::image_cache::{ImageCache, MemoryStore};
//!
//! // Once, where the server starts: one cache shared by every render.
//! let max_bytes = NonZeroU64::new(256 * 1024 * 1024).expect("not zero");
//! let cache = Arc::new(ImageCache::new(MemoryStore::new(max_bytes)));
//!
//! // Each render takes a clone of the `Arc`; see `Renderer::render_with_cache`.
//! assert_eq!(cache.stats().hits, 0);
//! ```

mod disk;
mod freshness;
mod http_date;
mod memory;

use std::{
    collections::HashMap,
    fmt,
    sync::{
        Arc, Condvar, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub use disk::{DiskStore, TEMP_FILE_MAX_AGE};
use freshness::{Headers, Settings};
pub use memory::MemoryStore;
use meo_canvas_scene::node::HttpOptions;

use crate::{Error, FetchFailure, diagnostic::Diagnostic, resolve};

/// How long past expiry a response with no `stale-if-error` of its own may be
/// served when revalidating it fails: 24 hours.
///
/// A day outlasts an origin outage of the length incidents usually run, a
/// night included, so a render does not lose its images to one; and it bounds
/// how long an image the origin has since replaced can keep being drawn while
/// the origin cannot be reached. Set per cache with
/// [`ImageCache::stale_if_error`]. Requires the `net` feature.
pub const DEFAULT_STALE_IF_ERROR: Duration = Duration::from_hours(24);

/// What one stored entry costs a [`MemoryStore`] beyond its body and its key,
/// in bytes: 256.
///
/// It stands for the metadata and the map slot: five times and a flag (41
/// bytes), a typical `Last-Modified` (29) and quoted `ETag` (under 70), and a
/// hash-map slot with its ordering entry (about 100). Counting it is what stops
/// many tiny entries from growing the store past its `max_bytes` while their
/// bodies stay under it. The npm memory store uses the same number. Requires
/// the `net` feature.
pub const ENTRY_OVERHEAD_BYTES: u64 = 256;

/// The largest body a store keeps when the caller names no `max_entry_bytes`:
/// 32 MiB.
///
/// It is the largest image a render fetches, so by default every image that
/// can be fetched can be kept. Requires the `net` feature.
pub const DEFAULT_MAX_ENTRY_BYTES: u64 = resolve::MAX_IMAGE_BYTES;

/// One cached response: its body and what decides when it is used again.
///
/// Requires the `net` feature. A store keeps it whole and hands it back whole;
/// only [`ImageCache`] reads the times. The fields match the npm surface's
/// `ImageCacheEntry`, and [`DiskStore`] writes them in a format both surfaces
/// read.
#[derive(Clone, PartialEq, Eq)]
pub struct ImageCacheEntry {
    /// The response body, shared rather than copied: the bytes are immutable,
    /// so a store, the cache and a render can all hold them.
    pub bytes: Arc<[u8]>,
    /// The response's `ETag`, sent back as `If-None-Match` to revalidate.
    pub etag: Option<String>,
    /// The response's `Last-Modified`, sent back as `If-Modified-Since`.
    pub last_modified: Option<String>,
    /// The response's `Cache-Control`, kept so a `304` that names none
    /// leaves it in force, as Chrome keeps the stored header
    /// (`http-cache.tsv`: "304 with no Cache-Control").
    pub cache_control: Option<String>,
    /// The response's `Expires`, kept for the same reason.
    pub expires: Option<String>,
    /// The response's `Pragma`, kept for the same reason.
    pub pragma: Option<String>,
    /// The response's `Date`, kept so a `304` without one ages the entry from
    /// the stored date, as Chrome does (`http-cache.tsv`: "304 with no Date
    /// over a stored old Date").
    pub date: Option<String>,
    /// The response's `Age`, kept so a `304` without one keeps it, as Chrome
    /// does (`http-cache.tsv`: "304 with no Age over a stored Age").
    pub age: Option<String>,
    /// When the response was received.
    pub stored_at: SystemTime,
    /// Fresh until this instant; at it and past it, the entry is revalidated
    /// before it is used.
    pub expires_at: SystemTime,
    /// A failed revalidation may serve the entry until this instant.
    pub stale_if_error_until: SystemTime,
    /// The response said `no-cache`: revalidate on every use, even while
    /// fresh.
    pub revalidate_always: bool,
}

impl fmt::Debug for ImageCacheEntry {
    /// Prints the body's length rather than its bytes.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImageCacheEntry")
            .field("bytes", &format_args!("[{} bytes]", self.bytes.len()))
            .field("etag", &self.etag)
            .field("last_modified", &self.last_modified)
            .field("cache_control", &self.cache_control)
            .field("expires", &self.expires)
            .field("pragma", &self.pragma)
            .field("date", &self.date)
            .field("age", &self.age)
            .field("stored_at", &self.stored_at)
            .field("expires_at", &self.expires_at)
            .field("stale_if_error_until", &self.stale_if_error_until)
            .field("revalidate_always", &self.revalidate_always)
            .finish()
    }
}

/// Why a store could not do what it was asked.
///
/// Requires the `net` feature. **A store error never fails a render**: the
/// cache treats it as a miss, fetches, and reports it on the canvas's
/// diagnostics. An error marked [`corrupt`](StoreError::corrupt) also makes
/// the cache delete the entry.
#[derive(Debug)]
pub struct StoreError {
    message: String,
    corrupt: bool,
    source: Option<std::io::Error>,
}

impl StoreError {
    /// An error described by `message`.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            corrupt: false,
            source: None,
        }
    }

    /// An entry that exists and cannot be read as one, which the cache then
    /// deletes.
    #[must_use]
    pub fn corrupt(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            corrupt: true,
            source: None,
        }
    }

    /// Whether the entry was unreadable rather than unreachable.
    #[must_use]
    pub const fn is_corrupt(&self) -> bool {
        self.corrupt
    }
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for StoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        let io: &(dyn std::error::Error + 'static) = self.source.as_ref()?;
        Some(io)
    }
}

impl From<std::io::Error> for StoreError {
    fn from(error: std::io::Error) -> Self {
        Self {
            message: error.to_string(),
            corrupt: false,
            source: Some(error),
        }
    }
}

/// What a store holds, for [`ImageCache::stats`].
///
/// Requires the `net` feature. The store's own numbers: two caches sharing
/// one store report the same ones.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StoreStats {
    /// Entries the store accepted.
    pub stored: u64,
    /// Entries it removed to stay under its size.
    pub evicted: u64,
    /// What it holds now, in bytes, as the store counts them.
    pub bytes: u64,
    /// How many entries it holds now.
    pub entries: u64,
}

/// Where an [`ImageCache`] keeps bytes: get, set, delete and clear, and no
/// policy.
///
/// Requires the `net` feature. [`MemoryStore`] and [`DiskStore`] are the two
/// this crate ships; a caller may write another, over a shared cache server
/// for instance.
///
/// # What a store must uphold
///
/// - **A missing key is `Ok(None)`**, never an error.
/// - **An entry comes back whole or not at all**: the bytes and every field as
///   [`set`](ImageStore::set) was given them, the stored headers included: a
///   `304` merges its own headers into those, so a store that drops one changes
///   how long a revalidated entry stays fresh. An entry that exists but cannot
///   be read is [`StoreError::corrupt`], which makes the cache delete it.
/// - **[`delete`](ImageStore::delete) is idempotent**: deleting a key that is
///   not there is `Ok`.
/// - **It is shared across threads**: renders on several threads call it at
///   once, hence `Send + Sync`.
/// - **It may refuse to keep an entry**, too large for instance, by returning
///   `Ok` from `set` and not storing it.
///
/// An `Err` from any method is reported as a diagnostic on the render that
/// met it, and the render goes on as though the store were empty.
///
/// ```
/// use std::{collections::HashMap, sync::Mutex};
///
/// use meo_canvas_core::image_cache::{
///     ImageCacheEntry, ImageStore, StoreError,
/// };
///
/// /// Every entry, kept forever: a minimal store.
/// #[derive(Debug, Default)]
/// struct Unbounded(Mutex<HashMap<String, ImageCacheEntry>>);
///
/// impl Unbounded {
///     fn map(
///         &self,
///     ) -> std::sync::MutexGuard<'_, HashMap<String, ImageCacheEntry>>
///     {
///         self.0
///             .lock()
///             .unwrap_or_else(std::sync::PoisonError::into_inner)
///     }
/// }
///
/// impl ImageStore for Unbounded {
///     fn get(
///         &self,
///         key: &str,
///     ) -> Result<Option<ImageCacheEntry>, StoreError> {
///         Ok(self.map().get(key).cloned())
///     }
///
///     fn set(
///         &self,
///         key: &str,
///         entry: &ImageCacheEntry,
///     ) -> Result<(), StoreError> {
///         self.map().insert(key.to_owned(), entry.clone());
///         Ok(())
///     }
///
///     fn delete(&self, key: &str) -> Result<(), StoreError> {
///         self.map().remove(key);
///         Ok(())
///     }
///
///     fn clear(&self) -> Result<(), StoreError> {
///         self.map().clear();
///         Ok(())
///     }
/// }
///
/// let cache =
///     meo_canvas_core::image_cache::ImageCache::new(Unbounded::default());
/// assert_eq!(cache.stats().entries, 0);
/// ```
pub trait ImageStore: Send + Sync {
    /// The entry stored under `key`, or `None`.
    ///
    /// # Errors
    ///
    /// A [`StoreError`] when the store cannot be read, or
    /// [`StoreError::corrupt`] when the entry exists and is not one.
    fn get(&self, key: &str) -> Result<Option<ImageCacheEntry>, StoreError>;

    /// Stores `entry` under `key`, replacing any entry already there.
    ///
    /// # Errors
    ///
    /// A [`StoreError`] when the store cannot be written.
    fn set(&self, key: &str, entry: &ImageCacheEntry)
    -> Result<(), StoreError>;

    /// Removes the entry under `key`, if there is one.
    ///
    /// # Errors
    ///
    /// A [`StoreError`] when an entry exists and cannot be removed.
    fn delete(&self, key: &str) -> Result<(), StoreError>;

    /// Removes every entry.
    ///
    /// # Errors
    ///
    /// A [`StoreError`] when an entry cannot be removed.
    fn clear(&self) -> Result<(), StoreError>;

    /// What the store holds, for [`ImageCache::stats`]. All zeros unless the
    /// store counts; both shipped stores do.
    fn stats(&self) -> StoreStats {
        StoreStats::default()
    }
}

impl<S: ImageStore + ?Sized> ImageStore for Arc<S> {
    fn get(&self, key: &str) -> Result<Option<ImageCacheEntry>, StoreError> {
        (**self).get(key)
    }

    fn set(
        &self,
        key: &str,
        entry: &ImageCacheEntry,
    ) -> Result<(), StoreError> {
        (**self).set(key, entry)
    }

    fn delete(&self, key: &str) -> Result<(), StoreError> {
        (**self).delete(key)
    }

    fn clear(&self) -> Result<(), StoreError> {
        (**self).clear()
    }

    fn stats(&self) -> StoreStats {
        (**self).stats()
    }
}

/// What a cache has done, from [`ImageCache::stats`].
///
/// Requires the `net` feature. The first five count this cache's requests by
/// how each was answered; a request that joined another one for the same key
/// already in flight counts as that one did, so eight renders sharing one
/// fetch count eight. The last four are the store's, as [`StoreStats`], and
/// are all zero for a store that does not implement
/// [`ImageStore::stats`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct CacheStats {
    /// Served fresh from the store, with no request.
    pub hits: u64,
    /// Fetched with nothing usable stored, whether the fetch succeeded or not.
    pub misses: u64,
    /// Revalidated with a `304 Not Modified`: the stored bytes were used.
    pub revalidated: u64,
    /// Revalidated with a `200`: the new bytes replaced the stored ones.
    pub replaced: u64,
    /// Revalidation failed, and the expired bytes were served under
    /// stale-if-error.
    pub stale_served: u64,
    /// Entries the store accepted.
    pub stored: u64,
    /// Entries the store removed to stay under its size.
    pub evicted: u64,
    /// What the store holds now, in bytes.
    pub bytes: u64,
    /// How many entries the store holds now.
    pub entries: u64,
}

/// Fetched images kept across renders, revalidated as an HTTP cache would.
///
/// Requires the `net` feature. One per process, created at start-up and
/// passed to every render (`Root::image_cache` on `meo-canvas`, or
/// [`Renderer::render_with_cache`](crate::Renderer::render_with_cache)); a
/// render given none fetches every URL as it always has.
///
/// ```
/// use std::{num::NonZeroU64, sync::Arc, time::Duration};
///
/// use meo_canvas_core::image_cache::{DiskStore, ImageCache};
///
/// let dir = std::env::temp_dir().join("meo-canvas-doc-image-cache");
/// let max_bytes = NonZeroU64::new(1024 * 1024 * 1024).expect("not zero");
/// let store = DiskStore::open(&dir, max_bytes)?;
/// let cache = Arc::new(
///     ImageCache::new(store)
///         .fallback_ttl(Duration::from_secs(60 * 60))
///         .stale_if_error(Duration::from_secs(6 * 60 * 60)),
/// );
/// # Ok::<(), meo_canvas_core::image_cache::StoreError>(())
/// ```
///
/// # Freshness
///
/// The rules are those of a private HTTP cache, [RFC 9111], as Chrome applies
/// them, with stale-if-error from [RFC 5861]:
///
/// - **What is stored**: a `200` within the 32 MiB fetch limit, unless it says
///   `Cache-Control: no-store`. A failure, a timeout, any other status and a
///   body too large are never stored. `private` is stored, since this is a
///   private cache.
/// - **Fresh** for `max-age`, else `Expires` less `Date`, less the response's
///   age on arrival. With neither, for [`fallback_ttl`](Self::fallback_ttl) if
///   set, else a tenth of the time since `Last-Modified`, else not at all.
///   `Expires: 0` is already expired, and an `Expires` that is no date at all
///   counts as absent, as in Chrome. A fresh entry is used with no request.
/// - **Dates** in `Date`, `Expires` and `Last-Modified` are read as Chrome
///   reads them, more leniently than RFC 9110 asks. A value is a date when it
///   names a month, a day and a year, in any order: the three forms RFC 9110
///   names, ISO 8601 (`1994-11-06T08:49:37Z`), three numbers joined by `/` or
///   `-` (month first unless the first is above 12, year first when it is above
///   31), and a month by the first three letters of its name in any case. A
///   one- or two-digit year below 70 is in the 2000s, and a year past 32767 is
///   no date. The time is optional, midnight when absent, and may carry
///   seconds, a fraction, `Z`, or AM or PM. The zone is optional, GMT when
///   absent: GMT, UT, EST, EDT, CST, CDT, MST, MDT, PST, PDT, AST, NST, BST,
///   MET, EET, JST, or an offset such as `+0130`; `UTC` is not one. A value
///   past its range carries over, as a calendar does (`31 Feb` is 3 March), and
///   anything else in the header is ignored.
/// - **Expired**, the entry is revalidated with `If-None-Match` and
///   `If-Modified-Since` from its `ETag` and `Last-Modified`; a `200` replaces
///   the bytes, and a `304` keeps them and replaces the stored headers it
///   sends, one by one, keeping the rest and the stored `ETag`, as Chrome does.
///   With neither validator the entry is fetched again.
/// - **`no-cache`** stores the entry and revalidates it on every use.
/// - **Stale-if-error**: when revalidating fails with a transport error, a
///   timeout or a 5xx, the expired bytes are drawn, with a diagnostic, until
///   the response's own `stale-if-error`, else
///   [`stale_if_error`](Self::stale_if_error), past expiry. `must-revalidate`
///   and `no-cache` allow none, and a cache whose `stale_if_error` is zero
///   never serves stale.
/// - **A clock that moves back** makes an entry stored "in the future" expired,
///   never fresh for longer.
///
/// The key is the URL with its request headers, so an authenticated request
/// never answers an anonymous one and `Vary` needs no handling.
///
/// # Concurrency and deletion
///
/// Requests for one key on several threads share one fetch. A
/// [`delete`](Self::delete) or [`clear`](Self::clear) while a fetch for that
/// key is in flight stops that fetch from storing what it brings back, so a
/// delete is never undone by a request already on the wire.
///
/// # A failing store never fails a render
///
/// A store error is a miss: the image is fetched, and the error is reported
/// on the canvas's diagnostics with the URL as its path. A corrupt entry is
/// also deleted.
///
/// [RFC 9111]: https://www.rfc-editor.org/rfc/rfc9111
/// [RFC 5861]: https://www.rfc-editor.org/rfc/rfc5861
pub struct ImageCache {
    store: Box<dyn ImageStore>,
    fallback_ttl: Option<Duration>,
    stale_if_error: Duration,
    flights: Mutex<HashMap<String, Arc<Flight>>>,
    counts: Counts,
}

impl fmt::Debug for ImageCache {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImageCache")
            .field("fallback_ttl", &self.fallback_ttl)
            .field("stale_if_error", &self.stale_if_error)
            .field("stats", &self.stats())
            .finish_non_exhaustive()
    }
}

/// This cache's request counters, by how each request was answered.
#[derive(Debug, Default)]
struct Counts {
    hits: AtomicU64,
    misses: AtomicU64,
    revalidated: AtomicU64,
    replaced: AtomicU64,
    stale_served: AtomicU64,
}

/// How one request was answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Answer {
    Hit,
    Miss,
    Revalidated,
    Replaced,
    StaleServed,
}

/// What a request for one key ended with, shared with every request that
/// joined it: the bytes and the diagnostics, or the failure.
type Outcome = Result<(Arc<[u8]>, Vec<Diagnostic>), FailedFetch>;

/// A fetch that failed, in the parts of [`Error::SourceFetch`], so each
/// request that shared it gets its own error.
#[derive(Debug, Clone)]
struct FailedFetch {
    url: String,
    detail: String,
    failure: FetchFailure,
}

impl From<FailedFetch> for Error {
    fn from(failed: FailedFetch) -> Self {
        Self::SourceFetch {
            url: failed.url,
            detail: failed.detail,
            failure: failed.failure,
        }
    }
}

/// What the store holds for a request.
enum Lookup {
    /// Fresh bytes, used with no request.
    Fresh(Arc<[u8]>),
    /// An entry to revalidate, or to serve under stale-if-error.
    Expired(Box<ImageCacheEntry>),
    /// Nothing of use.
    Missing,
}

/// One request in flight, which other requests for its key wait on.
#[derive(Debug, Default)]
struct Flight {
    done: Mutex<Option<(Outcome, Answer)>>,
    ready: Condvar,
    /// Set by a delete or a clear, so what this flight fetches is not stored.
    cancelled: AtomicBool,
}

/// A lock, recovered if a panic poisoned it: every update under these locks
/// is completed before the guard drops, so the data is consistent.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What a fetched image yields: its bytes, and what the cache reported.
pub(crate) struct Cached {
    pub(crate) bytes: Arc<[u8]>,
    pub(crate) diagnostics: Vec<Diagnostic>,
}

impl ImageCache {
    /// A cache over `store`, with no fallback lifetime and
    /// [`DEFAULT_STALE_IF_ERROR`].
    ///
    /// To share one store between caches with different settings, pass an
    /// `Arc` of it to each: `Arc<S>` is a store too. Each cache keeps its own
    /// counters; the store's numbers in [`stats`](Self::stats) are the
    /// store's.
    #[must_use]
    pub fn new(store: impl ImageStore + 'static) -> Self {
        Self {
            store: Box::new(store),
            fallback_ttl: None,
            stale_if_error: DEFAULT_STALE_IF_ERROR,
            flights: Mutex::new(HashMap::new()),
            counts: Counts::default(),
        }
    }

    /// How long a response that names no lifetime of its own stays fresh,
    /// replacing the heuristic of a tenth of its age since `Last-Modified`.
    ///
    /// It does not override `max-age`, `Expires` or `no-cache`.
    #[must_use]
    pub const fn fallback_ttl(mut self, ttl: Duration) -> Self {
        self.fallback_ttl = Some(ttl);
        self
    }

    /// How long past expiry a failed revalidation may serve an entry whose
    /// response names no `stale-if-error`; a response's own `stale-if-error`
    /// wins over it. [`Duration::ZERO`] means never serve stale: this cache
    /// then ignores even a response's own `stale-if-error`.
    #[must_use]
    pub const fn stale_if_error(mut self, window: Duration) -> Self {
        self.stale_if_error = window;
        self
    }

    /// What this cache has done, and what its store holds.
    #[must_use]
    pub fn stats(&self) -> CacheStats {
        let store = self.store.stats();
        let read = |counter: &AtomicU64| counter.load(Ordering::Relaxed);
        CacheStats {
            hits: read(&self.counts.hits),
            misses: read(&self.counts.misses),
            revalidated: read(&self.counts.revalidated),
            replaced: read(&self.counts.replaced),
            stale_served: read(&self.counts.stale_served),
            stored: store.stored,
            evicted: store.evicted,
            bytes: store.bytes,
            entries: store.entries,
        }
    }

    /// The key a request is stored under: the URL, then each header as
    /// `name: value` on its own line, in the canonical form the scene sends
    /// (see [`HttpOptions::canonical`]). `http` is what the request sends: a
    /// source's options over the scene's.
    #[must_use]
    pub fn key(url: &str, http: &HttpOptions) -> String {
        let mut key = url.to_owned();
        for (name, value) in http.canonical() {
            key.push('\n');
            key.push_str(&name);
            key.push_str(": ");
            key.push_str(&value);
        }
        key
    }

    /// Removes the entry for `url` requested with `http`, and stops a fetch
    /// for it already in flight from storing its result.
    ///
    /// # Errors
    ///
    /// The store's error, when the entry exists and cannot be removed.
    pub fn delete(
        &self,
        url: &str,
        http: &HttpOptions,
    ) -> Result<(), StoreError> {
        let key = Self::key(url, http);
        if let Some(flight) = lock(&self.flights).get(&key) {
            flight.cancelled.store(true, Ordering::SeqCst);
        }
        self.store.delete(&key)
    }

    /// Removes every entry, and stops every fetch in flight from storing its
    /// result.
    ///
    /// # Errors
    ///
    /// The store's error, when an entry cannot be removed.
    pub fn clear(&self) -> Result<(), StoreError> {
        for flight in lock(&self.flights).values() {
            flight.cancelled.store(true, Ordering::SeqCst);
        }
        self.store.clear()
    }

    /// The bytes for `url`, from the store or the network, sharing a fetch
    /// already in flight for the same key.
    pub(crate) fn fetch(
        &self,
        url: &str,
        http: &HttpOptions,
    ) -> Result<Cached, Error> {
        let key = Self::key(url, http);
        let mut flights = lock(&self.flights);
        let joined = flights.get(&key).map(Arc::clone);
        let leads = joined.is_none();
        let flight = joined.unwrap_or_else(|| {
            let flight = Arc::new(Flight::default());
            flights.insert(key.clone(), Arc::clone(&flight));
            flight
        });
        drop(flights);

        let (outcome, answer) = if leads {
            let lead = Lead {
                cache: self,
                key: &key,
                flight: &flight,
            };
            let found = self.answer(url, http, &key, &flight);
            lead.finish(found.clone());
            found
        } else {
            let mut done = lock(&flight.done);
            loop {
                if let Some(found) = done.as_ref() {
                    break found.clone();
                }
                done = flight
                    .ready
                    .wait(done)
                    .unwrap_or_else(PoisonError::into_inner);
            }
        };

        self.count(answer);
        outcome
            .map(|(bytes, diagnostics)| Cached { bytes, diagnostics })
            .map_err(Error::from)
    }

    fn count(&self, answer: Answer) {
        let counter = match answer {
            Answer::Hit => &self.counts.hits,
            Answer::Miss => &self.counts.misses,
            Answer::Revalidated => &self.counts.revalidated,
            Answer::Replaced => &self.counts.replaced,
            Answer::StaleServed => &self.counts.stale_served,
        };
        counter.fetch_add(1, Ordering::Relaxed);
    }

    fn settings(&self) -> Settings {
        Settings {
            fallback_ttl: self.fallback_ttl.map(duration_ms),
            stale_if_error: duration_ms(self.stale_if_error),
        }
    }

    /// One request, answered: store, then network, then stale-if-error.
    fn answer(
        &self,
        url: &str,
        http: &HttpOptions,
        key: &str,
        flight: &Flight,
    ) -> (Outcome, Answer) {
        let mut notes = Vec::new();
        let now = ms(SystemTime::now());
        let stored = match self.lookup(url, key, now, &mut notes) {
            Lookup::Fresh(bytes) => return (Ok((bytes, notes)), Answer::Hit),
            Lookup::Expired(entry) => Some(*entry),
            Lookup::Missing => None,
        };

        let conditional = stored.as_ref().map_or_else(Vec::new, |entry| {
            freshness::conditional(
                entry.etag.as_deref(),
                entry.last_modified.as_deref(),
            )
        });
        let request = ms(SystemTime::now());
        let fetched = resolve::fetch_response(url, http, &conditional);
        let response = ms(SystemTime::now());

        match (fetched, stored) {
            // A 304 answers the conditional request, sent only when an entry
            // carried a validator; one that arrives unasked is a plain
            // response.
            (Ok(answer), Some(entry))
                if answer.status == NOT_MODIFIED && !conditional.is_empty() =>
            {
                let timing = (request, response);
                self.renewed(
                    url,
                    key,
                    flight,
                    entry,
                    answer.headers,
                    timing,
                    notes,
                )
            }
            (Ok(answer), stored) => {
                let bytes: Arc<[u8]> = Arc::from(answer.body);
                if answer.status == OK {
                    let headers = Headers(&answer.headers);
                    let decided = freshness::freshness(
                        headers,
                        request,
                        response,
                        self.settings(),
                    );
                    if decided.storable {
                        let entry = entry_from(
                            Arc::clone(&bytes),
                            headers,
                            response,
                            &decided,
                        );
                        self.keep(key, &entry, flight, &mut notes, url);
                    } else if stored.is_some()
                        && let Err(error) = self.store.delete(key)
                    {
                        notes.push(Diagnostic::new(url, delete_failed(&error)));
                    }
                }
                // A 200 to a conditional request replaces; with no validator
                // to send, an expired entry is simply fetched again.
                let answered = if conditional.is_empty() {
                    Answer::Miss
                } else {
                    Answer::Replaced
                };
                (Ok((bytes, notes)), answered)
            }
            (Err(error), stored) => self.failed(url, error, stored, now, notes),
        }
    }

    /// The fresh bytes stored for `key`, or the expired entry to revalidate if
    /// one is still of use. A store error is a miss with a
    /// diagnostic, and a corrupt entry is also deleted.
    fn lookup(
        &self,
        url: &str,
        key: &str,
        now: i64,
        notes: &mut Vec<Diagnostic>,
    ) -> Lookup {
        let mut note =
            |detail: String| notes.push(Diagnostic::new(url, detail));
        let entry = match self.store.get(key) {
            Ok(Some(entry)) => entry,
            Ok(None) => return Lookup::Missing,
            Err(error) if error.is_corrupt() => {
                note(format!(
                    "the image cache's entry is corrupt ({error}), so it is \
                     deleted and the image fetched"
                ));
                if let Err(error) = self.store.delete(key) {
                    note(delete_failed(&error));
                }
                return Lookup::Missing;
            }
            Err(error) => {
                note(format!(
                    "the image cache could not read its entry ({error}), so \
                     the image is fetched"
                ));
                return Lookup::Missing;
            }
        };

        if freshness::is_fresh(
            ms(entry.stored_at),
            ms(entry.expires_at),
            entry.revalidate_always,
            now,
        ) {
            return Lookup::Fresh(entry.bytes);
        }
        // Expired, unrevalidatable and past its stale bound: it can never be
        // used again, so it goes now rather than when LRU reaches it.
        let validates = entry.etag.is_some() || entry.last_modified.is_some();
        if !validates
            && !freshness::stale_allowed(
                ms(entry.stale_if_error_until),
                self.settings().stale_if_error,
                now,
            )
        {
            if let Err(error) = self.store.delete(key) {
                note(delete_failed(&error));
            }
            return Lookup::Missing;
        }
        Lookup::Expired(Box::new(entry))
    }

    /// A fetch that failed: the stored bytes under stale-if-error, when the
    /// failure is one a retry might fix and the entry allows it, else the
    /// failure.
    fn failed(
        &self,
        url: &str,
        error: Error,
        stored: Option<ImageCacheEntry>,
        now: i64,
        mut notes: Vec<Diagnostic>,
    ) -> (Outcome, Answer) {
        let failed = match error {
            Error::SourceFetch {
                url,
                detail,
                failure,
            } => FailedFetch {
                url,
                detail,
                failure,
            },
            other => FailedFetch {
                url: url.to_owned(),
                detail: other.to_string(),
                failure: FetchFailure::Other,
            },
        };
        let retryable = matches!(failed.failure, FetchFailure::Transport)
            || matches!(failed.failure, FetchFailure::Status(code) if code >= 500);
        if let Some(entry) = stored
            && retryable
            && freshness::stale_allowed(
                ms(entry.stale_if_error_until),
                self.settings().stale_if_error,
                now,
            )
        {
            notes.push(Diagnostic::new(
                url,
                format!(
                    "revalidating the cached image failed ({}), so the \
                     expired copy is drawn, as stale-if-error allows",
                    failed.detail
                ),
            ));
            return (Ok((entry.bytes, notes)), Answer::StaleServed);
        }
        (Err(failed), Answer::Miss)
    }

    /// The entry a `304 Not Modified` renews: the stored bytes, with the
    /// lifetime the stored headers give once the 304's have replaced them
    /// header by header (RFC 9111 §4.3.4, as Chrome applies it in
    /// `http-cache.tsv`).
    #[expect(
        clippy::too_many_arguments,
        reason = "the rest of `answer`'s state at the moment a 304 arrives; a \
                  struct holding it would exist for this one call"
    )]
    fn renewed(
        &self,
        url: &str,
        key: &str,
        flight: &Flight,
        entry: ImageCacheEntry,
        mut headers: Vec<(String, String)>,
        (request, response): (i64, i64),
        mut notes: Vec<Diagnostic>,
    ) -> (Outcome, Answer) {
        // The stored headers, each replaced by the 304's where it sends one,
        // and the stored ETag kept whatever the 304 sends, as Chrome does in
        // `http-cache.tsv`'s "304 with ..." rows.
        headers.retain(|(name, _)| !name.eq_ignore_ascii_case("etag"));
        for (name, value) in [
            ("etag", entry.etag.as_ref()),
            ("last-modified", entry.last_modified.as_ref()),
            ("cache-control", entry.cache_control.as_ref()),
            ("expires", entry.expires.as_ref()),
            ("pragma", entry.pragma.as_ref()),
            ("date", entry.date.as_ref()),
            ("age", entry.age.as_ref()),
        ] {
            if let Some(value) = value
                && Headers(&headers).get(name).is_none()
            {
                headers.push((name.to_owned(), value.clone()));
            }
        }
        let decided = freshness::freshness(
            Headers(&headers),
            request,
            response,
            self.settings(),
        );
        let renewed =
            entry_from(entry.bytes, Headers(&headers), response, &decided);
        if decided.storable {
            self.keep(key, &renewed, flight, &mut notes, url);
        } else if let Err(error) = self.store.delete(key) {
            notes.push(Diagnostic::new(url, delete_failed(&error)));
        }
        (Ok((renewed.bytes, notes)), Answer::Revalidated)
    }

    /// Stores `entry` unless a delete or clear cancelled this flight, before
    /// or during the write.
    fn keep(
        &self,
        key: &str,
        entry: &ImageCacheEntry,
        flight: &Flight,
        notes: &mut Vec<Diagnostic>,
        url: &str,
    ) {
        if flight.cancelled.load(Ordering::SeqCst) {
            return;
        }
        if let Err(error) = self.store.set(key, entry) {
            notes.push(Diagnostic::new(
                url,
                format!(
                    "the image cache could not store the image ({error}); the \
                     render draws the fetched bytes"
                ),
            ));
            return;
        }
        // A delete that arrived during the write marked the flight after the
        // check above; undoing the write here is what keeps it deleted.
        if flight.cancelled.load(Ordering::SeqCst)
            && let Err(error) = self.store.delete(key)
        {
            notes.push(Diagnostic::new(url, delete_failed(&error)));
        }
    }
}

/// The leader of a flight, which finishes it and takes it out of the map even
/// when the request panics, so no waiter waits forever.
struct Lead<'a> {
    cache: &'a ImageCache,
    key: &'a str,
    flight: &'a Arc<Flight>,
}

impl Lead<'_> {
    fn finish(self, found: (Outcome, Answer)) {
        *lock(&self.flight.done) = Some(found);
        // `Drop` takes it out of the map and wakes the waiters.
    }
}

impl Drop for Lead<'_> {
    fn drop(&mut self) {
        {
            let mut done = lock(&self.flight.done);
            if done.is_none() {
                *done = Some((
                    Err(FailedFetch {
                        url: self
                            .key
                            .lines()
                            .next()
                            .unwrap_or_default()
                            .to_owned(),
                        detail: "the request this one shared did not finish"
                            .to_owned(),
                        failure: FetchFailure::Other,
                    }),
                    Answer::Miss,
                ));
            }
        }
        let mut flights = lock(&self.cache.flights);
        if flights
            .get(self.key)
            .is_some_and(|held| Arc::ptr_eq(held, self.flight))
        {
            flights.remove(self.key);
        }
        drop(flights);
        self.flight.ready.notify_all();
    }
}

/// `200 OK`, the one status stored.
const OK: u16 = 200;

/// `304 Not Modified`, the answer to a revalidation that keeps the bytes.
const NOT_MODIFIED: u16 = 304;

fn delete_failed(error: &StoreError) -> String {
    format!("the image cache could not delete its entry ({error})")
}

/// The entry for a response received at `response`.
fn entry_from(
    bytes: Arc<[u8]>,
    headers: Headers<'_>,
    response: i64,
    decided: &freshness::Freshness,
) -> ImageCacheEntry {
    ImageCacheEntry {
        bytes,
        etag: headers.get("etag"),
        last_modified: headers.get("last-modified"),
        cache_control: headers.get("cache-control"),
        expires: headers.get("expires"),
        pragma: headers.get("pragma"),
        date: headers.get("date"),
        age: headers.get("age"),
        stored_at: time(response),
        expires_at: time(decided.expires_at),
        stale_if_error_until: time(decided.stale_if_error_until),
        revalidate_always: decided.revalidate_always,
    }
}

/// Milliseconds since the epoch, negative before it, saturating far past it.
pub(crate) fn ms(time: SystemTime) -> i64 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(after) => i64::try_from(after.as_millis()).unwrap_or(i64::MAX),
        Err(before) => i64::try_from(before.duration().as_millis())
            .map_or(i64::MIN, |ms| -ms),
    }
}

/// The instant `ms` milliseconds after the epoch.
pub(crate) fn time(ms: i64) -> SystemTime {
    let magnitude = Duration::from_millis(ms.unsigned_abs());
    if ms >= 0 {
        UNIX_EPOCH.checked_add(magnitude)
    } else {
        UNIX_EPOCH.checked_sub(magnitude)
    }
    .unwrap_or(UNIX_EPOCH)
}

pub(crate) fn duration_ms(duration: Duration) -> i64 {
    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use meo_canvas_scene::node::HttpOptions;

    use super::{ImageCache, disk::file_name};

    /// Every row of `keys.tsv`: the key a request is stored under and the
    /// file the disk store names for it, as the npm surface builds them.
    #[test]
    fn every_row_of_the_shared_key_table() {
        let table = include_str!("../tests/assets/image-cache/keys.tsv");
        let mut count = 0;
        for line in table.lines().filter(|line| !line.starts_with('#')).skip(1)
        {
            let c: Vec<&str> = line.split('\t').collect();
            assert_eq!(c.len(), 5, "{line}");
            let mut http = HttpOptions::new();
            if c[2] != "-" {
                for item in c[2].split(" | ") {
                    let (name, value) =
                        item.split_once(": ").unwrap_or_else(|| {
                            unreachable!("not `name: value`: {item}")
                        });
                    http = http.header(name, value.replace("\\t", "\t"));
                }
            }
            let key = ImageCache::key(c[1], &http);
            assert_eq!(key, c[3].replace("\\n", "\n"), "{}", c[0]);
            assert_eq!(file_name(&key), c[4], "{}", c[0]);
            count += 1;
        }
        assert_eq!(count, 8, "every row was read");
    }
}
