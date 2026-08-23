//! What a chart draws, read off the pixels.
//!
//! # Why this exists beside the agreement table
//!
//! `chart_geometry.rs` checks the Rust arithmetic against the TypeScript
//! surface's own numbers. **Two implementations agreeing is evidence about the
//! port and no evidence about the geometry** -- both surfaces agreeing on a
//! wrong bar edge passes every row there. And there is nothing external to
//! appeal to: Chrome has no charts.
//!
//! **So this renders and measures.** It is the only thing here that touches
//! the picture.
//!
//! # Derived, then checked against the pin
//!
//! Every expectation below is worked out from the arithmetic rather than
//! copied: two labels give a group width of half the plot, `BAR_GROUP_SPACING`
//! takes a tenth, so a bar is 40% wide and the first starts at 5%. On a
//! 200-pixel plot that is `x 10..89` and `x 110..189` -- **which is what the
//! TypeScript render tests measured independently.** Two sources agreeing is
//! the point; one source copied twice would not be.

use meo_canvas::{
    Element, EncodeOptions, Format, Renderer,
    chart::bar::{Dataset, Options, bar},
};

/// The page every case is drawn on.
const SIZE: (f32, f32) = (200.0, 120.0);
/// A pixel counts as drawn when it is at least this opaque.
///
/// **Alpha rather than darkness.** The page has no background, so an unpainted
/// pixel in the raw buffer is transparent black -- and a test reading the red
/// channel called every one of them ink and passed nothing. The question here
/// is whether a bar was drawn, which is what alpha answers.
const DRAWN: u8 = 128;

/// Renders one chart and returns its pixels with the row stride.
///
/// **Raw rather than PNG**, so this crate needs no decoder in its
/// dev-dependencies: `Format::Raw` is the surface's own bytes with no
/// container, four channels per pixel in row order.
fn pixels(chart: Element) -> (usize, Vec<u8>) {
    let scene = chart.into_scene(SIZE.0, SIZE.1).unwrap_or_else(|error| {
        unreachable!("the chart is not a scene: {error}")
    });
    let mut renderer = Renderer::new();
    // The two rasterisers do not agree to the byte, and this reads bytes.
    renderer.set_gpu(false);
    let raw = renderer
        .render_to_buffer(&scene, Format::Raw, &EncodeOptions::default())
        .unwrap_or_else(|error| unreachable!("it did not render: {error}"));
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a page 200 pixels wide"
    )]
    let stride = SIZE.0 as usize;
    (stride, raw)
}

/// The inked columns of one row, and the inked rows of one column.
fn inked(stride: usize, buffer: &[u8], along: usize, row: bool) -> Vec<usize> {
    let height = buffer.len() / (stride * 4);
    (0..if row { stride } else { height })
        .filter(|index| {
            let (x, y) = if row {
                (*index, along)
            } else {
                (along, *index)
            };
            buffer[(((y * stride) + x) * 4) + 3] >= DRAWN
        })
        .collect()
}

/// Two labels, one series, nothing but bars.
fn two_bars(values: Vec<f64>) -> Element {
    bar(
        &["a".to_owned(), "b".to_owned()],
        &[Dataset {
            data: values,
            ..Dataset::default()
        }],
        &Options::default(),
    )
    .unwrap_or_else(|error| unreachable!("{error}"))
}

#[test]
fn a_bar_is_where_the_arithmetic_puts_it() {
    // Two labels: a group is half the plot, spacing takes a tenth of a group,
    // so a bar is 40% of 200 = 80 wide and the first starts at 5% = 10.
    let (stride, buffer) = pixels(two_bars(vec![1.0, 2.0]));
    let columns = inked(stride, &buffer, 119, true);
    let (first, last) = (columns[0], columns[columns.len() - 1]);
    assert_eq!(first, 10, "the first bar starts at {first} rather than 10");
    assert_eq!(last, 189, "the last bar ends at {last} rather than 189");

    // And the gap between them is the spacing, not a joined block.
    let gap: Vec<usize> = (90..110)
        .filter(|x| buffer[(((119 * stride) + x) * 4) + 3] >= DRAWN)
        .collect();
    assert!(gap.is_empty(), "the bars run together across {gap:?}");
}

#[test]
fn a_bar_is_as_tall_as_its_share_of_the_maximum() {
    // Values 1 and 2 against a maximum of 2: half height and full height, on
    // a plot that is the whole 120 because nothing else is shown.
    let (stride, buffer) = pixels(two_bars(vec![1.0, 2.0]));
    let short = inked(stride, &buffer, 50, false);
    let tall = inked(stride, &buffer, 150, false);
    assert_eq!(tall.len(), 120, "the full bar covers {} rows", tall.len());
    assert!(
        (59..=61).contains(&short.len()),
        "the half bar covers {} rows rather than about 60",
        short.len()
    );
    assert_eq!(
        short[short.len() - 1],
        119,
        "the half bar does not sit on the floor"
    );
}

#[test]
fn an_all_zero_chart_draws_nothing_rather_than_failing() {
    // The stated divergence: v1 divides by a zero maximum and `NaN` reaches
    // layout as an absent height, so the chart draws nothing and reads as a
    // broken renderer. Zero is the honest height, and an empty plot is the
    // honest picture.
    let (stride, buffer) = pixels(two_bars(vec![0.0, 0.0]));
    let columns = inked(stride, &buffer, 119, true);
    assert!(
        columns.is_empty(),
        "an all-zero chart drew {} columns",
        columns.len()
    );
}

#[test]
fn the_measurement_can_tell_a_wrong_layout_from_a_right_one() {
    // The control. Every assertion above would pass against a chart drawn to
    // a different rule if the reading could not see the difference -- so this
    // draws bars at a deliberately wrong width and asserts the measurement
    // moves. A test that cannot fail is the thing being avoided.
    let wrong = bar(
        &["a".to_owned(), "b".to_owned(), "c".to_owned()],
        &[Dataset {
            data: vec![1.0, 1.0, 1.0],
            ..Dataset::default()
        }],
        &Options::default(),
    )
    .unwrap_or_else(|error| unreachable!("{error}"));
    let (stride, buffer) = pixels(wrong);
    let columns = inked(stride, &buffer, 119, true);
    assert_ne!(
        columns[columns.len() - 1],
        189,
        "three labels put the last bar where two did, so the reading is not \
         measuring the layout"
    );
}
