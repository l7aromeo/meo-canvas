//! The Skia linked is meo-skia-canvas's patched one, which draws what the npm
//! package draws. A build that took crates.io's Skia instead would pass every
//! test that draws nothing exact, so this asks the crate which tree it linked.

#[test]
fn the_linked_skia_is_the_patched_one() {
    let tree = meo_skia_canvas::skia_patches();
    assert!(
        tree.starts_with("skia-") && tree.len() > "skia-".len(),
        "the linked Skia names no patch set ({tree:?}); `patched-skia` with the \
         justfile's SKIA_BINARIES_URL links one"
    );
}
