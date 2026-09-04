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
    Length, Scene, Size,
    node::{ImageSource, Node, NodeKind},
    style::paint::ObjectFit,
};

/// A scene whose one node names a URL, which is the only way to reach `fetch`
/// from outside the crate.
fn scene_naming(url: String) -> Scene {
    let mut scene = Scene::new(Size::new(64.0, 64.0));
    let root = scene
        .root()
        .unwrap_or_else(|| unreachable!("a new scene has a root"));
    let node = Node::new(NodeKind::Image {
        source: ImageSource::Url(url),
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
