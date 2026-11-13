//! The image cache through a render, against a local origin that counts the
//! requests it answers: what is stored, what is reused, what revalidates, and
//! what a failing store or origin does to a render.
#![cfg(feature = "net")]

use std::{
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    num::NonZeroU64,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::{Duration, SystemTime},
};

use meo_canvas_core::{
    RenderedCanvas, Renderer,
    image_cache::{
        DiskStore, ImageCache, ImageCacheEntry, ImageStore, MemoryStore,
        StoreError,
    },
};
use meo_canvas_scene::{
    Length, Scene, Size,
    node::{HttpOptions, ImageSource, Node, NodeKind},
    style::paint::ObjectFit,
};

/// A 1x1 PNG, so a response decodes and the render reaches its end.
const DOT: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D,
    0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,
    0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00,
    0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0xF8, 0xCF, 0xC0, 0x00,
    0x00, 0x03, 0x01, 0x01, 0x00, 0x18, 0xDD, 0x8D, 0xB0, 0x00, 0x00, 0x00,
    0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

/// What the origin answers the `n`th request (from 0) with, given its head.
type Reply = Box<dyn FnMut(usize, &str) -> Response + Send>;

/// One response: a status line and headers, a body, and how long to wait
/// before sending it. `None` closes the connection without answering.
struct Response {
    head: Option<String>,
    body: Vec<u8>,
    delay: Duration,
}

fn answer(status: &str, headers: &[&str], body: &[u8]) -> Response {
    let mut head = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for header in headers {
        head.push_str(header);
        head.push_str("\r\n");
    }
    head.push_str("\r\n");
    Response {
        head: Some(head),
        body: body.to_vec(),
        delay: Duration::ZERO,
    }
}

fn ok(headers: &[&str]) -> Response {
    answer("200 OK", headers, DOT)
}

const fn hang_up() -> Response {
    Response {
        head: None,
        body: Vec::new(),
        delay: Duration::ZERO,
    }
}

/// A local origin that counts and records every request.
struct Origin {
    base: String,
    count: Arc<AtomicUsize>,
    heads: Arc<Mutex<Vec<String>>>,
}

impl Origin {
    fn new(
        reply: impl FnMut(usize, &str) -> Response + Send + 'static,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .unwrap_or_else(|error| unreachable!("{error}"));
        let port = listener
            .local_addr()
            .unwrap_or_else(|error| unreachable!("{error}"))
            .port();
        let count = Arc::new(AtomicUsize::new(0));
        let heads = Arc::new(Mutex::new(Vec::new()));
        let reply: Arc<Mutex<Reply>> = Arc::new(Mutex::new(Box::new(reply)));
        let (counting, recording) = (Arc::clone(&count), Arc::clone(&heads));
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let (count, heads, reply) = (
                    Arc::clone(&counting),
                    Arc::clone(&recording),
                    Arc::clone(&reply),
                );
                std::thread::spawn(move || {
                    serve(stream, &count, &heads, &reply);
                });
            }
        });
        Self {
            base: format!("http://127.0.0.1:{port}"),
            count,
            heads,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    fn requests(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }

    fn head(&self, n: usize) -> String {
        self.heads
            .lock()
            .map(|heads| heads.get(n).cloned().unwrap_or_default())
            .unwrap_or_default()
    }
}

fn serve(
    mut stream: TcpStream,
    count: &AtomicUsize,
    heads: &Mutex<Vec<String>>,
    reply: &Mutex<Reply>,
) {
    let Ok(clone) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(clone);
    let mut head = String::new();
    let mut line = String::new();
    while reader.read_line(&mut line).unwrap_or(0) > 0 {
        if line == "\r\n" || line == "\n" {
            break;
        }
        head.push_str(&line);
        line.clear();
    }
    if head.is_empty() {
        return;
    }
    let n = count.fetch_add(1, Ordering::SeqCst);
    if let Ok(mut heads) = heads.lock() {
        heads.push(head.to_ascii_lowercase());
    }
    let response = match reply.lock() {
        Ok(mut reply) => reply(n, &head),
        Err(_) => return,
    };
    std::thread::sleep(response.delay);
    if let Some(head) = response.head {
        let _ = stream.write_all(head.as_bytes());
        let _ = stream.write_all(&response.body);
        let _ = stream.flush();
    }
}

/// A scene whose one node draws `url`, sent with `http`.
fn scene(url: &str, http: HttpOptions) -> Scene {
    let mut scene = Scene::new(Size::new(8.0, 8.0));
    let root = scene
        .root()
        .unwrap_or_else(|| unreachable!("a new scene has a root"));
    let node = Node::new(NodeKind::Image {
        source: ImageSource::url_with(url, http),
        frame: None,
        fit: ObjectFit::Fill,
        position: (Length::ZERO, Length::ZERO),
    });
    scene
        .push(root, node)
        .unwrap_or_else(|error| unreachable!("{error}"));
    scene
}

fn render(url: &str, cache: &ImageCache) -> RenderedCanvas {
    Renderer::new()
        .render_with_cache(&scene(url, HttpOptions::new()), cache)
        .unwrap_or_else(|error| unreachable!("the render failed: {error}"))
}

fn bytes(n: u64) -> NonZeroU64 {
    NonZeroU64::new(n).unwrap_or(NonZeroU64::MIN)
}

fn memory() -> ImageCache {
    ImageCache::new(MemoryStore::new(bytes(1 << 24)))
}

fn details(canvas: &RenderedCanvas) -> Vec<String> {
    canvas
        .diagnostics()
        .iter()
        .map(|found| format!("{} {}", found.path, found.detail))
        .collect()
}

#[test]
fn a_fresh_hit_makes_no_request() {
    let origin = Origin::new(|_, _| ok(&["Cache-Control: max-age=60"]));
    let cache = memory();
    let url = origin.url("/a.png");
    for _ in 0..3 {
        let canvas = render(&url, &cache);
        assert!(canvas.warnings().is_empty(), "{:?}", canvas.warnings());
        assert!(canvas.diagnostics().is_empty());
    }
    assert_eq!(origin.requests(), 1);
    let stats = cache.stats();
    assert_eq!(
        (stats.misses, stats.hits, stats.stored, stats.entries),
        (1, 2, 1, 1)
    );
}

#[test]
fn without_a_cache_every_render_fetches() {
    let origin = Origin::new(|_, _| ok(&["Cache-Control: max-age=60"]));
    let url = origin.url("/a.png");
    for _ in 0..2 {
        let _ = Renderer::new()
            .render(&scene(&url, HttpOptions::new()))
            .unwrap_or_else(|error| unreachable!("{error}"));
    }
    assert_eq!(origin.requests(), 2);
}

#[test]
fn an_expired_entry_with_an_etag_revalidates_and_a_304_keeps_it() {
    let origin = Origin::new(|n, _| match n {
        0 => ok(&["Cache-Control: max-age=0", "ETag: \"v1\""]),
        _ => answer("304 Not Modified", &["Cache-Control: max-age=60"], &[]),
    });
    let cache = memory();
    let url = origin.url("/a.png");
    let _ = render(&url, &cache);
    let canvas = render(&url, &cache);
    assert!(canvas.warnings().is_empty(), "the stored bytes were drawn");
    assert!(
        origin.head(1).contains("if-none-match: \"v1\""),
        "{}",
        origin.head(1)
    );
    // The 304's max-age made it fresh again.
    let _ = render(&url, &cache);
    assert_eq!(origin.requests(), 2);
    let stats = cache.stats();
    assert_eq!((stats.misses, stats.revalidated, stats.hits), (1, 1, 1));
}

#[test]
fn last_modified_revalidates_with_if_modified_since() {
    let origin = Origin::new(|n, _| match n {
        0 => ok(&[
            "Cache-Control: no-cache",
            "Last-Modified: Thu, 22 Oct 2026 00:00:00 GMT",
        ]),
        _ => answer("304 Not Modified", &[], &[]),
    });
    let cache = memory();
    let url = origin.url("/a.png");
    let _ = render(&url, &cache);
    let _ = render(&url, &cache);
    assert!(
        origin
            .head(1)
            .contains("if-modified-since: thu, 22 oct 2026 00:00:00 gmt"),
        "{}",
        origin.head(1)
    );
    assert_eq!(cache.stats().revalidated, 1);
}

#[test]
fn a_200_to_a_revalidation_replaces_the_entry() {
    let origin = Origin::new(|n, _| match n {
        0 => ok(&["Cache-Control: max-age=0", "ETag: \"v1\""]),
        _ => ok(&["Cache-Control: max-age=60", "ETag: \"v2\""]),
    });
    let cache = memory();
    let url = origin.url("/a.png");
    let _ = render(&url, &cache);
    let _ = render(&url, &cache);
    let _ = render(&url, &cache);
    assert_eq!(origin.requests(), 2);
    let stats = cache.stats();
    assert_eq!(
        (stats.misses, stats.replaced, stats.hits, stats.stored),
        (1, 1, 1, 2)
    );
}

#[test]
fn an_expired_entry_with_no_validator_is_fetched_again_plainly() {
    let origin = Origin::new(|_, _| ok(&["Cache-Control: max-age=0"]));
    let cache = memory();
    let url = origin.url("/a.png");
    let _ = render(&url, &cache);
    let _ = render(&url, &cache);
    assert_eq!(origin.requests(), 2);
    assert!(!origin.head(1).contains("if-"), "{}", origin.head(1));
    assert_eq!(cache.stats().misses, 2);
}

#[test]
fn eviction_by_bytes_fetches_the_evicted_image_again() {
    let origin = Origin::new(|_, _| ok(&["Cache-Control: max-age=60"]));
    let url = |n: usize| origin.url(&format!("/{n}.png"));
    // Room for two entries and not three.
    let one = (DOT.len() + url(0).len()) as u64
        + meo_canvas_core::image_cache::ENTRY_OVERHEAD_BYTES;
    let cache = ImageCache::new(MemoryStore::new(bytes(2 * one + 1)));
    for n in 0..3 {
        let _ = render(&url(n), &cache);
    }
    assert_eq!(cache.stats().evicted, 1);
    let _ = render(&url(0), &cache);
    assert_eq!(origin.requests(), 4, "the first was evicted");
    let _ = render(&url(2), &cache);
    assert_eq!(origin.requests(), 4, "the last was kept");
}

#[test]
fn an_entry_over_max_entry_bytes_is_not_stored_and_still_drawn() {
    let origin = Origin::new(|_, _| ok(&["Cache-Control: max-age=60"]));
    let store = MemoryStore::with_max_entry_bytes(bytes(1 << 20), 10)
        .unwrap_or_else(|error| unreachable!("{error}"));
    let cache = ImageCache::new(store);
    let url = origin.url("/a.png");
    for _ in 0..2 {
        assert!(render(&url, &cache).warnings().is_empty());
    }
    assert_eq!(origin.requests(), 2);
    assert_eq!(cache.stats().entries, 0);
}

#[test]
fn failures_are_not_stored() {
    for (path, first) in [
        (
            "/404.png",
            answer("404 Not Found", &["Cache-Control: max-age=60"], b"no"),
        ),
        (
            "/503.png",
            answer(
                "503 Service Unavailable",
                &["Cache-Control: max-age=60"],
                b"no",
            ),
        ),
        ("/closed.png", hang_up()),
    ] {
        let mut first = Some(first);
        let origin = Origin::new(move |_, _| {
            first
                .take()
                .unwrap_or_else(|| ok(&["Cache-Control: max-age=60"]))
        });
        let cache = memory();
        let url = origin.url(path);
        assert_eq!(render(&url, &cache).warnings().len(), 1, "{path}");
        assert_eq!(cache.stats().entries, 0, "{path}");
        assert!(render(&url, &cache).warnings().is_empty(), "{path}");
        assert_eq!(origin.requests(), 2, "{path}");
    }
}

#[test]
fn a_body_past_the_fetch_limit_is_not_stored() {
    let big = vec![0_u8; 33 * 1024 * 1024];
    let origin = Origin::new(move |_, _| {
        answer("200 OK", &["Cache-Control: max-age=60"], &big)
    });
    let cache = memory();
    let canvas = render(&origin.url("/big.png"), &cache);
    assert_eq!(canvas.warnings().len(), 1);
    assert_eq!(cache.stats().entries, 0);
}

#[test]
fn no_store_is_not_stored() {
    let origin =
        Origin::new(|_, _| ok(&["Cache-Control: no-store, max-age=60"]));
    let cache = memory();
    let url = origin.url("/a.png");
    let _ = render(&url, &cache);
    let _ = render(&url, &cache);
    assert_eq!(origin.requests(), 2);
    assert_eq!(cache.stats().entries, 0);
}

#[test]
fn a_decodable_failure_is_cached_as_the_response_it_was() {
    // A host serving an error page with a 200: the cache holds responses.
    let origin = Origin::new(|_, _| {
        answer(
            "200 OK",
            &["Cache-Control: max-age=60"],
            b"<html>oops</html>",
        )
    });
    let cache = memory();
    let url = origin.url("/a.png");
    assert_eq!(render(&url, &cache).warnings().len(), 1);
    assert_eq!(render(&url, &cache).warnings().len(), 1);
    assert_eq!(origin.requests(), 1);
}

#[test]
fn different_headers_are_different_entries() {
    let origin = Origin::new(|_, _| ok(&["Cache-Control: max-age=60"]));
    let cache = memory();
    let url = origin.url("/a.png");
    let signed = HttpOptions::new().header("Authorization", "Bearer t");
    for _ in 0..2 {
        for http in [HttpOptions::new(), signed.clone()] {
            let _ = Renderer::new()
                .render_with_cache(&scene(&url, http), &cache)
                .unwrap_or_else(|error| unreachable!("{error}"));
        }
    }
    assert_eq!(origin.requests(), 2, "one per key, then hits");
    assert_eq!(cache.stats().entries, 2);
}

#[test]
fn stale_if_error_serves_the_expired_copy_inside_the_bound() {
    for failure in [answer("503 Service Unavailable", &[], b""), hang_up()] {
        let mut failure = Some(failure);
        let origin = Origin::new(move |n, _| match n {
            0 => ok(&[
                "Cache-Control: max-age=0, stale-if-error=600",
                "ETag: \"v1\"",
            ]),
            _ => failure.take().unwrap_or_else(hang_up),
        });
        let cache = memory();
        let url = origin.url("/a.png");
        let _ = render(&url, &cache);
        let canvas = render(&url, &cache);
        assert!(canvas.warnings().is_empty(), "the stale bytes were drawn");
        let found = details(&canvas);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].starts_with(&format!(
                "{url} revalidating the cached image failed ("
            )),
            "{found:?}"
        );
        assert!(found[0].ends_with(
            "so the expired copy is drawn, as stale-if-error allows"
        ));
        assert_eq!(cache.stats().stale_served, 1);
    }
}

#[test]
fn stale_if_error_past_the_bound_fails_as_without_a_cache() {
    let origin = Origin::new(|n, _| match n {
        0 => {
            ok(&["Cache-Control: max-age=0, stale-if-error=0", "ETag: \"v1\""])
        }
        _ => answer("503 Service Unavailable", &[], b""),
    });
    let cache = memory();
    let url = origin.url("/a.png");
    let _ = render(&url, &cache);
    let canvas = render(&url, &cache);
    assert_eq!(canvas.warnings().len(), 1);
    assert_eq!(cache.stats().stale_served, 0);
}

#[test]
fn a_cache_with_stale_if_error_zero_never_serves_stale() {
    let origin = Origin::new(|n, _| match n {
        0 => ok(&[
            "Cache-Control: max-age=0, stale-if-error=600",
            "ETag: \"v1\"",
        ]),
        _ => answer("503 Service Unavailable", &[], b""),
    });
    let cache = memory().stale_if_error(Duration::ZERO);
    let url = origin.url("/a.png");
    let _ = render(&url, &cache);
    assert_eq!(render(&url, &cache).warnings().len(), 1);
}

#[test]
fn a_4xx_on_revalidation_is_not_a_reason_to_serve_stale() {
    let origin = Origin::new(|n, _| match n {
        0 => ok(&["Cache-Control: max-age=0", "ETag: \"v1\""]),
        _ => answer("404 Not Found", &[], b""),
    });
    let cache = memory();
    let url = origin.url("/a.png");
    let _ = render(&url, &cache);
    assert_eq!(render(&url, &cache).warnings().len(), 1);
}

#[test]
fn an_entry_that_can_never_be_used_again_is_deleted_on_lookup() {
    // No validator, and no stale-if-error for this cache: once expired it is
    // useless, and the failing refetch must not leave it behind.
    let origin = Origin::new(|n, _| match n {
        0 => ok(&["Cache-Control: max-age=0"]),
        _ => answer("404 Not Found", &[], b""),
    });
    let cache = memory().stale_if_error(Duration::ZERO);
    let url = origin.url("/a.png");
    let _ = render(&url, &cache);
    assert_eq!(cache.stats().entries, 1);
    let _ = render(&url, &cache);
    assert_eq!(cache.stats().entries, 0);
}

#[test]
fn concurrent_renders_of_one_key_make_one_request() {
    let origin = Origin::new(|_, _| Response {
        delay: Duration::from_millis(300),
        ..ok(&["Cache-Control: max-age=60"])
    });
    let cache = Arc::new(memory());
    let url = origin.url("/a.png");
    let renders: Vec<_> = (0..8)
        .map(|_| {
            let (cache, url) = (Arc::clone(&cache), url.clone());
            std::thread::spawn(move || render(&url, &cache).warnings().len())
        })
        .collect();
    for handle in renders {
        assert_eq!(handle.join().ok(), Some(0));
    }
    assert_eq!(origin.requests(), 1);
    let stats = cache.stats();
    assert_eq!(stats.misses + stats.hits, 8);
}

/// A directory of its own, removed when the test ends.
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "meo-canvas-image-cache-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        Self(dir)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_disk_store_persists_across_two_instances() {
    let origin = Origin::new(|_, _| ok(&["Cache-Control: max-age=60"]));
    let scratch = Scratch::new();
    let url = origin.url("/a.png");
    let open = || {
        ImageCache::new(
            DiskStore::open(&scratch.0, bytes(1 << 24))
                .unwrap_or_else(|error| unreachable!("{error}")),
        )
    };
    let _ = render(&url, &open());
    let again = open();
    assert!(render(&url, &again).warnings().is_empty());
    assert_eq!(origin.requests(), 1, "the second process found the file");
    assert_eq!(again.stats().hits, 1);
}

/// Starts a render on another thread and waits until its request reaches
/// the origin, which holds it for `HOLD`.
fn in_flight(
    cache: &Arc<ImageCache>,
    origin: &Origin,
    url: &str,
) -> std::thread::JoinHandle<usize> {
    let (cache, url) = (Arc::clone(cache), url.to_owned());
    let render =
        std::thread::spawn(move || render(&url, &cache).warnings().len());
    while origin.requests() == 0 {
        std::thread::sleep(Duration::from_millis(5));
    }
    render
}

/// How long the origin holds a request a delete or clear has to overtake.
const HOLD: Duration = Duration::from_millis(400);

#[test]
fn clear_during_a_fetch_keeps_that_fetch_out_of_the_store() {
    let origin = Origin::new(|_, _| Response {
        delay: HOLD,
        ..ok(&["Cache-Control: max-age=60"])
    });
    let cache = Arc::new(memory());
    let url = origin.url("/a.png");
    let render = in_flight(&cache, &origin, &url);
    assert!(cache.clear().is_ok());
    assert_eq!(render.join().ok(), Some(0), "the render still drew it");
    assert_eq!(cache.stats().entries, 0, "and did not store it");
}

#[test]
fn delete_during_a_fetch_keeps_that_fetch_out_of_the_store() {
    let origin = Origin::new(|_, _| Response {
        delay: HOLD,
        ..ok(&["Cache-Control: max-age=60"])
    });
    let cache = Arc::new(memory());
    let url = origin.url("/a.png");
    let render = in_flight(&cache, &origin, &url);
    assert!(cache.delete(&url, &HttpOptions::new()).is_ok());
    assert_eq!(render.join().ok(), Some(0));
    assert_eq!(cache.stats().entries, 0);
}

#[test]
fn a_clock_moved_back_revalidates_rather_than_serving_fresh() {
    let origin = Origin::new(|_, _| {
        answer("304 Not Modified", &["Cache-Control: max-age=60"], &[])
    });
    let store = Arc::new(MemoryStore::new(bytes(1 << 20)));
    let url = origin.url("/a.png");
    let future = SystemTime::now() + Duration::from_hours(1);
    let planted = ImageCacheEntry {
        bytes: Arc::from(DOT),
        etag: Some("\"v1\"".to_owned()),
        last_modified: None,
        cache_control: None,
        expires: None,
        pragma: None,
        date: None,
        age: None,
        stored_at: future,
        expires_at: future + Duration::from_hours(1),
        stale_if_error_until: future + Duration::from_hours(1),
        revalidate_always: false,
    };
    store
        .set(&ImageCache::key(&url, &HttpOptions::new()), &planted)
        .unwrap_or_else(|error| unreachable!("{error}"));
    let cache = ImageCache::new(Arc::clone(&store));
    assert!(render(&url, &cache).warnings().is_empty());
    assert_eq!(origin.requests(), 1, "expired, so revalidated");
    assert_eq!(cache.stats().revalidated, 1);
}

#[test]
fn two_caches_over_one_store_keep_their_own_counts() {
    let origin = Origin::new(|_, _| ok(&["Cache-Control: max-age=60"]));
    let store = Arc::new(MemoryStore::new(bytes(1 << 20)));
    let (first, second) = (
        ImageCache::new(Arc::clone(&store)),
        ImageCache::new(Arc::clone(&store))
            .fallback_ttl(Duration::from_secs(5)),
    );
    let url = origin.url("/a.png");
    let _ = render(&url, &first);
    let _ = render(&url, &second);
    assert_eq!(origin.requests(), 1);
    let (a, b) = (first.stats(), second.stats());
    assert_eq!((a.misses, a.hits, b.misses, b.hits), (1, 0, 0, 1));
    assert_eq!(
        (a.entries, a.bytes),
        (b.entries, b.bytes),
        "the store's own"
    );
}

/// A store that fails as it is told to, and records what it was asked.
#[derive(Debug, Default)]
struct Failing {
    corrupt: bool,
    deleted: Mutex<Vec<String>>,
}

impl ImageStore for Failing {
    fn get(&self, _: &str) -> Result<Option<ImageCacheEntry>, StoreError> {
        Err(if self.corrupt {
            StoreError::corrupt("bad magic")
        } else {
            StoreError::new("permission denied")
        })
    }

    fn set(&self, _: &str, _: &ImageCacheEntry) -> Result<(), StoreError> {
        Err(StoreError::new("no space left on device"))
    }

    fn delete(&self, key: &str) -> Result<(), StoreError> {
        if let Ok(mut deleted) = self.deleted.lock() {
            deleted.push(key.to_owned());
        }
        Ok(())
    }

    fn clear(&self) -> Result<(), StoreError> {
        Ok(())
    }
}

#[test]
fn a_failing_store_is_a_miss_with_a_diagnostic_and_never_fails_the_render() {
    let origin = Origin::new(|_, _| ok(&["Cache-Control: max-age=60"]));
    let cache = ImageCache::new(Failing::default());
    let url = origin.url("/a.png");
    let canvas = render(&url, &cache);
    assert!(canvas.warnings().is_empty(), "the image was drawn");
    assert_eq!(
        details(&canvas),
        [
            format!(
                "{url} the image cache could not read its entry (permission denied), so the image is fetched"
            ),
            format!(
                "{url} the image cache could not store the image (no space left on device); the render draws the fetched bytes"
            ),
        ]
    );
}

#[test]
fn a_corrupt_entry_is_deleted_and_reported() {
    let origin = Origin::new(|_, _| ok(&["Cache-Control: max-age=60"]));
    let store = Arc::new(Failing {
        corrupt: true,
        ..Failing::default()
    });
    let cache = ImageCache::new(Arc::clone(&store));
    let url = origin.url("/a.png");
    let canvas = render(&url, &cache);
    assert!(details(&canvas)[0].ends_with(
        "the image cache's entry is corrupt (bad magic), so it is deleted and the image fetched"
    ));
    let deleted = store.deleted.lock().map(|d| d.clone()).unwrap_or_default();
    assert_eq!(deleted, [ImageCache::key(&url, &HttpOptions::new())]);
}

#[test]
fn a_corrupt_disk_file_is_a_miss_and_is_replaced() {
    let origin = Origin::new(|_, _| ok(&["Cache-Control: max-age=60"]));
    let scratch = Scratch::new();
    let cache = ImageCache::new(
        DiskStore::open(&scratch.0, bytes(1 << 24))
            .unwrap_or_else(|error| unreachable!("{error}")),
    );
    let url = origin.url("/a.png");
    let _ = render(&url, &cache);
    for listed in std::fs::read_dir(&scratch.0)
        .into_iter()
        .flatten()
        .flatten()
    {
        let _ = std::fs::write(listed.path(), b"garbage");
    }
    let canvas = render(&url, &cache);
    assert!(canvas.warnings().is_empty());
    assert_eq!(details(&canvas).len(), 1, "{:?}", details(&canvas));
    assert_eq!(origin.requests(), 2);
    let _ = render(&url, &cache);
    assert_eq!(origin.requests(), 2, "stored whole again");
}

#[test]
fn the_cache_reports_after_the_renderer_s_own_diagnostics() {
    let origin = Origin::new(|_, _| ok(&["Cache-Control: max-age=60"]));
    let url = origin.url("/a.png");
    let mut scene = scene(&url, HttpOptions::new());
    if let Some(root) = scene.root().and_then(|root| scene.get_mut(root)) {
        root.effects.filter = Some("bogus(1)".to_owned());
    }
    let canvas = Renderer::new()
        .render_with_cache(&scene, &ImageCache::new(Failing::default()))
        .unwrap_or_else(|error| unreachable!("{error}"));
    let paths: Vec<&str> = canvas
        .diagnostics()
        .iter()
        .map(|found| found.path.as_str())
        .collect();
    assert_eq!(paths, ["filter", url.as_str(), url.as_str()]);
}
