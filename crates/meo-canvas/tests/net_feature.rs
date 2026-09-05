//! What a URL does on each side of the `net` flag.
//!
//! **Two tests, one per build, and only one of them compiles at a time.** The
//! flag is the subject, so a test that ran under both would be measuring
//! neither.
use meo_canvas::{Box, OnImageError, Renderer, Root, Styled, px};

/// A page whose one node names a URL nothing will answer.
fn page() -> Root {
    // **`Throw`, because this file is about the flag rather than the policy.**
    // The default is `on_image_error(Placeholder)`, under which a dead URL is a
    // warning and the render finishes -- so both arms below would have nothing
    // to read. What the flag decides is whether a fetch is *attempted*, and
    // that distinction is only visible in an error when the failure is one.
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
