//! The rhythm a dotted border is drawn with, against Chrome.
//!
//! # What the table settles
//!
//! **A dot is `w` wide and the gap between two is `w`**, at every measured
//! width. A side carries an open run of `n` dots and `n - 1` gaps, **flush at
//! both ends**, so the count is `(edge / w + 1) / 2` taken to the nearest
//! whole number -- zero mismatches in thirty rows, against ten for ceiling and
//! fourteen for floor.
//!
//! **That count is not a dotted rule.** It is
//! [`meo_canvas_core::paint::fitted_dash`]'s own,
//! `round((length + gap) / (dash + gap))`, with a nominal pattern of `w` on
//! and `w` off. The dashed and dotted tables were each taken to be measuring
//! their own rule and were confirming one, which is stronger evidence than
//! either was thought to be -- two instruments, two patterns, one answer
//! neither was looking for.
//!
//! # Why 137 and not 240
//!
//! `(240 / w + 1) / 2` is an exact half at every width Chrome was read at --
//! 120.5, 60.5, 40.5, 30.5, 15.5 -- and **a tie is where a rounding rule is
//! undetermined**. Chrome's own tie answers disagree with each other: 120 at
//! width 1 and 61 at width 2, down then up. A rule read off a tie is a coin
//! toss recorded as a measurement. 137 leaves a remainder at every width.
//!
//! # Read through the renderer
//!
//! Every assertion here paints a box and reads the ink back. A test that
//! checks the arithmetic while the renderer hands it a different number is
//! what let a dashed border fit the wrong length for a whole day.
//!
//! Measured through `just conformance`;
//! `crates/meo-canvas/tests/assets/chrome/dotted-rhythm.tsv`.

use meo_canvas_core::{ImageFormat, Renderer, encode::EncodeOptions};
use meo_canvas_scene::{
    Scene, Sides, Size,
    node::{Node, NodeId, NodeKind},
    style::{
        Dimension,
        paint::{BorderStyle, Color},
    },
};

/// Chrome's own rows.
const TABLE: &str =
    include_str!("../../meo-canvas/tests/assets/chrome/dotted-rhythm.tsv");

/// The edge every `137` row was read along.
const EDGE: f32 = 137.0;
/// The box's height, which is also the side the vertical runs are fitted to.
const HEIGHT: f32 = 48.0;
/// Ink is anything below this in the red channel, as the harness reads it.
const INK: u8 = 128;

/// One row of the table, split on tabs.
fn rows(prefix: &str) -> impl Iterator<Item = Vec<&'static str>> {
    TABLE
        .lines()
        .filter(move |line| line.starts_with(prefix))
        .map(|line| line.split('\t').collect())
}

/// Renders a dotted box and returns its pixels.
fn dotted(width: f32) -> (usize, Vec<u8>) {
    let mut scene = Scene::new(Size::new(EDGE, HEIGHT));
    if let Some(root) = scene.get_mut(NodeId::ROOT) {
        root.paint.background_color = Color::rgb(255, 255, 255);
    }
    let id = scene
        .push(NodeId::ROOT, Node::new(NodeKind::Box))
        .unwrap_or_else(|error| unreachable!("{error}"));
    if let Some(node) = scene.get_mut(id) {
        node.layout.size = (Dimension::Points(EDGE), Dimension::Points(HEIGHT));
        node.layout.border = Sides::all(width);
        node.paint.background_color = Color::rgb(255, 255, 255);
        node.paint.border_color_all = Color::rgb(0, 0, 0);
        node.paint.border_style = BorderStyle::Dotted;
    }
    let mut renderer = Renderer::new();
    // The two rasterisers do not agree to the byte, and this reads bytes.
    renderer.set_gpu(false);
    let png = renderer
        .render_to_buffer(&scene, ImageFormat::Png, &EncodeOptions::default())
        .unwrap_or_else(|error| unreachable!("it did not render: {error}"));
    let mut decoder = png::Decoder::new(std::io::Cursor::new(png));
    decoder.set_transformations(
        png::Transformations::normalize_to_color8()
            | png::Transformations::ALPHA,
    );
    let mut reader = decoder
        .read_info()
        .unwrap_or_else(|error| unreachable!("{error}"));
    let mut pixels = vec![0; reader.output_buffer_size().unwrap_or(0)];
    let info = reader
        .next_frame(&mut pixels)
        .unwrap_or_else(|error| unreachable!("{error}"));
    pixels.truncate(info.buffer_size());
    (info.width as usize, pixels)
}

/// The ink runs along one row, as `(is_ink, length)` in order.
fn runs(stride: usize, pixels: &[u8], y: usize) -> Vec<(bool, usize)> {
    let mut out: Vec<(bool, usize)> = Vec::new();
    for x in 0..stride {
        let ink = pixels[((y * stride) + x) * 4] < INK;
        match out.last_mut() {
            Some(last) if last.0 == ink => last.1 += 1,
            _ => out.push((ink, 1)),
        }
    }
    out
}

#[test]
fn a_side_is_flush_at_both_ends() {
    let mut checked = 0;
    for row in rows("dotted-span-137") {
        let width: f32 = row[1]
            .parse()
            .unwrap_or_else(|error| unreachable!("{error}"));
        let y: usize = row[2]
            .parse()
            .unwrap_or_else(|error| unreachable!("{error}"));
        let (stride, pixels) = dotted(width);
        let inked = |x: usize| pixels[((y * stride) + x) * 4] < INK;
        let first = (0..stride).find(|&x| inked(x));
        let last = (0..stride).rev().find(|&x| inked(x));

        // Chrome reads `first@0 last@136 trailing=0` at every width. Ours was
        // short at the far end by up to six pixels before the run was inset
        // by half a width to account for the round cap's reach.
        assert_eq!(first, Some(0), "width {width}: the first dot is not flush");
        assert_eq!(
            last,
            Some(stride - 1),
            "width {width}: the last dot is not flush"
        );
        checked += 1;
    }
    assert_eq!(checked, 5, "the table should carry five widths at 137");
}

#[test]
fn a_side_holds_the_dots_chrome_counts() {
    for row in rows("dotted-span-137") {
        let width: f32 = row[1]
            .parse()
            .unwrap_or_else(|error| unreachable!("{error}"));
        let y: usize = row[2]
            .parse()
            .unwrap_or_else(|error| unreachable!("{error}"));
        let (stride, pixels) = dotted(width);
        let ours = runs(stride, &pixels, y).iter().filter(|run| run.0).count();

        // `(edge / w + 1) / 2` to nearest, spelled as the general count with
        // both terms `w` so it is visibly the same rule the dashed table
        // measured.
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a count of dots along a 137-pixel edge"
        )]
        let expected = ((EDGE + width) / (2.0 * width)).round() as usize;
        assert_eq!(
            ours, expected,
            "width {width}: we draw {ours} dots where the rule gives {expected}"
        );
    }
}

#[test]
fn the_dots_are_the_width_and_so_are_the_gaps() {
    // Away from the ends, where a run is neither starting nor closing: every
    // dot is `w` and every gap is `w`, give or take the pixel a fractional
    // phase moves a boundary by. **Assert the structure and the span, never
    // every run length** -- a mark of exactly `w` starting at a fractional
    // offset covers `w - 1`, `w` or `w + 1` pixels depending on where it
    // falls, and the geometry has not changed.
    for row in rows("dotted-span-137") {
        let width: f32 = row[1]
            .parse()
            .unwrap_or_else(|error| unreachable!("{error}"));
        let y: usize = row[2]
            .parse()
            .unwrap_or_else(|error| unreachable!("{error}"));
        let (stride, pixels) = dotted(width);
        let all = runs(stride, &pixels, y);
        let middle = &all[2..all.len().saturating_sub(2)];
        for &(ink, length) in middle {
            let nominal = width as usize;
            let kind = if ink { "dot" } else { "gap" };
            assert!(
                length.abs_diff(nominal) <= 1,
                "width {width}: a {kind} of {length} where {nominal} is nominal"
            );
        }
    }
}
