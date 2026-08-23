//! A pie or doughnut, drawn as stacked paths in one square space.
//!
//! **Every slice is drawn in the same square and stacked**, so each one's
//! viewBox is the whole drawing rather than its own bounds -- which is what
//! keeps them concentric. The square is why these two kinds needed nothing
//! more than a viewBox: v1's `radius = min(w, h) / 2` keeps a pie circular in
//! any box, and `xMidYMid meet` does the same thing. **A line chart is the one
//! that does not**, since it should fill its box rather than stay square.

#![expect(
    clippy::suboptimal_flops,
    reason = "a slice label's position is compared against the other \
              surface's byte for byte, and `mul_add` is fused -- one \
              rounding where JavaScript's `0.5 + Math.cos(m) * reach` has \
              two and no fused form to reach for. Clippy's `more \
              accurately` is true and it is the wrong property. THE LINT DID \
              NOT CHANGE, THE CODE'S OBLIGATIONS DID: this was ordinary \
              arithmetic until it acquired a second implementation, which \
              moved it from `paint.rs`'s category to `animate`'s without the \
              line being edited. And the tempting repair is the dangerous \
              one -- fusing makes `chart_agreement` fail, and regenerating \
              the assets would leave a green test asserting that a fused \
              Rust and an unfused JavaScript match. A re-pin is how a broken \
              agreement becomes a documented one."
)]

use meo_canvas_scene::{Length, node::PathPaint, style::effect::Transform};

use crate::{
    Box as BoxElement, Element, Error, Path, PositionType, Style, Text,
    chart::{
        bar::Options,
        frame::{framed, legend},
        geometry::{series_color, slice_angles},
    },
    fraction, hex_rgb, px,
    unit::sides,
};

/// The space a pie is drawn in, before it is scaled into its box.
const PIE_SPACE: f64 = 100.0;

/// How much of the radius v1's ten-pixel inset takes.
///
/// **A stated divergence.** v1 writes `radius = min(w, h) / 2 - 10`, ten
/// *pixels* regardless of size, which under a viewBox has no meaning -- the
/// drawing is authored once and scaled, so a pixel is not a fixed quantity
/// inside it. A proportion behaves better at both ends: v1's inset is a fifth
/// of the radius on a hundred-pixel chart and invisible on a thousand-pixel
/// one.
const PIE_INSET: f64 = 0.05;

/// How far along the radius v1 puts a slice's label: `radius * 0.7`.
const PIE_LABEL_REACH: f64 = 0.7;

/// v1 strokes every slice in white, which is what separates two slices of
/// similar colour.
const SLICE_STROKE: f32 = 2.0;

/// One wedge of a pie.
#[derive(Debug, Clone, Default)]
pub struct Slice {
    /// What the label and the legend call it.
    pub label: String,
    /// How much of the whole it is.
    pub value: f64,
    /// Its colour. Taken from the palette in order when absent.
    pub color: Option<String>,
}

/// A pie, or a doughnut when `inner_fraction` is above zero.
///
/// # Errors
///
/// Returns [`Error::Chart`] for a negative value, as the bar chart does and
/// for the same reason.
pub fn pie(
    slices: &[Slice],
    inner_fraction: f64,
    options: &Options,
) -> Result<Element, Error> {
    if slices.iter().any(|slice| slice.value < 0.0) {
        return Err(Error::Chart("a chart cannot draw a negative value"));
    }

    let outer = (PIE_SPACE / 2.0) * (1.0 - PIE_INSET);
    let inner = outer * inner_fraction;
    let values: Vec<f64> = slices.iter().map(|slice| slice.value).collect();
    let angles = slice_angles(&values);

    let mut drawn: Vec<Element> = Vec::new();
    for (index, (start, end)) in angles.iter().enumerate() {
        let colour = series_color(index, slices[index].color.as_deref());
        #[expect(
            clippy::cast_possible_truncation,
            reason = "the pie's own space, narrowed once at the style boundary"
        )]
        let space = PIE_SPACE as f32;
        drawn.push(
            Path::d(crate::chart::geometry::slice_path(
                *start, *end, outer, inner,
            ))
            .name(format!("slice {index}"))
            .view_box(Some((0.0, 0.0, space, space)))
            .fill(Some(PathPaint::Solid(
                meo_canvas_core::parse_color(&colour)
                    .unwrap_or(hex_rgb(0x00_00_00)),
            )))
            .stroke(Some(PathPaint::Solid(hex_rgb(0xff_ff_ff))))
            .line_width(SLICE_STROKE)
            .with_style(
                Style::new().position_type(PositionType::Absolute).position(
                    sides(
                        Some(px(0.0)),
                        Some(px(0.0)),
                        Some(px(0.0)),
                        Some(px(0.0)),
                    ),
                ),
            ),
        );
    }

    if options.show_labels {
        for (index, (start, end)) in angles.iter().enumerate() {
            drawn.push(slice_label(
                index,
                *start,
                *end,
                outer,
                &slices[index].label,
                options,
            ));
        }
    }

    Ok(framed(
        options,
        BoxElement::new()
            .name("plot")
            .with_style(
                Style::new()
                    .flex_grow(1.0)
                    .position_type(PositionType::Relative),
            )
            .children(drawn),
        legend(
            options,
            &slices
                .iter()
                .enumerate()
                .map(|(index, slice)| {
                    // v1 names a slice by its share as well as its label,
                    // which is the one place a legend entry is not just the
                    // series name.
                    (
                        format!("{} ({})", slice.label, slice.value),
                        series_color(index, slice.color.as_deref()),
                    )
                })
                .collect::<Vec<_>>(),
        ),
        if inner_fraction > 0.0 {
            "doughnut chart"
        } else {
            "pie chart"
        },
    ))
}

/// A slice's label, seven tenths of the way along its own middle angle.
///
/// **In percentages of the plot rather than pixels**, since the drawing is
/// square and centred and the label rides with it.
fn slice_label(
    index: usize,
    start: f64,
    end: f64,
    outer: f64,
    label: &str,
    options: &Options,
) -> Element {
    let middle = start + (end - start) / 2.0;
    let reach = (outer * PIE_LABEL_REACH) / PIE_SPACE;
    let (left, top) = (0.5 + middle.cos() * reach, 0.5 + middle.sin() * reach);
    BoxElement::new()
        .name(format!("slice label {index}"))
        .with_style(
            Style::new()
                .position_type(PositionType::Absolute)
                .position(sides(
                    Some(fraction(top)),
                    None,
                    None,
                    Some(fraction(left)),
                ))
                // **Half its own width and half its own height back from the
                // point**, which is v1's
                // `render(ctx, labelX - width / 2, labelY - height / 2)`.
                // Without it the label's top-left corner sits on the point and
                // the text hangs down and to the right of where it belongs.
                // The same shape as the axis label's missing transform, and it
                // survived longer here because no doc comment claimed it.
                .transform(Transform {
                    translate_x: Length::Percent(-0.5),
                    translate_y: Length::Percent(-0.5),
                    ..Transform::default()
                }),
        )
        .children([Text::new(label).with_style(
            Style::new()
                .font_size(options.label_font_size.unwrap_or(12.0))
                .color(options.label_color.unwrap_or(hex_rgb(0x00_00_00))),
        )])
}
