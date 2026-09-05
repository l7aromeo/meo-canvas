//! The SVG document API the 0.15 backend adds, exercised at our feature set.
//!
//! # Why this test exists at all
//!
//! `meo-skia-canvas` 0.15.0 is the first version carrying [`Svg`], and nothing
//! in this repository consumes SVG as an image source yet. So the bump would
//! otherwise be a version number with no assertion behind it, and the way to
//! tell a dependency actually moved is to name something that only exists after
//! it did.
//!
//! **This file cannot compile against 0.14.** `Svg` is not in that version, so
//! reverting the pin fails the build rather than failing an assertion. That is
//! the reversion control here, stated rather than dressed up as a runtime
//! check: a bump adds surface, and a test that still compiled without it would
//! be testing nothing.
//!
//! # What is deliberately not asserted
//!
//! **That `set_current_color` changes the drawing.** It is called here and the
//! rasterization is checked to still succeed, which is the linkage claim. The
//! visual effect needs pixels, and an [`Image`] exposes no pixel accessor with
//! `default-features = false` -- reading it back means building a
//! `crate::paint::Surface` and going through `encode`, which is plumbing that
//! belongs with the SVG node rather than with a dependency bump. Whoever lands
//! that node owns the measurement, where the surface already exists.

use meo_skia_canvas::{RgbaLinear, image::Svg};

/// A document with an explicit `viewBox` and explicit pixel dimensions.
const SIZED: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20" viewBox="0 0 40 20"><rect width="40" height="20" fill="currentColor"/></svg>"#;

/// The same drawing with no stated size, which is what `is_autosized` is for.
const AUTOSIZED: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 20"><rect width="40" height="20" fill="currentColor"/></svg>"#;

#[test]
fn a_document_reports_the_size_it_states() {
    let svg = Svg::parse(SIZED).unwrap_or_else(|error| unreachable!("{error}"));

    let size = svg.intrinsic_size();
    assert_eq!(
        (size.width, size.height),
        (40.0, 20.0),
        "a document with explicit dimensions should report them"
    );
    assert!(
        !svg.is_autosized(),
        "a document that states its size is not autosized"
    );
}

#[test]
fn a_document_with_no_stated_size_says_so() {
    let svg =
        Svg::parse(AUTOSIZED).unwrap_or_else(|error| unreachable!("{error}"));

    // **The pair is the point.** `is_autosized` returning true for everything
    // would pass on its own, so the sized document above is this test's
    // control and the two must disagree.
    assert!(
        svg.is_autosized(),
        "a document with no width or height is autosized"
    );
}

#[test]
fn rasterizing_honours_the_size_asked_for_rather_than_the_document_s() {
    let mut svg =
        Svg::parse(SIZED).unwrap_or_else(|error| unreachable!("{error}"));

    // Deliberately neither the intrinsic size nor a multiple of it, so a
    // backend that ignored the argument could not land here by accident.
    let image = svg
        .rasterize(37, 11)
        .unwrap_or_else(|error| unreachable!("{error}"));

    assert_eq!((image.width(), image.height()), (37, 11));
}

#[test]
fn a_zero_dimension_is_refused_rather_than_allocated() {
    let mut svg =
        Svg::parse(SIZED).unwrap_or_else(|error| unreachable!("{error}"));

    assert!(
        svg.rasterize(0, 10).is_err(),
        "a zero width should be refused"
    );
    assert!(
        svg.rasterize(10, 0).is_err(),
        "a zero height should be refused"
    );
    // The control: the same document at a real size still works, so the two
    // refusals above are about the dimension and not about the document.
    assert!(svg.rasterize(10, 10).is_ok());
}

#[test]
fn the_current_colour_can_be_set_and_the_document_still_rasterizes() {
    let mut svg =
        Svg::parse(SIZED).unwrap_or_else(|error| unreachable!("{error}"));

    svg.set_current_color(RgbaLinear {
        r: 1.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    });

    // Linkage only -- see the module doc for why the drawn result is not
    // asserted here and who owns that measurement.
    assert!(svg.rasterize(40, 20).is_ok());
}

#[test]
fn a_document_that_is_not_svg_is_refused() {
    assert!(Svg::parse("not markup at all").is_err());
    assert!(Svg::parse("").is_err());
}
