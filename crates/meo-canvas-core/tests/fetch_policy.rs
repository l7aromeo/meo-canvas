//! The size limit and the timeouts, through the path a caller reaches.
//!
//! **Only the size case is here.** The global timeout was verified the same
//! way and is not committed: a host that accepts a connection and says nothing
//! returns `FetchFailure::Transport` with `"timeout: global"` after **60.1
//! seconds**, measured once. A minute of a gate to re-prove a constant is not
//! worth it, and a test that sleeps for a minute is one people learn to skip.
//!
//! That measurement is the one that matters, though, so it is written down
//! rather than assumed: the timeout spans `read_to_vec`, which happens after
//! `.call()` returns. A clock that stopped at the response header would have
//! left the hang exactly where it was and looked like a fix.
#![cfg(feature = "net")]

use std::{io::Write, net::TcpListener};

use meo_canvas_core::{Error, FetchFailure, Renderer};
use meo_canvas_scene::{
    Length, OnImageError, Scene, Size,
    node::{HttpOptions, ImageSource, Node, NodeKind},
    style::paint::ObjectFit,
};

/// A scene whose one node names a URL, which is the only way to reach `fetch`
/// from outside the crate.
fn scene_naming(url: String) -> Scene {
    scene_sending(url, HttpOptions::new())
}

/// The same, with options on the source.
fn scene_sending(url: String, http: HttpOptions) -> Scene {
    let mut scene = Scene::new(Size::new(64.0, 64.0));
    // **`Throw`, because this file is about the classification rather than the
    // policy.** The default is `Placeholder`, under which an oversized image
    // is softened into a warning and the render finishes -- which is the right
    // default and would make every assertion below unreachable. The warning
    // carries the same `failure` value this checks, copied from the same
    // error, so pinning it here pins it for both paths.
    scene.on_image_error = OnImageError::Throw;
    let root = scene
        .root()
        .unwrap_or_else(|| unreachable!("a new scene has a root"));
    let node = Node::new(NodeKind::Image {
        source: ImageSource::url_with(url, http),
        frame: None,
        fit: ObjectFit::Fill,
        position: (Length::Points(0.0), Length::Points(0.0)),
    });
    scene
        .push(root, node)
        .unwrap_or_else(|error| unreachable!("{error}"));
    scene
}

/// Serves one response of `body_len` zero bytes and stops listening.
fn serving(body_len: usize) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .unwrap_or_else(|error| unreachable!("{error}"));
    let port = listener
        .local_addr()
        .unwrap_or_else(|error| unreachable!("{error}"))
        .port();
    std::thread::spawn(move || {
        if let Some(Ok(mut stream)) = listener.incoming().next() {
            drain_request(&stream);
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {body_len}\r\n\
                 Content-Type: image/png\r\n\r\n"
            );
            let _ = stream.write_all(head.as_bytes());
            let chunk = vec![0_u8; 64 * 1024];
            let mut sent = 0;
            while sent < body_len {
                let take = chunk.len().min(body_len - sent);
                if stream.write_all(&chunk[..take]).is_err() {
                    break;
                }
                sent += take;
            }
            let _ = stream.flush();
        }
    });
    format!("http://127.0.0.1:{port}/big.png")
}

#[test]
fn an_image_past_the_limit_says_so_in_this_crate_s_own_words() {
    // One mebibyte past `MAX_IMAGE_BYTES`. The refusal arrives in about a
    // tenth of a second over loopback -- the limit stops the read rather than
    // measuring the whole body and then complaining.
    let refused =
        Renderer::new().render(&scene_naming(serving(33 * 1024 * 1024)));

    let Err(Error::SourceFetch {
        detail, failure, ..
    }) = refused
    else {
        unreachable!("an oversized image was not refused as a fetch failure");
    };

    // **The classification is the point.** Before `TooLarge` existed this
    // arrived as `Transport`, indistinguishable from a slow host -- and the
    // two want opposite responses: retry the slow one, never the large one.
    assert_eq!(failure, FetchFailure::TooLarge);
    assert!(
        detail.contains("32 MiB") && detail.contains("this renderer"),
        "the message quotes someone else's limit: {detail}"
    );
}

/// Reads the request before answering it.
///
/// **Without this the server is the defect.** Closing a socket with the
/// request still unread in the kernel's receive buffer sends an RST, which
/// discards whatever the client had not yet read -- so a 1 MiB response came
/// back as "Peer disconnected" with no timeouts configured and
/// "Invalid argument (os error 22)" with them, and both look exactly like a
/// library fault.
fn drain_request(stream: &std::net::TcpStream) {
    use std::io::{BufRead, BufReader};
    let Ok(clone) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(clone);
    let mut line = String::new();
    while reader.read_line(&mut line).unwrap_or(0) > 0 {
        if line == "\r\n" || line == "\n" {
            break;
        }
        line.clear();
    }
}

/// Serves one 1x1 PNG and hands back the request line and headers it read.
///
/// The request is returned rather than asserted on inside the thread: an
/// assertion that fails on a spawned thread fails the *thread*, and the test
/// carries on to whatever it does next with the panic recorded nowhere a
/// reader will look.
fn recording() -> (String, std::sync::mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .unwrap_or_else(|error| unreachable!("{error}"));
    let port = listener
        .local_addr()
        .unwrap_or_else(|error| unreachable!("{error}"))
        .port();
    let (send, receive) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        if let Some(Ok(mut stream)) = listener.incoming().next() {
            let _ = send.send(read_head(&stream));
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\
                 Content-Type: image/png\r\n\r\n",
                RED_DOT.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(RED_DOT);
            let _ = stream.flush();
        }
    });
    (format!("http://127.0.0.1:{port}/a.png"), receive)
}

/// A 1x1 PNG, so the response decodes and the render reaches its end.
const RED_DOT: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D,
    0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,
    0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00,
    0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0xF8, 0xCF, 0xC0, 0x00,
    0x00, 0x03, 0x01, 0x01, 0x00, 0x18, 0xDD, 0x8D, 0xB0, 0x00, 0x00, 0x00,
    0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

/// The request line and headers, as one string.
fn read_head(stream: &std::net::TcpStream) -> String {
    use std::io::{BufRead, BufReader};
    let Ok(clone) = stream.try_clone() else {
        return String::new();
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
    head
}

#[test]
fn a_source_s_headers_reach_the_server() {
    let (url, received) = recording();
    let http = HttpOptions::new()
        .header("authorization", "Bearer probe")
        .header("x-probe", "two");

    let rendered = Renderer::new().render(&scene_sending(url, http));
    assert!(rendered.is_ok(), "the render failed: {rendered:?}");

    let head = received
        .recv_timeout(std::time::Duration::from_secs(10))
        .unwrap_or_else(|error| unreachable!("{error}"));
    let sent = head.to_ascii_lowercase();

    // **Both, because one header proves less than it looks.** A writer that
    // sent only the first, or that overwrote each with the next, passes a
    // single-header assertion; the second header is what separates "the list
    // is sent" from "a header is sent".
    assert!(
        sent.contains("authorization: bearer probe"),
        "the first header did not arrive: {head}"
    );
    assert!(
        sent.contains("x-probe: two"),
        "the second header did not arrive: {head}"
    );
}

#[test]
fn a_header_a_request_cannot_carry_is_refused_before_it_is_sent() {
    let (url, received) = recording();
    // A newline in a value is request splitting if it is ever written to the
    // socket, which is why the assertion below is about the socket rather than
    // only about the error.
    let http = HttpOptions::new().header("x-probe", "one\r\nx-injected: two");

    let refused = Renderer::new().render(&scene_sending(url, http));

    let Err(Error::SourceFetch { failure, .. }) = refused else {
        unreachable!("a malformed header was not refused: {refused:?}");
    };
    // `Other` rather than a variant of its own: `http`'s own grammar is what
    // rejects it, the advice `Other` carries -- do not retry blindly -- is the
    // right advice, and a second grammar here would disagree with the first on
    // whatever nobody enumerated.
    assert_eq!(failure, FetchFailure::Other);

    // **The discriminating half.** An error alone is also what a request sent
    // and then rejected looks like. Nothing was accepted, so nothing was read.
    assert!(
        received
            .recv_timeout(std::time::Duration::from_millis(250))
            .is_err(),
        "the request was sent before it was refused"
    );
}
