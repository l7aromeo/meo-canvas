//! [`DiskStore`]: one file per entry in a directory, in a format the npm
//! surface's `diskImageStore` reads and writes too.
//!
//! # The file
//!
//! Named by the SHA-256 of the key in lower-case hex, then `.entry`. All
//! integers little-endian:
//!
//! | bytes | field                                             |
//! | ----- | ------------------------------------------------- |
//! | 8     | magic, `MEOIMGC\n`                                |
//! | 4     | version, `1`                                      |
//! | 4     | metadata length                                   |
//! | 8     | body length                                       |
//! | 32    | SHA-256 of the metadata then the body             |
//! | ...   | metadata, then body                               |
//!
//! The metadata is `stored_at`, `expires_at` and `stale_if_error_until` as
//! `u64` milliseconds since the epoch, a flags byte, then eight strings, each
//! a `u32` length and UTF-8: the key, then the `ETag`, `Last-Modified`,
//! `Cache-Control`, `Expires`, `Pragma`, `Date` and `Age`. Flag bit 0 is
//! revalidate always, and bits 1 to 7 say which of those seven headers the
//! response had; an absent one is written empty. `entry.tsv` and the file
//! beside it pin the layout on both surfaces.

use std::{
    collections::HashMap,
    fs::{self, File},
    io::{ErrorKind, Read as _, Write as _},
    num::NonZeroU64,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, SystemTime},
};

use sha2::{Digest as _, Sha256};

use super::{
    DEFAULT_MAX_ENTRY_BYTES, ImageCacheEntry, ImageStore, StoreError,
    StoreStats, lock, ms, time,
};

/// The first bytes of every entry file.
const MAGIC: &[u8; 8] = b"MEOIMGC\n";

/// The layout this build reads and writes.
const VERSION: u32 = 1;

/// Magic, version, the two lengths and the checksum.
const HEADER_BYTES: usize = 8 + 4 + 4 + 8 + 32;

/// Three times and the flags byte, before the strings.
const FIXED_META_BYTES: usize = 8 * 3 + 1;

/// The flag bit for an entry that revalidates on every use.
const REVALIDATE_ALWAYS: u8 = 1;

/// The flag bit for an entry with an `ETag`.
const HAS_ETAG: u8 = 2;

/// The flag bit for an entry with a `Last-Modified`.
const HAS_LAST_MODIFIED: u8 = 4;

/// The flag bit for each stored header after `Last-Modified`, in file order:
/// `Cache-Control`, `Expires`, `Pragma`, `Date`, `Age`.
const HAS_HEADER: [u8; 5] = [8, 16, 32, 64, 128];

/// The suffix of an entry file.
const ENTRY_SUFFIX: &str = ".entry";

/// The prefix of a file being written, so no lookup ever reads one.
const TEMP_PREFIX: &str = "tmp-";

/// How old a temp file must be before [`DiskStore::open`] removes it: an hour.
///
/// A younger one may belong to a writer in another process; writing one entry,
/// at most 32 MiB to a local disk, takes seconds, so an hour old is a crash.
/// Requires the `net` feature.
pub const TEMP_FILE_MAX_AGE: Duration = Duration::from_hours(1);

/// Hex digits in a SHA-256.
const HASH_HEX: usize = 64;

/// Hex digits in the part of a temp file's name that tells writers apart.
const UNIQUE_HEX: usize = 16;

/// Entries as files in one directory, evicted oldest first by modification
/// time once they pass `max_bytes`, and kept across restarts.
///
/// Requires the `net` feature. A Node process using the npm surface's
/// `diskImageStore` can share the directory: the two write the same files.
///
/// ```
/// use std::{num::NonZeroU64, sync::Arc};
///
/// use meo_canvas_core::image_cache::{DiskStore, ImageCache};
///
/// // At start-up, once: 2 GiB under the given directory.
/// let dir = std::env::temp_dir().join("meo-canvas-doc-disk-store");
/// let max = NonZeroU64::new(2 * 1024 * 1024 * 1024).expect("not zero");
/// let cache = Arc::new(ImageCache::new(DiskStore::open(&dir, max)?));
/// # Ok::<(), meo_canvas_core::image_cache::StoreError>(())
/// ```
///
/// # What it guarantees
///
/// **The files are the truth.** The size the store keeps in memory is an
/// estimate that a directory scan corrects; anything it finds missing or
/// wrong is a miss, never an error that reaches a render.
///
/// - **Writes are whole.** An entry is written to a temp file in the same
///   directory, synced, and renamed into place, so a partial file never has an
///   entry's name. Two processes writing one key each rename their own file;
///   the last wins, and a reader sees one whole file or the other.
/// - **Corrupt files are dropped.** A wrong magic, version, length or checksum
///   is a miss with a diagnostic, and the file is deleted. A file holding
///   another key is a miss and is left alone, since it may be valid for its own
///   key.
/// - **Files removed from outside are misses.** Deleted by hand, by a cleanup
///   job or by another process's eviction, an entry is fetched and stored again
///   with no diagnostic. If the whole directory goes, the next write creates it
///   again.
/// - **Eviction** removes the least recently used entries, by modification
///   time, which a hit refreshes, until the total is under `max_bytes`; entries
///   with the same time go in file-name order. A file another process already
///   removed is skipped, and the entry just written is never the one evicted.
///   An entry larger than `max_entry_bytes`, or than `max_bytes`, is not
///   stored.
/// - **Failed writes cost nothing but the cache.** A full disk, a write error
///   or a read-only directory removes the temp file and reports a diagnostic;
///   the render draws the bytes it fetched. A rename the system refuses stores
///   nothing and reports nothing; the next render tries again.
/// - **Crashes leave temp files**, named `tmp-...`, which no lookup reads.
///   [`open`](Self::open) removes those older than [`TEMP_FILE_MAX_AGE`].
/// - **[`clear`](ImageStore::clear) removes only the store's own files**:
///   entries and temp files, never anything else a caller keeps in the
///   directory.
///
/// The bytes [`get`](ImageStore::get) returns are the caller's own copy.
pub struct DiskStore {
    dir: PathBuf,
    max_bytes: u64,
    max_entry_bytes: u64,
    state: Mutex<State>,
    /// A write error to report instead of writing, so a test can fill the
    /// disk.
    #[cfg(test)]
    fail_writes: std::sync::atomic::AtomicBool,
    /// Called between opening an entry and reading it.
    #[cfg(test)]
    after_open: Mutex<Option<Hook>>,
}

/// What a test runs between opening an entry file and reading it.
#[cfg(test)]
type Hook = Box<dyn Fn(&Path) + Send>;

impl std::fmt::Debug for DiskStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DiskStore")
            .field("dir", &self.dir)
            .field("max_bytes", &self.max_bytes)
            .field("max_entry_bytes", &self.max_entry_bytes)
            .field("stats", &self.stats())
            .finish_non_exhaustive()
    }
}

/// The store's estimate of what the directory holds.
#[derive(Debug, Default)]
struct State {
    /// Each entry file's size, by name.
    sizes: HashMap<String, u64>,
    total: u64,
    stored: u64,
    evicted: u64,
}

impl State {
    fn record(&mut self, name: &str, size: u64) {
        if let Some(old) = self.sizes.insert(name.to_owned(), size) {
            self.total -= old;
        }
        self.total += size;
    }

    fn forget(&mut self, name: &str) {
        if let Some(old) = self.sizes.remove(name) {
            self.total -= old;
        }
    }

    /// The directory is gone, and with it every entry.
    fn empty(&mut self) {
        self.sizes.clear();
        self.total = 0;
    }
}

impl DiskStore {
    /// A store in `dir`, created if missing, holding up to `max_bytes` of
    /// files and keeping bodies up to the smaller of `max_bytes` and
    /// [`DEFAULT_MAX_ENTRY_BYTES`](super::DEFAULT_MAX_ENTRY_BYTES).
    ///
    /// Scans the directory: entries already there count toward `max_bytes`,
    /// the oldest are evicted if they pass it, and temp files older than
    /// [`TEMP_FILE_MAX_AGE`] are removed.
    ///
    /// # Errors
    ///
    /// A [`StoreError`] when the directory cannot be created or read.
    pub fn open(
        dir: impl Into<PathBuf>,
        max_bytes: NonZeroU64,
    ) -> Result<Self, StoreError> {
        let max_entry_bytes = DEFAULT_MAX_ENTRY_BYTES.min(max_bytes.get());
        Self::open_with_max_entry_bytes(dir, max_bytes, max_entry_bytes)
    }

    /// [`open`](Self::open), keeping bodies up to `max_entry_bytes`.
    ///
    /// # Errors
    ///
    /// A [`StoreError`] when `max_entry_bytes` is above `max_bytes`, refused
    /// rather than clamped, or when the directory cannot be created or read.
    pub fn open_with_max_entry_bytes(
        dir: impl Into<PathBuf>,
        max_bytes: NonZeroU64,
        max_entry_bytes: u64,
    ) -> Result<Self, StoreError> {
        if max_entry_bytes > max_bytes.get() {
            return Err(StoreError::new(format!(
                "max_entry_bytes ({max_entry_bytes}) is above max_bytes ({})",
                max_bytes.get()
            )));
        }
        let store = Self {
            dir: dir.into(),
            max_bytes: max_bytes.get(),
            max_entry_bytes,
            state: Mutex::new(State::default()),
            #[cfg(test)]
            fail_writes: std::sync::atomic::AtomicBool::new(false),
            #[cfg(test)]
            after_open: Mutex::new(None),
        };
        fs::create_dir_all(&store.dir)?;
        let now = SystemTime::now();
        for listed in fs::read_dir(&store.dir)? {
            let listed = listed?;
            let name = listed.file_name().to_string_lossy().into_owned();
            if is_temp(&name)
                && listed
                    .metadata()
                    .ok()
                    .and_then(|meta| meta.modified().ok())
                    .and_then(|modified| now.duration_since(modified).ok())
                    .is_some_and(|age| age > TEMP_FILE_MAX_AGE)
            {
                // Another user's file, or one a writer just finished: either
                // way not this store's to insist on.
                let _ = remove(&listed.path());
            }
        }
        store.evict(None)?;
        Ok(store)
    }

    /// The directory the entries are in.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn state(&self) -> MutexGuard<'_, State> {
        lock(&self.state)
    }

    /// Rescans the directory, correcting the estimate, then removes the
    /// oldest entries until the total is under `max_bytes`, sparing `keep`.
    fn evict(&self, keep: Option<&str>) -> Result<(), StoreError> {
        let mut found = Vec::new();
        let listing = match fs::read_dir(&self.dir) {
            Ok(listing) => listing,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                self.state().empty();
                return Ok(());
            }
            Err(error) => return Err(error.into()),
        };
        for listed in listing {
            let listed = listed?;
            let name = listed.file_name().to_string_lossy().into_owned();
            if !is_entry(&name) {
                continue;
            }
            // Gone since the listing: another process evicted it.
            let Ok(meta) = listed.metadata() else {
                continue;
            };
            let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            found.push((modified, name, meta.len()));
        }
        found.sort();

        let mut sizes: HashMap<String, u64> = found
            .iter()
            .map(|(_, name, size)| (name.clone(), *size))
            .collect();
        let mut total: u64 = sizes.values().sum();
        let mut evicted = 0;
        for (_, name, size) in &found {
            if total <= self.max_bytes {
                break;
            }
            if Some(name.as_str()) == keep {
                continue;
            }
            match fs::remove_file(self.dir.join(name)) {
                Ok(()) => evicted += 1,
                Err(error) if error.kind() == ErrorKind::NotFound => {}
                // Held open on Windows, or not ours to remove: left for the
                // next eviction, and the total stays honest about it.
                Err(_) => continue,
            }
            sizes.remove(name);
            total -= size;
        }
        let mut state = self.state();
        state.sizes = sizes;
        state.total = total;
        state.evicted += evicted;
        drop(state);
        Ok(())
    }

    /// Writes `bytes` to a temp file beside `name` and renames it into place.
    /// `Ok(false)` when the rename was refused.
    fn write(&self, name: &str, bytes: &[u8]) -> Result<bool, StoreError> {
        fs::create_dir_all(&self.dir)?;
        let temp = self.dir.join(format!(
            "{TEMP_PREFIX}{}-{}",
            &name[..HASH_HEX],
            unique()
        ));
        let written = (|| {
            let mut file =
                File::options().write(true).create_new(true).open(&temp)?;
            #[cfg(test)]
            if self.fail_writes.load(Ordering::SeqCst) {
                return Err(std::io::Error::new(
                    ErrorKind::StorageFull,
                    "no space left on device (injected)",
                ));
            }
            file.write_all(bytes)?;
            file.sync_all()
        })();
        if let Err(error) = written {
            let _ = fs::remove_file(&temp);
            return Err(error.into());
        }
        if fs::rename(&temp, self.dir.join(name)).is_err() {
            let _ = fs::remove_file(&temp);
            return Ok(false);
        }
        Ok(true)
    }
}

impl ImageStore for DiskStore {
    fn get(&self, key: &str) -> Result<Option<ImageCacheEntry>, StoreError> {
        let name = file_name(key);
        let path = self.dir.join(&name);
        let read = File::open(&path).and_then(|mut file| {
            #[cfg(test)]
            if let Some(hook) = lock(&self.after_open).as_ref() {
                hook(&path);
            }
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)?;
            Ok(bytes)
        });
        let bytes = match read {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                self.state().forget(&name);
                return Ok(None);
            }
            Err(error) => return Err(error.into()),
        };
        let Some(entry) = decode(&bytes, key)? else {
            return Ok(None);
        };
        // A hit makes the entry the newest. Failing to is not worth a
        // diagnostic: the entry is merely evicted a little sooner.
        let _ = File::options()
            .append(true)
            .open(&path)
            .and_then(|file| file.set_modified(SystemTime::now()));
        self.state().record(&name, bytes.len() as u64);
        Ok(Some(entry))
    }

    fn set(
        &self,
        key: &str,
        entry: &ImageCacheEntry,
    ) -> Result<(), StoreError> {
        if entry.bytes.len() as u64 > self.max_entry_bytes {
            return Ok(());
        }
        let bytes = encode(key, entry);
        if bytes.len() as u64 > self.max_bytes {
            return Ok(());
        }
        let name = file_name(key);
        if !self.write(&name, &bytes)? {
            return Ok(());
        }
        let over = {
            let mut state = self.state();
            state.record(&name, bytes.len() as u64);
            state.stored += 1;
            state.total > self.max_bytes
        };
        if over {
            self.evict(Some(&name))?;
        }
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<(), StoreError> {
        let name = file_name(key);
        remove(&self.dir.join(&name))?;
        self.state().forget(&name);
        Ok(())
    }

    fn clear(&self) -> Result<(), StoreError> {
        let listing = match fs::read_dir(&self.dir) {
            Ok(listing) => listing,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                self.state().empty();
                return Ok(());
            }
            Err(error) => return Err(error.into()),
        };
        let mut first_error = None;
        for listed in listing {
            let listed = listed?;
            let name = listed.file_name().to_string_lossy().into_owned();
            if !is_entry(&name) && !is_temp(&name) {
                continue;
            }
            match remove(&listed.path()) {
                Ok(()) => self.state().forget(&name),
                Err(error) => {
                    first_error.get_or_insert(error);
                }
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    fn stats(&self) -> StoreStats {
        let state = self.state();
        StoreStats {
            stored: state.stored,
            evicted: state.evicted,
            bytes: state.total,
            entries: state.sizes.len() as u64,
        }
    }
}

/// Removes a file, counting one already gone as removed.
fn remove(path: &Path) -> Result<(), StoreError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

/// The file an entry for `key` lives in.
pub(crate) fn file_name(key: &str) -> String {
    let mut name = hex(&Sha256::digest(key.as_bytes()));
    name.push_str(ENTRY_SUFFIX);
    name
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from(DIGITS[usize::from(byte >> 4)]));
        out.push(char::from(DIGITS[usize::from(byte & 0xf)]));
    }
    out
}

fn is_hex(text: &str, len: usize) -> bool {
    text.len() == len
        && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Whether `name` is one of the store's entry files.
fn is_entry(name: &str) -> bool {
    name.strip_suffix(ENTRY_SUFFIX)
        .is_some_and(|hash| is_hex(hash, HASH_HEX))
}

/// Whether `name` is one of the store's temp files.
fn is_temp(name: &str) -> bool {
    name.strip_prefix(TEMP_PREFIX)
        .and_then(|rest| rest.split_once('-'))
        .is_some_and(|(hash, unique)| {
            is_hex(hash, HASH_HEX) && is_hex(unique, UNIQUE_HEX)
        })
}

/// Sixteen hex digits no other writer of this directory uses at the same
/// moment: this process, a counter, and the clock.
fn unique() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let count = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |since| u64::from(since.subsec_nanos()));
    let low = (count.wrapping_mul(0x9E37_79B9) ^ nanos) & 0xFFFF_FFFF;
    format!("{:016x}", (u64::from(std::process::id()) << 32) | low)
}

/// The entry file for `key`.
pub(crate) fn encode(key: &str, entry: &ImageCacheEntry) -> Vec<u8> {
    let mut meta = Vec::with_capacity(FIXED_META_BYTES + key.len() + 128);
    for when in [
        entry.stored_at,
        entry.expires_at,
        entry.stale_if_error_until,
    ] {
        meta.extend_from_slice(
            &u64::try_from(ms(when)).unwrap_or(0).to_le_bytes(),
        );
    }
    let mut flags = 0;
    if entry.revalidate_always {
        flags |= REVALIDATE_ALWAYS;
    }
    if entry.etag.is_some() {
        flags |= HAS_ETAG;
    }
    if entry.last_modified.is_some() {
        flags |= HAS_LAST_MODIFIED;
    }
    let headers = stored_headers(entry);
    for (bit, header) in HAS_HEADER.iter().zip(headers) {
        if header.is_some() {
            flags |= bit;
        }
    }
    meta.push(flags);
    let texts = [
        Some(key),
        entry.etag.as_deref(),
        entry.last_modified.as_deref(),
    ]
    .into_iter()
    .chain(headers);
    for text in texts.map(Option::unwrap_or_default) {
        meta.extend_from_slice(&(text.len() as u32).to_le_bytes());
        meta.extend_from_slice(text.as_bytes());
    }

    let mut checksum = Sha256::new();
    checksum.update(&meta);
    checksum.update(&entry.bytes);

    let mut file =
        Vec::with_capacity(HEADER_BYTES + meta.len() + entry.bytes.len());
    file.extend_from_slice(MAGIC);
    file.extend_from_slice(&VERSION.to_le_bytes());
    file.extend_from_slice(&(meta.len() as u32).to_le_bytes());
    file.extend_from_slice(&(entry.bytes.len() as u64).to_le_bytes());
    file.extend_from_slice(&checksum.finalize());
    file.extend_from_slice(&meta);
    file.extend_from_slice(&entry.bytes);
    file
}

/// The headers an entry keeps after its validators, in file order.
fn stored_headers(entry: &ImageCacheEntry) -> [Option<&str>; 5] {
    [
        entry.cache_control.as_deref(),
        entry.expires.as_deref(),
        entry.pragma.as_deref(),
        entry.date.as_deref(),
        entry.age.as_deref(),
    ]
}

/// Reads fixed-width fields off the front of a slice.
struct Cursor<'a>(&'a [u8]);

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], StoreError> {
        if self.0.len() < n {
            return Err(StoreError::corrupt(
                "the file is shorter than it says",
            ));
        }
        let (head, rest) = self.0.split_at(n);
        self.0 = rest;
        Ok(head)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], StoreError> {
        let mut out = [0; N];
        out.copy_from_slice(self.take(N)?);
        Ok(out)
    }

    fn text(&mut self) -> Result<&'a str, StoreError> {
        let len = u32::from_le_bytes(self.array()?) as usize;
        std::str::from_utf8(self.take(len)?).map_err(|_| {
            StoreError::corrupt("a string in the file is not UTF-8")
        })
    }
}

/// The entry in `file`, `Ok(None)` if it holds another key, or
/// [`StoreError::corrupt`] if it is not an entry file this build reads.
pub(crate) fn decode(
    file: &[u8],
    key: &str,
) -> Result<Option<ImageCacheEntry>, StoreError> {
    let mut at = Cursor(file);
    if at.take(MAGIC.len())? != MAGIC {
        return Err(StoreError::corrupt("the file does not begin as an entry"));
    }
    let version = u32::from_le_bytes(at.array()?);
    if version != VERSION {
        return Err(StoreError::corrupt(format!(
            "the file is version {version}, and this build reads {VERSION}"
        )));
    }
    let meta_len = u32::from_le_bytes(at.array()?) as usize;
    let body_len = u64::from_le_bytes(at.array()?);
    let checksum: [u8; 32] = at.array()?;
    if usize::try_from(body_len)
        .ok()
        .and_then(|body| body.checked_add(meta_len))
        != Some(at.0.len())
    {
        return Err(StoreError::corrupt(format!(
            "the file holds {} bytes after its header where it says {}",
            at.0.len(),
            meta_len as u64 + body_len
        )));
    }
    if Sha256::digest(at.0).as_slice() != checksum {
        return Err(StoreError::corrupt("the checksum does not match"));
    }

    let mut meta = Cursor(at.take(meta_len)?);
    let body = at.0;
    let mut when = || {
        meta.array().map(|bytes| {
            time(i64::try_from(u64::from_le_bytes(bytes)).unwrap_or(i64::MAX))
        })
    };
    let (stored_at, expires_at, stale_if_error_until) =
        (when()?, when()?, when()?);
    let [flags] = meta.array()?;
    let held = meta.text()?;
    let etag = meta.text()?;
    let last_modified = meta.text()?;
    let mut headers = [""; 5];
    for header in &mut headers {
        *header = meta.text()?;
    }
    if !meta.0.is_empty() {
        return Err(StoreError::corrupt("the metadata runs past its fields"));
    }
    if held != key {
        return Ok(None);
    }
    let present =
        |bit: u8, text: &str| (flags & bit != 0).then(|| text.to_owned());
    Ok(Some(ImageCacheEntry {
        bytes: Arc::from(body),
        etag: present(HAS_ETAG, etag),
        last_modified: present(HAS_LAST_MODIFIED, last_modified),
        cache_control: present(HAS_HEADER[0], headers[0]),
        expires: present(HAS_HEADER[1], headers[1]),
        pragma: present(HAS_HEADER[2], headers[2]),
        date: present(HAS_HEADER[3], headers[3]),
        age: present(HAS_HEADER[4], headers[4]),
        stored_at,
        expires_at,
        stale_if_error_until,
        revalidate_always: flags & REVALIDATE_ALWAYS != 0,
    }))
}

#[cfg(test)]
mod tests {
    use std::{
        fs::{self, File},
        num::NonZeroU64,
        path::{Path, PathBuf},
        sync::{
            Arc,
            atomic::{AtomicU64, Ordering},
        },
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    use super::{
        DiskStore, TEMP_FILE_MAX_AGE, decode, encode, file_name, is_entry,
        is_temp,
    };
    use crate::image_cache::{ImageCacheEntry, ImageStore, StoreError};

    /// A directory of its own, removed when the test ends.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let dir = std::env::temp_dir().join(format!(
                "meo-canvas-disk-store-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = fs::remove_dir_all(&dir);
            Self(dir)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt as _;
                let _ = fs::set_permissions(
                    &self.0,
                    fs::Permissions::from_mode(0o755),
                );
            }
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn bytes(n: u64) -> NonZeroU64 {
        NonZeroU64::new(n).unwrap_or(NonZeroU64::MIN)
    }

    fn open(dir: &Path, max: u64) -> DiskStore {
        DiskStore::open(dir, bytes(max))
            .unwrap_or_else(|error| unreachable!("open: {error}"))
    }

    fn entry(body: &[u8]) -> ImageCacheEntry {
        ImageCacheEntry {
            bytes: Arc::from(body),
            etag: Some("\"e\"".to_owned()),
            last_modified: None,
            cache_control: None,
            expires: None,
            pragma: None,
            date: None,
            age: None,
            stored_at: UNIX_EPOCH + Duration::from_secs(1_000),
            expires_at: UNIX_EPOCH + Duration::from_secs(1_060),
            stale_if_error_until: UNIX_EPOCH + Duration::from_secs(2_000),
            revalidate_always: false,
        }
    }

    fn set(store: &DiskStore, key: &str, entry: &ImageCacheEntry) {
        store
            .set(key, entry)
            .unwrap_or_else(|error| unreachable!("set {key}: {error}"));
    }

    fn get(store: &DiskStore, key: &str) -> Option<ImageCacheEntry> {
        store
            .get(key)
            .unwrap_or_else(|error| unreachable!("get {key}: {error}"))
    }

    fn touch(path: &Path, at: SystemTime) {
        File::options()
            .append(true)
            .open(path)
            .and_then(|file| file.set_modified(at))
            .unwrap_or_else(|error| unreachable!("touch: {error}"));
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .map(|listing| {
                listing
                    .filter_map(Result::ok)
                    .map(|listed| {
                        listed.file_name().to_string_lossy().into_owned()
                    })
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }

    fn corrupt(found: Result<Option<ImageCacheEntry>, StoreError>) -> bool {
        found.is_err_and(|error| error.is_corrupt())
    }

    /// The shared table's entry is written byte for byte as the committed
    /// file, which both surfaces read.
    #[test]
    fn the_shared_entry_file_round_trips() {
        let table = include_str!("../../tests/assets/image-cache/entry.tsv");
        let row: Vec<&str> = table
            .lines()
            .filter(|line| !line.starts_with('#'))
            .nth(1)
            .unwrap_or_default()
            .split('\t')
            .collect();
        assert_eq!(row.len(), 14);
        let at = |value: &str| {
            UNIX_EPOCH
                + Duration::from_millis(value.parse().unwrap_or_else(|_| {
                    unreachable!("not milliseconds: {value}")
                }))
        };
        let header = |value: &str| (value != "-").then(|| value.to_owned());
        let body: Vec<u8> = (0..row[12].len())
            .step_by(2)
            .map(|at| u8::from_str_radix(&row[12][at..at + 2], 16).unwrap_or(0))
            .collect();
        let key = row[0].replace("\\n", "\n");
        let wanted = ImageCacheEntry {
            bytes: Arc::from(body),
            etag: header(row[1]),
            last_modified: header(row[2]),
            cache_control: header(row[3]),
            expires: header(row[4]),
            pragma: header(row[5]),
            date: header(row[6]),
            age: header(row[7]),
            stored_at: at(row[8]),
            expires_at: at(row[9]),
            stale_if_error_until: at(row[10]),
            revalidate_always: row[11] == "yes",
        };
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/assets/image-cache")
            .join(row[13]);
        let committed = fs::read(&path)
            .unwrap_or_else(|error| unreachable!("{path:?}: {error}"));
        assert_eq!(file_name(&key), row[13]);
        assert_eq!(encode(&key, &wanted), committed, "written as committed");
        let read = decode(&committed, &key)
            .unwrap_or_else(|error| unreachable!("decode: {error}"));
        assert_eq!(read, Some(wanted.clone()), "read back whole");

        // And through the store, from a directory holding only that file.
        let scratch = Scratch::new();
        fs::create_dir_all(&scratch.0)
            .unwrap_or_else(|error| unreachable!("{error}"));
        fs::copy(&path, scratch.0.join(row[13]))
            .unwrap_or_else(|error| unreachable!("{error}"));
        let store = open(&scratch.0, 1 << 20);
        assert_eq!(store.stats().entries, 1, "counted by the opening scan");
        assert_eq!(get(&store, &key), Some(wanted));
    }

    #[test]
    fn an_entry_persists_across_two_store_instances() {
        let scratch = Scratch::new();
        set(&open(&scratch.0, 1 << 20), "k", &entry(b"abc"));
        let again = open(&scratch.0, 1 << 20);
        assert_eq!(
            get(&again, "k").map(|e| e.bytes),
            Some(Arc::from(&b"abc"[..]))
        );
        assert_eq!(
            again.stats().bytes,
            fs::metadata(scratch.0.join(file_name("k"))).map_or(0, |m| m.len())
        );
    }

    #[test]
    fn delete_twice_is_harmless() {
        let scratch = Scratch::new();
        let store = open(&scratch.0, 1 << 20);
        set(&store, "k", &entry(b"abc"));
        assert!(store.delete("k").is_ok());
        assert!(store.delete("k").is_ok(), "already gone is success");
        assert_eq!(get(&store, "k"), None);
        assert_eq!((store.stats().entries, store.stats().bytes), (0, 0));
    }

    #[test]
    fn clear_removes_only_the_store_s_own_files() {
        let scratch = Scratch::new();
        let store = open(&scratch.0, 1 << 20);
        set(&store, "a", &entry(b"a"));
        set(&store, "b", &entry(b"b"));
        let temp = format!("tmp-{}-0123456789abcdef", &file_name("c")[..64]);
        for kept in ["notes.txt", "a.entry", "tmp-x"] {
            fs::write(scratch.0.join(kept), b"the caller's")
                .unwrap_or_else(|error| unreachable!("{error}"));
        }
        fs::write(scratch.0.join(&temp), b"partial")
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert!(store.clear().is_ok());
        assert_eq!(names(&scratch.0), ["a.entry", "notes.txt", "tmp-x"]);
        assert_eq!(store.stats().entries, 0);
    }

    #[test]
    fn eviction_removes_the_oldest_by_modification_time() {
        let scratch = Scratch::new();
        let one = encode("k1", &entry(&[1; 100])).len() as u64;
        let store = open(&scratch.0, 3 * one);
        for key in ["k1", "k2", "k3"] {
            set(&store, key, &entry(&[1; 100]));
        }
        // k2 oldest, then k3, then k1.
        let base = SystemTime::now() - Duration::from_secs(600);
        for (key, age) in [("k2", 0), ("k3", 10), ("k1", 20)] {
            touch(
                &scratch.0.join(file_name(key)),
                base + Duration::from_secs(age),
            );
        }
        set(&store, "k4", &entry(&[1; 100]));
        assert_eq!(get(&store, "k2"), None, "the oldest went");
        assert!(get(&store, "k3").is_some() && get(&store, "k1").is_some());
        assert!(get(&store, "k4").is_some(), "the new entry stays");
        assert_eq!(store.stats().evicted, 1);
    }

    #[test]
    fn a_hit_makes_the_entry_the_newest() {
        let scratch = Scratch::new();
        let one = encode("k1", &entry(&[1; 100])).len() as u64;
        let store = open(&scratch.0, 2 * one);
        set(&store, "k1", &entry(&[1; 100]));
        set(&store, "k2", &entry(&[1; 100]));
        let old = SystemTime::now() - Duration::from_secs(600);
        touch(&scratch.0.join(file_name("k1")), old);
        touch(
            &scratch.0.join(file_name("k2")),
            old + Duration::from_secs(1),
        );
        assert!(get(&store, "k1").is_some(), "k1 used, so now newest");
        set(&store, "k3", &entry(&[1; 100]));
        assert_eq!(get(&store, "k2"), None, "k2 was least recently used");
        assert!(get(&store, "k1").is_some());
    }

    #[test]
    fn equal_modification_times_evict_in_file_name_order() {
        let scratch = Scratch::new();
        let one = encode("k1", &entry(&[1; 100])).len() as u64;
        let store = open(&scratch.0, 3 * one);
        let keys = ["k1", "k2", "k3"];
        for key in keys {
            set(&store, key, &entry(&[1; 100]));
        }
        let same = SystemTime::now() - Duration::from_secs(600);
        for key in keys {
            touch(&scratch.0.join(file_name(key)), same);
        }
        let first = keys
            .iter()
            .min_by_key(|key| file_name(key))
            .copied()
            .unwrap_or_default();
        set(&store, "k4", &entry(&[1; 100]));
        assert_eq!(get(&store, first), None, "the first name went");
        assert_eq!(
            keys.iter().filter(|key| get(&store, key).is_some()).count(),
            2
        );
    }

    #[test]
    fn eviction_skips_a_file_another_process_removed() {
        let scratch = Scratch::new();
        let one = encode("k1", &entry(&[1; 100])).len() as u64;
        let store = open(&scratch.0, 2 * one);
        set(&store, "k1", &entry(&[1; 100]));
        set(&store, "k2", &entry(&[1; 100]));
        fs::remove_file(scratch.0.join(file_name("k1")))
            .unwrap_or_else(|error| unreachable!("{error}"));
        set(&store, "k3", &entry(&[1; 100]));
        assert!(get(&store, "k2").is_some(), "the scan found room");
        assert!(get(&store, "k3").is_some());
        assert_eq!(store.stats().evicted, 0);
    }

    #[test]
    fn an_entry_larger_than_max_bytes_is_not_stored() {
        let scratch = Scratch::new();
        let store = open(&scratch.0, 400);
        set(&store, "small", &entry(&[1; 10]));
        set(&store, "big", &entry(&[1; 400]));
        assert_eq!(get(&store, "big"), None);
        assert!(get(&store, "small").is_some(), "nothing evicted for it");
        let limited =
            DiskStore::open_with_max_entry_bytes(&scratch.0, bytes(400), 5)
                .unwrap_or_else(|error| unreachable!("{error}"));
        set(&limited, "six", &entry(&[1; 6]));
        assert_eq!(get(&limited, "six"), None, "over max_entry_bytes");
        assert!(
            DiskStore::open_with_max_entry_bytes(&scratch.0, bytes(400), 401)
                .is_err(),
            "max_entry_bytes above max_bytes is refused"
        );
    }

    #[test]
    fn an_entry_file_deleted_outside_is_a_plain_miss() {
        let scratch = Scratch::new();
        let store = open(&scratch.0, 1 << 20);
        set(&store, "k", &entry(b"abc"));
        assert!(store.stats().bytes > 0);
        fs::remove_file(scratch.0.join(file_name("k")))
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert!(matches!(store.get("k"), Ok(None)), "a miss, not an error");
        assert_eq!((store.stats().entries, store.stats().bytes), (0, 0));
        set(&store, "k", &entry(b"abc"));
        assert!(get(&store, "k").is_some(), "stored again");
    }

    #[test]
    fn the_whole_dir_deleted_is_a_miss_and_the_next_write_recreates_it() {
        let scratch = Scratch::new();
        let store = open(&scratch.0, 1 << 20);
        set(&store, "k", &entry(b"abc"));
        fs::remove_dir_all(&scratch.0)
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert!(matches!(store.get("k"), Ok(None)));
        assert!(store.clear().is_ok());
        set(&store, "k", &entry(b"abc"));
        assert!(get(&store, "k").is_some());
    }

    #[test]
    fn a_truncated_file_is_corrupt() {
        let scratch = Scratch::new();
        let store = open(&scratch.0, 1 << 20);
        set(&store, "k", &entry(b"abcdef"));
        let path = scratch.0.join(file_name("k"));
        let whole = fs::read(&path).unwrap_or_default();
        fs::write(&path, &whole[..whole.len() - 1])
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert!(corrupt(store.get("k")));
    }

    #[test]
    fn a_file_of_garbage_is_corrupt() {
        let scratch = Scratch::new();
        let store = open(&scratch.0, 1 << 20);
        set(&store, "k", &entry(b"abcdef"));
        fs::write(scratch.0.join(file_name("k")), b"<html>not an entry</html>")
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert!(corrupt(store.get("k")));
    }

    #[test]
    fn one_flipped_byte_is_corrupt() {
        let scratch = Scratch::new();
        let store = open(&scratch.0, 1 << 20);
        set(&store, "k", &entry(b"abcdef"));
        let path = scratch.0.join(file_name("k"));
        let mut whole = fs::read(&path).unwrap_or_default();
        let last = whole.len() - 1;
        whole[last] ^= 1;
        fs::write(&path, &whole)
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert!(corrupt(store.get("k")), "caught by the checksum");
    }

    #[test]
    fn another_version_is_corrupt() {
        let mut file = encode("k", &entry(b"abc"));
        file[8] = 2;
        assert!(corrupt(decode(&file, "k")));
    }

    #[test]
    fn a_file_for_another_key_is_a_miss_and_is_kept() {
        let scratch = Scratch::new();
        let store = open(&scratch.0, 1 << 20);
        set(&store, "a", &entry(b"abc"));
        fs::copy(
            scratch.0.join(file_name("a")),
            scratch.0.join(file_name("b")),
        )
        .unwrap_or_else(|error| unreachable!("{error}"));
        assert!(matches!(store.get("b"), Ok(None)));
        assert!(scratch.0.join(file_name("b")).exists(), "not deleted");
    }

    #[test]
    fn a_file_deleted_between_open_and_read_is_read_whole() {
        let scratch = Scratch::new();
        let store = open(&scratch.0, 1 << 20);
        set(&store, "k", &entry(b"abcdef"));
        *super::lock(&store.after_open) = Some(Box::new(|path: &Path| {
            let _ = fs::remove_file(path);
        }));
        let read = store.get("k");
        *super::lock(&store.after_open) = None;
        assert!(
            matches!(&read, Ok(Some(found)) if *found.bytes == *b"abcdef"),
            "{read:?}"
        );
        assert!(!scratch.0.join(file_name("k")).exists(), "the delete ran");
    }

    #[test]
    fn two_writers_on_one_key_leave_one_whole_file() {
        let scratch = Scratch::new();
        let first = Arc::new(open(&scratch.0, 1 << 24));
        let second = Arc::new(open(&scratch.0, 1 << 24));
        let bodies = [vec![1_u8; 64 * 1024], vec![2_u8; 96 * 1024]];
        let writers: Vec<_> = [first.clone(), second]
            .into_iter()
            .zip(bodies.clone())
            .map(|(store, body)| {
                std::thread::spawn(move || {
                    for _ in 0..50 {
                        let _ = store.set("k", &entry(&body));
                    }
                })
            })
            .collect();
        let mut seen = 0;
        while seen < 200 {
            if let Some(found) = get(&first, "k") {
                assert!(
                    bodies.iter().any(|body| *found.bytes == **body),
                    "a mix of two writes"
                );
            }
            seen += 1;
        }
        for writer in writers {
            assert!(writer.join().is_ok());
        }
        assert!(get(&first, "k").is_some());
        assert!(names(&scratch.0).iter().all(|name| !is_temp(name)));
    }

    #[test]
    fn a_refused_rename_stores_nothing_and_reports_nothing() {
        let scratch = Scratch::new();
        let store = open(&scratch.0, 1 << 20);
        // A directory with something in it where the entry would go: no
        // rename replaces it, on any platform.
        let target = scratch.0.join(file_name("k"));
        fs::create_dir_all(target.join("held"))
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert!(store.set("k", &entry(b"abc")).is_ok(), "no error");
        assert!(names(&scratch.0).iter().all(|name| !is_temp(name)));
        assert_eq!(store.stats().stored, 0);
    }

    #[test]
    fn a_full_disk_removes_the_temp_file_and_is_an_error() {
        let scratch = Scratch::new();
        let store = open(&scratch.0, 1 << 20);
        store.fail_writes.store(true, Ordering::SeqCst);
        let refused = store.set("k", &entry(b"abc"));
        assert!(
            refused.as_ref().is_err_and(|error| !error.is_corrupt()),
            "{refused:?}"
        );
        assert!(names(&scratch.0).is_empty(), "{:?}", names(&scratch.0));
        store.fail_writes.store(false, Ordering::SeqCst);
        set(&store, "k", &entry(b"abc"));
        assert!(get(&store, "k").is_some());
    }

    #[test]
    fn open_removes_temp_files_older_than_an_hour_and_keeps_younger_ones() {
        let scratch = Scratch::new();
        fs::create_dir_all(&scratch.0)
            .unwrap_or_else(|error| unreachable!("{error}"));
        let hash = &file_name("k")[..64];
        let (old, young) = (
            format!("tmp-{hash}-00000000000000aa"),
            format!("tmp-{hash}-00000000000000bb"),
        );
        for name in [&old, &young] {
            fs::write(scratch.0.join(name), b"partial")
                .unwrap_or_else(|error| unreachable!("{error}"));
        }
        let now = SystemTime::now();
        touch(
            &scratch.0.join(&old),
            now - TEMP_FILE_MAX_AGE - Duration::from_secs(60),
        );
        touch(
            &scratch.0.join(&young),
            now - TEMP_FILE_MAX_AGE + Duration::from_secs(60),
        );
        let store = open(&scratch.0, 1 << 20);
        assert_eq!(names(&scratch.0), [young]);
        assert!(
            matches!(store.get("k"), Ok(None)),
            "a temp file is never read"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_read_only_dir_serves_lookups_and_refuses_writes() {
        use std::os::unix::fs::PermissionsExt as _;

        let scratch = Scratch::new();
        let store = open(&scratch.0, 1 << 20);
        set(&store, "k", &entry(b"abc"));
        fs::set_permissions(&scratch.0, fs::Permissions::from_mode(0o555))
            .unwrap_or_else(|error| unreachable!("{error}"));
        // Root ignores the mode; nothing here could be refused then.
        if fs::write(scratch.0.join("probe"), b"").is_ok() {
            return;
        }
        assert!(get(&store, "k").is_some(), "lookups work");
        assert!(store.set("j", &entry(b"abc")).is_err(), "writes report");
        assert!(store.delete("k").is_err(), "so do deletes");
    }

    #[cfg(unix)]
    #[test]
    fn a_dir_that_cannot_be_created_again_is_an_error() {
        use std::os::unix::fs::PermissionsExt as _;

        let scratch = Scratch::new();
        let dir = scratch.0.join("cache");
        let store = open(&dir, 1 << 20);
        fs::remove_dir_all(&dir)
            .unwrap_or_else(|error| unreachable!("{error}"));
        fs::set_permissions(&scratch.0, fs::Permissions::from_mode(0o555))
            .unwrap_or_else(|error| unreachable!("{error}"));
        if fs::create_dir(scratch.0.join("probe")).is_ok() {
            return;
        }
        assert!(store.set("k", &entry(b"abc")).is_err());
        assert!(matches!(store.get("k"), Ok(None)));
    }

    #[test]
    fn names_are_recognised_exactly() {
        let hash = &file_name("k")[..64];
        assert!(is_entry(&format!("{hash}.entry")));
        assert!(!is_entry(&format!("{}.entry", &hash[1..])));
        assert!(!is_entry(&format!("{}.entry", hash.to_uppercase())));
        assert!(is_temp(&format!("tmp-{hash}-0123456789abcdef")));
        assert!(!is_temp(&format!("tmp-{hash}-0123")));
        assert!(!is_temp(&format!("{hash}.entry")));
    }
}
