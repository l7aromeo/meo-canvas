//! What `decode` reserves for a count it has not read yet.
//!
//! **The ratio is the assertion.** A `Vec::with_capacity(count)` in
//! `Reader::list` turned one megabyte of input into 1.02 GB of reservation --
//! `Node` is 1048 bytes in memory against 184 on the wire, and the count is
//! bounded by the bytes remaining rather than by the memory they can justify.
//! The bound above that line is correct about the count and says so accurately,
//! which is the worst place for the defect to be: the comment reads as though
//! the problem is handled.
//!
//! A counting allocator measures it rather than arguing about it, so a future
//! `with_capacity` cannot reintroduce it quietly.
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicUsize, Ordering},
};

use meo_canvas_scene::{Scene, Size, codec, node::Node};

static PEAK: AtomicUsize = AtomicUsize::new(0);
static LIVE: AtomicUsize = AtomicUsize::new(0);

struct Counting;

// SAFETY: every method forwards to `System`, which is a correct allocator, and
// adds only two relaxed atomic counters around it. The pointer handed to
// `dealloc` is one `System` returned, because `alloc` is the only source.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let live =
            LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
        PEAK.fetch_max(live, Ordering::Relaxed);
        // SAFETY: `layout` is the caller's, forwarded unchanged.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        // SAFETY: `ptr` and `layout` are the caller's, and `ptr` came from
        // this allocator, which allocates only through `System`.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOC: Counting = Counting;

/// A header `decode` accepts, up to and including a node count it will not be
/// able to honour, followed by `filler` bytes of nothing.
fn declares(count: u32, filler: usize) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"MCSC");
    bytes.extend_from_slice(&5_u16.to_le_bytes());
    bytes.extend_from_slice(&100.0_f32.to_le_bytes());
    bytes.extend_from_slice(&100.0_f32.to_le_bytes());
    bytes.push(0);
    bytes.extend_from_slice(&1.0_f32.to_le_bytes());
    bytes.push(0);
    bytes.push(0);
    bytes.push(0);
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(&count.to_le_bytes());
    bytes.resize(bytes.len() + filler, 0);
    bytes
}

#[test]
fn a_declared_count_cannot_reserve_much_more_than_the_input_is_long() {
    // The worst case the format allows: `MAX_NODES` declared, and just enough
    // filler that the count is not refused as larger than the bytes left.
    let count = 1_u32 << 20;
    let bytes = declares(count, count as usize);

    PEAK.store(0, Ordering::Relaxed);
    let floor = LIVE.load(Ordering::Relaxed);
    let outcome = codec::decode(&bytes);
    let peak = PEAK.load(Ordering::Relaxed).saturating_sub(floor);
    let ratio = peak / bytes.len();

    assert!(
        outcome.is_err(),
        "a header with no nodes behind it is refused"
    );
    // **1047 before the reservation was bounded, 17 after**, both measured
    // here. Six of the seventeen are the reservation itself --
    // `size_of::<Node>() / Node::MIN_ENCODED`, which cannot be less than one
    // and should not be much more -- and the rest is the decode running until
    // it discovers there are no nodes behind the count, which allocates as it
    // goes and is not the defect. Twenty-four leaves room for `Node` growing a
    // field without leaving room for the reservation coming back.
    assert!(
        ratio <= 24,
        "decoding {} bytes reserved {peak} ({ratio}x); the reservation is not \
         bounded by what those bytes could contain",
        bytes.len()
    );
}

#[test]
fn a_node_never_encodes_smaller_than_the_reservation_assumes() {
    // `Node::MIN_ENCODED` is what the reservation divides by, so it has to be
    // a true floor. Measured against the smallest node there is: a default
    // container, which costs its own bytes plus the four its parent spends
    // naming it.
    let mut scene = Scene::new(Size::new(1.0, 1.0));
    let root = scene
        .root()
        .unwrap_or_else(|| unreachable!("a scene has one"));
    let empty = codec::encode(&scene).len();
    scene
        .push(root, Node::container())
        .unwrap_or_else(|error| unreachable!("{error}"));
    let one = codec::encode(&scene).len();

    let marginal = one - empty;
    assert!(
        marginal >= 184,
        "a default container encodes to {marginal} bytes, which is below the \
         184 the reservation assumes -- lower `Node::MIN_ENCODED` to match"
    );
}
