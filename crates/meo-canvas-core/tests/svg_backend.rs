//! The `meo-skia-canvas` SVG document API, [`Svg`], at this crate's feature
//! set: it links and rasterises with `set_current_color` applied. Whether the
//! tint changes the drawing is asserted in `svg_source.rs`, which reads pixels.

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
