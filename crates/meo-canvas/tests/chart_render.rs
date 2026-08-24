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
    chart::{
        bar::{Dataset, Options, bar},
        frame::LegendPosition,
        line::line,
        pie::{Slice, doughnut, pie},
    },
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
    pixels_at(chart, SIZE)
}

/// The same, on a page of a stated size.
///
/// **A second size is what makes the pen measurable.** A stroke that scaled
/// with the drawing would be thicker on a taller page and the same on both if
/// it does not, so one page cannot answer the question and two can.
fn pixels_at(chart: Element, size: (f32, f32)) -> (usize, Vec<u8>) {
    let scene = chart.into_scene(size.0, size.1).unwrap_or_else(|error| {
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
    let stride = size.0 as usize;
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

/// A pie of four equal slices, or a doughnut when `inner` is above zero.
fn four_slices(inner: f64) -> Element {
    let slices: Vec<Slice> = ["a", "b", "c", "d"]
        .into_iter()
        .map(|label| Slice {
            label: label.to_owned(),
            value: 1.0,
            color: None,
        })
        .collect();
    // The hole is an option now rather than an argument, and the two kinds are
    // two functions -- `pie` has no hole and `doughnut` reads the option.
    if inner > 0.0 {
        doughnut(
            &slices,
            &Options {
                inner_fraction: Some(inner),
                ..Options::default()
            },
        )
    } else {
        pie(&slices, &Options::default())
    }
    .unwrap_or_else(|error| unreachable!("{error}"))
}

/// How much of a small disc at the plot's centre is drawn.
///
/// **The measure that separates a pie from a doughnut**, and the one Agent
/// Zero's TypeScript renders use: a pie fills its middle and a doughnut has a
/// hole there, so the share of a centre disc that is painted is near total for
/// one and nothing for the other.
fn centre_disc(stride: usize, buffer: &[u8], radius: usize) -> f64 {
    let height = buffer.len() / (stride * 4);
    let (cx, cy) = (stride / 2, height / 2);
    let (mut drawn, mut total) = (0.0, 0.0);
    for y in cy - radius..=cy + radius {
        for x in cx - radius..=cx + radius {
            let (dx, dy) = (x.abs_diff(cx), y.abs_diff(cy));
            if dx * dx + dy * dy > radius * radius {
                continue;
            }
            total += 1.0;
            if buffer[(((y * stride) + x) * 4) + 3] >= DRAWN {
                drawn += 1.0;
            }
        }
    }
    if total == 0.0 { 0.0 } else { drawn / total }
}

#[test]
fn a_pie_fills_its_middle_and_a_doughnut_does_not() {
    // The kinds differ by exactly this and by nothing else in the builder, so
    // it is the one reading that tells them apart.
    let (stride, pie_pixels) = pixels(four_slices(0.0));
    let (_, ring_pixels) = pixels(four_slices(0.5));
    let filled = centre_disc(stride, &pie_pixels, 8);
    let hollow = centre_disc(stride, &ring_pixels, 8);
    assert!(filled > 0.9, "a pie's middle is {filled:.2} covered");
    assert!(hollow < 0.05, "a doughnut's middle is {hollow:.2} covered");
}

#[test]
fn a_pie_stays_circular_in_a_box_that_is_not_square() {
    // The page is 200x120, so a drawing that stretched would be half as wide
    // again as it is tall. `min(w, h)` and `xMidYMid meet` agree that it must
    // not, which is why these kinds needed only a viewBox.
    let (stride, buffer) = pixels(four_slices(0.0));
    let height = buffer.len() / (stride * 4);
    let row = inked(stride, &buffer, height / 2, true);
    let column = inked(stride, &buffer, stride / 2, false);
    let (wide, tall) = (row.len(), column.len());
    assert!(
        wide.abs_diff(tall) <= 2,
        "the pie is {wide} across and {tall} down, so it is an ellipse"
    );
}

#[test]
fn the_centre_disc_measure_can_tell_the_two_kinds_apart() {
    // The control for the reading itself. If `centre_disc` returned the same
    // number for both kinds, the assertion above would be measuring nothing
    // -- so the gap between them is what is asserted here rather than either
    // value.
    let (stride, pie_pixels) = pixels(four_slices(0.0));
    let (_, ring_pixels) = pixels(four_slices(0.5));
    let gap = centre_disc(stride, &pie_pixels, 8)
        - centre_disc(stride, &ring_pixels, 8);
    assert!(
        gap > 0.8,
        "the two kinds differ by {gap:.2}, which is not a hole"
    );
}

/// Three labels, one series, nothing but the line and its markers.
fn one_line(values: Vec<f64>) -> Element {
    line(
        &["a".to_owned(), "b".to_owned(), "c".to_owned()],
        &[Dataset {
            data: values,
            ..Dataset::default()
        }],
        &Options::default(),
    )
    .unwrap_or_else(|error| unreachable!("{error}"))
}

#[test]
fn a_line_spans_every_column_and_reaches_both_extreme_rows() {
    // Three labels put points at 0, 0.5 and 1 across; `[0, 1, 0]` against a
    // maximum of 1 puts them on the floor, the ceiling and the floor. So the
    // drawing touches all four edges -- and every column between them, since
    // the path is continuous.
    let (stride, buffer) = pixels(one_line(vec![0.0, 1.0, 0.0]));
    let height = buffer.len() / (stride * 4);

    let bare: Vec<usize> = (0..stride)
        .filter(|x| inked(stride, &buffer, *x, false).is_empty())
        .collect();
    assert!(bare.is_empty(), "the line misses columns {bare:?}");

    assert!(
        !inked(stride, &buffer, 0, true).is_empty(),
        "nothing is drawn on the top row, so the peak did not reach it"
    );
    assert!(
        !inked(stride, &buffer, height - 1, true).is_empty(),
        "nothing is drawn on the bottom row, so the ends did not reach it"
    );
}

#[test]
fn the_plot_is_stretched_rather_than_fitted_into_a_square() {
    // The control for the test above, and the reason `stretch` exists. Under
    // `meet` a hundred-by-hundred drawing on a 200x120 page is letterboxed
    // into the middle 120 columns, leaving 40 blank at each side -- so ink in
    // column 0 is the whole difference between the two rules.
    let (stride, buffer) = pixels(one_line(vec![0.0, 1.0, 0.0]));
    let letterboxed =
        (0..40).any(|x| !inked(stride, &buffer, x, false).is_empty());
    assert!(
        letterboxed,
        "columns 0..40 are blank, which is what `meet` would leave"
    );
}

/// How many rows of one column are inked inside a band.
fn thickness(
    stride: usize,
    buffer: &[u8],
    column: usize,
    band: (usize, usize),
) -> usize {
    inked(stride, buffer, column, false)
        .into_iter()
        .filter(|y| *y >= band.0 && *y <= band.1)
        .count()
}

#[test]
fn the_pen_is_not_stretched_with_the_drawing() {
    // A flat series halfway up, drawn on a page of 120 and again on one of
    // 60. The vertical scale differs by two between them, so a pen that
    // scaled would draw four pixels on one and two on the other. It draws two
    // on both: `view_box` scales the drawing and not the pen.
    //
    // Measured at column 50, which is between the markers at 0, 100 and 200.
    let flat = || {
        line(
            &["a".to_owned(), "b".to_owned(), "c".to_owned()],
            &[
                Dataset {
                    data: vec![1.0, 1.0, 1.0],
                    ..Dataset::default()
                },
                Dataset {
                    data: vec![2.0, 2.0, 2.0],
                    ..Dataset::default()
                },
            ],
            &Options::default(),
        )
        .unwrap_or_else(|error| unreachable!("{error}"))
    };
    let (stride, tall) = pixels_at(flat(), (200.0, 120.0));
    let (_, short) = pixels_at(flat(), (200.0, 60.0));

    // The lower series sits at half height on both pages: row 60 of 120 and
    // row 30 of 60.
    let on_tall = thickness(stride, &tall, 50, (55, 65));
    let on_short = thickness(stride, &short, 50, (25, 35));
    assert_eq!(
        on_tall, on_short,
        "the stroke is {on_tall} pixels on a tall page and {on_short} on a \
         short one, so the pen scaled with the drawing"
    );
    assert_eq!(on_tall, 2, "the stroke is {on_tall} pixels rather than 2");
}

#[test]
fn the_two_pages_do_draw_the_same_line_at_different_scales() {
    // The control for the pen test. Two equal thicknesses prove nothing if
    // the drawing itself did not change between the pages -- so this asserts
    // that it did, by the distance between the two series.
    let apart = |size: (f32, f32)| {
        let (stride, buffer) = pixels_at(
            line(
                &["a".to_owned(), "b".to_owned(), "c".to_owned()],
                &[
                    Dataset {
                        data: vec![1.0, 1.0, 1.0],
                        ..Dataset::default()
                    },
                    Dataset {
                        data: vec![2.0, 2.0, 2.0],
                        ..Dataset::default()
                    },
                ],
                &Options::default(),
            )
            .unwrap_or_else(|error| unreachable!("{error}")),
            size,
        );
        let rows = inked(stride, &buffer, 50, false);
        rows[rows.len() - 1] - rows[0]
    };
    let (tall, short) = (apart((200.0, 120.0)), apart((200.0, 60.0)));
    assert!(
        tall > short + 20,
        "the series are {tall} apart on a tall page and {short} on a short \
         one, so the drawing did not scale and the pen test measured nothing"
    );
}

#[test]
fn a_point_marker_is_centred_on_its_point_and_is_round() {
    // `[2, 1, 2]` puts the middle point at half height: left 50% of 200 and
    // top 50% of 120, then pulled back by half its own eight pixels -- so the
    // marker occupies columns 96..=103 and rows 56..=63.
    let (stride, buffer) = pixels(one_line(vec![2.0, 1.0, 2.0]));

    let across: Vec<usize> = inked(stride, &buffer, 60, true)
        .into_iter()
        .filter(|x| (90..110).contains(x))
        .collect();
    let (first, last) = (across[0], across[across.len() - 1]);
    assert_eq!(
        (first, last),
        (96, 103),
        "the marker spans {first}..={last} rather than 96..=103, so the \
         half-its-own-size pull did not centre it"
    );

    // A row near the marker's bottom edge cuts a chord rather than the full
    // width, which a square would not. The line's own stroke is above it: the
    // vertex is the lowest the path goes.
    let chord = inked(stride, &buffer, 63, true)
        .into_iter()
        .filter(|x| (90..110).contains(x))
        .count();
    assert!(
        (1..across.len()).contains(&chord),
        "row 63 is {chord} wide against the marker's {}, so the marker is a \
         square rather than a disc",
        across.len()
    );
}

#[test]
fn the_label_strip_sits_under_the_plot_rather_than_beside_it() {
    // **The regression the pixels could not see until it was asked this
    // question.** `with_style` replaces a style rather than merging it, so a
    // `Column::new().with_style(...)` discarded the column direction and laid
    // the plot and the label strip out side by side. Every measurement of a
    // bar *within* the plot still passed -- the arithmetic was never wrong --
    // and the picture was. Agent Zero's byte comparison found it; this is the
    // rendered question that would have.
    let chart = bar(
        &["a".to_owned(), "b".to_owned()],
        &[Dataset {
            data: vec![1.0, 1.0],
            ..Dataset::default()
        }],
        &Options {
            show_labels: true,
            ..Options::default()
        },
    )
    .unwrap_or_else(|error| unreachable!("{error}"));
    let (stride, buffer) = pixels(chart);

    // Both values equal the maximum, so the bars are full height and the top
    // row crosses them. Side by side, the plot would be a fraction of the
    // width and the second bar would end far short of 189.
    let top = inked(stride, &buffer, 0, true);
    let (first, last) = (top[0], top[top.len() - 1]);
    assert_eq!(
        (first, last),
        (10, 189),
        "the top row runs {first}..={last}, so the plot is not the full width"
    );

    // And the strip has rows of its own below the bars: a row whose ink is a
    // few label glyphs rather than 160 columns of bar.
    let strip = (0..buffer.len() / (stride * 4)).any(|y| {
        let count = inked(stride, &buffer, y, true).len();
        count > 0 && count < 40
    });
    assert!(
        strip,
        "no row holds label-sized ink, so the strip is not under the plot"
    );
    let bars = inked(stride, &buffer, 0, true).len();
    assert!(
        bars > 100,
        "the bar rows hold {bars} columns, so this measured the wrong thing"
    );
}

#[test]
fn a_label_is_centred_under_its_own_slot() {
    // **The only instrument that could ever have caught this.** Both surfaces
    // set `align-items: center` on a row, where that is the *cross* axis --
    // so the labels centred vertically and sat against the left edge of their
    // slots. The two implementations agreed to the byte and the geometry
    // table had no row for it; the pixels are what disagreed with v1.
    //
    // Two labels on a 200-wide page own 0..99 and 100..199, so their ink
    // straddles x = 50 and x = 150 rather than starting at 1 and 101.
    let chart = bar(
        &["a".to_owned(), "b".to_owned()],
        &[Dataset {
            data: vec![1.0, 1.0],
            ..Dataset::default()
        }],
        &Options {
            show_labels: true,
            ..Options::default()
        },
    )
    .unwrap_or_else(|error| unreachable!("{error}"));
    let (stride, buffer) = pixels(chart);

    // The strip is whatever rows hold label-sized ink rather than 160 columns
    // of bar, which is where the test above left them.
    let rows: Vec<usize> = (0..buffer.len() / (stride * 4))
        .filter(|y| {
            let count = inked(stride, &buffer, *y, true).len();
            count > 0 && count < 40
        })
        .collect();
    assert!(!rows.is_empty(), "there is no label strip to measure");

    let ink: Vec<usize> = rows
        .iter()
        .flat_map(|y| inked(stride, &buffer, *y, true))
        .collect();
    for (slot, middle) in [(0..100, 50), (100..200, 150)] {
        let own: Vec<usize> =
            ink.iter().copied().filter(|x| slot.contains(x)).collect();
        assert!(!own.is_empty(), "slot {slot:?} holds no label");
        let (first, last) = (own[0], own[own.len() - 1]);
        let centre = first.midpoint(last);
        assert!(
            centre.abs_diff(middle) <= 4,
            "the label in {slot:?} runs {first}..={last}, centred on {centre} \
             rather than {middle} -- so it is aligned to an edge"
        );
    }
}

/// The same chart with its legend on one of the four sides.
fn legended(position: LegendPosition) -> (usize, Vec<u8>) {
    pixels(
        bar(
            &["a".to_owned(), "b".to_owned()],
            &[Dataset {
                data: vec![1.0, 1.0],
                ..Dataset::default()
            }],
            &Options {
                show_legend: true,
                legend_position: position,
                ..Options::default()
            },
        )
        .unwrap_or_else(|error| unreachable!("{error}")),
    )
}

#[test]
fn the_legend_takes_the_side_it_is_given() {
    // Both values equal the maximum, so the bars are full height and fill the
    // plot's width from 5% to 95% of it. **The legend is a sibling, not an
    // overlay**, so whichever side it takes is a side the plot no longer has
    // -- which is what each of these four reads.
    let wide = 100;
    let narrow = 40;

    let (stride, top) = legended(LegendPosition::Top);
    let height = top.len() / (stride * 4);
    let (first, last) = (
        inked(stride, &top, 0, true).len(),
        inked(stride, &top, height - 1, true).len(),
    );
    assert!(
        first < narrow && last > wide,
        "with the legend on top the first row holds {first} and the last \
         {last}, so the plot did not move down"
    );

    let (_, bottom) = legended(LegendPosition::Bottom);
    let (first, last) = (
        inked(stride, &bottom, 0, true).len(),
        inked(stride, &bottom, height - 1, true).len(),
    );
    assert!(
        first > wide && last < narrow,
        "with the legend below, the first row holds {first} and the last \
         {last}, so it is not below"
    );

    // Beside the plot the legend takes width instead: the bars no longer
    // start at 10, and something is drawn in the columns before them.
    let (_, left) = legended(LegendPosition::Left);
    let bars = inked(stride, &left, height / 2, true);
    assert!(
        bars[0] > 20,
        "with the legend on the left the ink starts at {}, so the plot did \
         not move right",
        bars[0]
    );

    let (_, right) = legended(LegendPosition::Right);
    let bars = inked(stride, &right, height / 2, true);
    let end = bars[bars.len() - 1];
    // The plot keeps the left edge and loses the right, so the first bar
    // starts at 5% of a *narrower* plot -- nearer 7 than 10 -- and the last
    // ends well short of 189. The legend's own ink is high up rather than at
    // the middle row: its items stack from the top of the column it is given.
    let beside = (0..20)
        .any(|row| inked(stride, &right, row, true).iter().any(|x| *x > end));
    assert!(
        bars[0] < 10 && end < 189 && beside,
        "with the legend on the right the plot's ink runs {}..={end} and the \
         columns past it are {}, so the legend is not beside the plot",
        bars[0],
        if beside { "drawn" } else { "blank" }
    );
}

#[test]
fn the_four_sides_are_four_different_pictures() {
    // The control. Each assertion above reads one number, and four readings
    // that happened to agree with four positions would pass while the
    // position was ignored -- so this asserts the pictures themselves differ.
    let drawn: Vec<Vec<u8>> = [
        LegendPosition::Top,
        LegendPosition::Bottom,
        LegendPosition::Left,
        LegendPosition::Right,
    ]
    .into_iter()
    .map(|position| legended(position).1)
    .collect();
    for (index, one) in drawn.iter().enumerate() {
        for (other, two) in drawn.iter().enumerate().skip(index + 1) {
            assert_ne!(
                one, two,
                "positions {index} and {other} draw the same picture"
            );
        }
    }
}

#[test]
fn no_legend_leaves_the_plot_the_whole_page() {
    // And the other direction: a chart that asks for no legend is not one
    // that draws an empty one. Without this, every reading above could be
    // explained by a legend that is never drawn at all.
    let (stride, buffer) = pixels(two_bars(vec![1.0, 1.0]));
    let height = buffer.len() / (stride * 4);
    for row in [0, height - 1] {
        let count = inked(stride, &buffer, row, true).len();
        assert_eq!(
            count, 160,
            "row {row} holds {count} rather than two full-height bars, so \
             something took a side of the plot"
        );
    }
}

#[test]
fn every_kind_frames_its_own_legend() {
    // `framed` and `legend` are shared, so **the risk is not that they draw
    // wrongly but that a kind never calls them** -- which the tests above
    // cannot see, since they all ask a bar chart. Each kind is asked for a
    // legend and compared against itself without one.
    let with_legend = Options {
        show_legend: true,
        ..Options::default()
    };
    let slices = || {
        ["a".to_owned(), "b".to_owned()]
            .into_iter()
            .map(|label| Slice {
                label,
                value: 1.0,
                color: None,
            })
            .collect::<Vec<_>>()
    };
    let series = || {
        vec![Dataset {
            data: vec![1.0, 2.0],
            ..Dataset::default()
        }]
    };
    let labels = || ["a".to_owned(), "b".to_owned()];

    for (kind, plain, legended) in [
        (
            "pie",
            pie(&slices(), &Options::default()),
            pie(&slices(), &with_legend),
        ),
        (
            "doughnut",
            doughnut(
                &slices(),
                &Options {
                    inner_fraction: Some(0.5),
                    ..Options::default()
                },
            ),
            doughnut(
                &slices(),
                &Options {
                    inner_fraction: Some(0.5),
                    ..with_legend.clone()
                },
            ),
        ),
        (
            "line",
            line(&labels(), &series(), &Options::default()),
            line(&labels(), &series(), &with_legend),
        ),
    ] {
        let (_, plain) =
            pixels(plain.unwrap_or_else(|error| unreachable!("{error}")));
        let (_, legended) =
            pixels(legended.unwrap_or_else(|error| unreachable!("{error}")));
        assert_ne!(
            plain, legended,
            "a {kind} chart draws the same picture with a legend and without \
             one, so it never framed it"
        );
    }
}
