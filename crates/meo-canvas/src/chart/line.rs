//! A line chart: one stretched path per series, with markers beside it.
//!
//! **The plot fills its box rather than fitting into it**, which is the one
//! place a chart needs `preserveAspectRatio: none`. `meet` would letterbox a
//! hundred-by-hundred drawing into a wide plot and leave the series in a
//! square in the middle of it.
//!
//! # Why the markers are not in the path
//!
//! A circle authored in a stretched viewBox comes out an ellipse, by the ratio
//! of the two scales -- which on a 200x120 plot is not subtle. **The markers
//! are round boxes placed in percentages instead**, and a percentage of the
//! plot is measured after the stretch rather than through it.

use meo_canvas_scene::{Length, node::PathPaint, style::effect::Transform};

use crate::{
    Box as BoxElement, Column, Element, Error, Path, PositionType, Style,
    Styled,
    chart::{
        bar::{Dataset, Options, label_strip, plot_area, series_labels},
        frame::{framed, legend},
        geometry::{LINE_SPACE, line_path, line_points, series_color},
    },
    fraction, hex_rgb, px,
    unit::sides,
};

/// v1's `ctx.lineWidth = 2` for a line series.
const SERIES_STROKE: f32 = 2.0;

/// v1's point marker: `ctx.arc(pointX, pointY, 4, 0, Math.PI * 2)`.
const POINT_RADIUS: f32 = 4.0;

/// A line chart of `labels` against `datasets`.
///
/// # Errors
///
/// Returns [`Error::Chart`] for a negative value, as the other kinds do.
pub fn line(
    labels: &[String],
    datasets: &[Dataset],
    options: &Options,
) -> Result<Element, Error> {
    if datasets.iter().flat_map(|set| &set.data).any(|v| *v < 0.0) {
        return Err(Error::Chart("a chart cannot draw a negative value"));
    }
    let max_value = datasets
        .iter()
        .flat_map(|set| &set.data)
        .copied()
        .fold(0.0_f64, f64::max);

    let mut inside: Vec<Element> = Vec::new();
    for (index, dataset) in datasets.iter().enumerate() {
        inside.push(series(index, dataset, labels.len(), max_value));
    }
    // Markers after every series, so a line never covers a point of a series
    // drawn before it -- v1 draws them per dataset and interleaves them, which
    // differs only where two series cross.
    for (index, dataset) in datasets.iter().enumerate() {
        let colour = series_color(index, dataset.color.as_deref());
        for (at, point) in line_points(labels.len(), &dataset.data, max_value)
            .iter()
            .enumerate()
        {
            inside.push(marker(index, at, point.x, point.y, &colour));
        }
    }

    let mut body: Vec<Element> = vec![plot_area(options, max_value, inside)];
    if options.show_labels {
        body.push(label_strip(labels, options));
    }

    Ok(framed(
        options,
        Column::new().name("body").flex_grow(1.0).children(body),
        legend(options, &series_labels(datasets)),
        "line chart",
    ))
}

/// One series as a path stretched over the whole plot.
fn series(
    index: usize,
    dataset: &Dataset,
    labels: usize,
    max_value: f64,
) -> Element {
    let colour = series_color(index, dataset.color.as_deref());
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the plot's own space, narrowed once at the style boundary"
    )]
    let space = LINE_SPACE as f32;
    Path::d(line_path(&line_points(labels, &dataset.data, max_value)))
        .name(format!("series {index}"))
        .view_box(Some((0.0, 0.0, space, space)))
        .stretch(true)
        .fill(None)
        .stroke(Some(PathPaint::Solid(
            meo_canvas_core::parse_color(&colour)
                .unwrap_or(hex_rgb(0x00_00_00)),
        )))
        .line_width(SERIES_STROKE)
        .with_style(
            Style::new()
                .position_type(PositionType::Absolute)
                .position(sides(
                    Some(px(0.0)),
                    Some(px(0.0)),
                    Some(px(0.0)),
                    Some(px(0.0)),
                )),
        )
}

/// One point marker, centred on its point by a half-its-own-size translation.
fn marker(
    series: usize,
    index: usize,
    x: f64,
    y: f64,
    colour: &str,
) -> Element {
    BoxElement::new()
        .name(format!("point {series}.{index}"))
        .with_style(
            Style::new()
                .position_type(PositionType::Absolute)
                .position(sides(
                    Some(fraction(y)),
                    None,
                    None,
                    Some(fraction(x)),
                ))
                .width(px(POINT_RADIUS * 2.0))
                .height(px(POINT_RADIUS * 2.0))
                .border_radius(POINT_RADIUS)
                .transform(Transform {
                    translate_x: Length::Percent(-0.5),
                    translate_y: Length::Percent(-0.5),
                    ..Transform::default()
                })
                .background_color(
                    meo_canvas_core::parse_color(colour)
                        .unwrap_or(hex_rgb(0x00_00_00)),
                ),
        )
}
