//! What a URL does on each side of the `net` flag: one test per build, only one
//! compiling at a time, since the flag is the subject.
use meo_canvas::{Box, OnImageError, Renderer, Root, Styled, px};

/// A page whose one node names a URL nothing will answer.
fn page() -> Root {
    // `Throw`, since this file is about the flag rather than the policy: under
    // the default `Placeholder` a dead URL is a warning, and whether a fetch
    // was attempted shows only in an error.
    Root::new(64.0)
        .on_image_error(OnImageError::Throw)
        .height(64.0)
        .children([
            meo_canvas::Image::url("http://127.0.0.1:1/never.png")
                .size(px(16.0), px(16.0)),
            Box::new(),
        ])
}

#[cfg(not(feature = "net"))]
#[test]
fn without_the_feature_the_error_names_the_feature() {
    let refused = page()
        .render(&Renderer::new())
        .err()
        .unwrap_or_else(|| unreachable!("a URL cannot be resolved here"));
    let message = refused.to_string();

    // **The flag is the fix, so the error has to say the flag.** A caller
    // meeting this has already demonstrated they did not read the feature
    // list; the message is the one thing they are certainly looking at.
    assert!(
        message.contains("net"),
        "the refusal does not name the feature that answers it: {message}"
    );
    assert!(
        message.contains("ImageSource::Bytes"),
        "the refusal does not name the other way out: {message}"
    );
}

#[cfg(feature = "net")]
#[test]
fn with_the_feature_a_url_is_attempted_rather_than_refused() {
    // Port 1 refuses a connection immediately, so this is a fetch that failed
    // rather than a fetch that never happened -- which is the whole difference
    // the flag makes. The message is the client's, not a refusal to try.
    let refused = page()
        .render(&Renderer::new())
        .err()
        .unwrap_or_else(|| unreachable!("nothing is listening on port 1"));
    let message = refused.to_string();

    assert!(
        !message.contains("cannot fetch: enable"),
        "the build with `net` on still refused to try: {message}"
    );
    assert!(
        message.contains("127.0.0.1:1"),
        "the failure does not name the URL it tried: {message}"
    );
}

/// `Root::image_cache` reaches the core's cache: two renders, one request.
#[cfg(feature = "net")]
#[test]
fn root_image_cache_reuses_the_first_fetch() {
    use std::{
        io::{BufRead as _, BufReader, Write as _},
        net::TcpListener,
        num::NonZeroU64,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };

    use meo_canvas::image_cache::{ImageCache, MemoryStore};

    /// A 1x1 PNG.
    const DOT: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D,
        0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,
        0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00,
        0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0xF8, 0xCF, 0xC0, 0x00,
        0x00, 0x03, 0x01, 0x01, 0x00, 0x18, 0xDD, 0x8D, 0xB0, 0x00, 0x00, 0x00,
        0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    let listener = TcpListener::bind("127.0.0.1:0")
        .unwrap_or_else(|error| unreachable!("{error}"));
    let port = listener
        .local_addr()
        .unwrap_or_else(|error| unreachable!("{error}"))
        .port();
    let served = Arc::new(AtomicUsize::new(0));
    let counting = Arc::clone(&served);
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let Ok(clone) = stream.try_clone() else {
                continue;
            };
            let mut reader = BufReader::new(clone);
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap_or(0) > 2 {
                line.clear();
            }
            counting.fetch_add(1, Ordering::SeqCst);
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\
                 Cache-Control: max-age=60\r\nConnection: close\r\n\r\n",
                DOT.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(DOT);
        }
    });

    let max = NonZeroU64::new(1 << 20).unwrap_or(NonZeroU64::MIN);
    let cache = Arc::new(ImageCache::new(MemoryStore::new(max)));
    let url = format!("http://127.0.0.1:{port}/a.png");
    for _ in 0..2 {
        let canvas = Root::new(16.0)
            .height(16.0)
            .on_image_error(OnImageError::Throw)
            .image_cache(Arc::clone(&cache))
            .children(
                meo_canvas::Image::url(url.as_str()).size(px(8.0), px(8.0)),
            )
            .render(&Renderer::new())
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert!(canvas.warnings().is_empty());
    }
    assert_eq!(served.load(Ordering::SeqCst), 1);
    assert_eq!(cache.stats().hits, 1);
}
