//! A bar chart, built out of layout rather than draw calls.
//!
//! The plot is `flex-grow: 1` inside a column, so its height is whatever the
//! label strip and legend leave. Bars are absolutely positioned inside it in
//! percentages, anchored to the bottom.

#![expect(
    clippy::suboptimal_flops,
    reason = "the y-axis label values become strings that the other surface \
              also computes, so a fused multiply-add -- rounding once where \
              JavaScript rounds twice -- could change a label by a digit. \
              Agreement is the objective here, as in `chart::geometry`."
)]

use std::rc::Rc;

use meo_canvas_scene::{
    Length,
    style::{Dimension, effect::Transform, paint::Color},
};

use crate::{
    Align, Box as BoxElement, Column, Element, Error, Justify, Overflow,
    PositionType, Row, Style, Styled, Text,
    chart::{
        frame::{LegendPosition, framed, legend},
        geometry::{Bar, GRID_DIVISIONS, bar_layout, grid_lines, series_color},
    },
    fraction, pct, px,
    unit::sides,
};

/// One series of a cartesian chart.
#[derive(Debug, Clone, Default)]
pub struct Dataset {
    /// What the legend calls it. `Series 1`, `Series 2` and so on when absent.
    pub label: Option<String>,
    /// The series colour. Taken from the palette in order when absent.
    pub color: Option<String>,
    /// The values, one per label.
    pub data: Vec<f64>,
}

/// Whether a grid is drawn behind the plot, and in what colour.
#[derive(Debug, Clone, Default)]
pub struct Grid {
    /// Whether to draw it at all.
    pub show: bool,
    /// The rule's colour. `#e0e0e0` when absent.
    pub color: Option<String>,
}

/// What a label hatch is handed.
///
/// A struct rather than positional arguments: it reads field for field
/// against the other surface's `{ item, index }`, and a later field is
/// non-breaking on both.
#[derive(Debug)]
pub struct LabelItem<'a> {
    /// The label being drawn.
    pub item: &'a str,
    /// Which slot it sits in.
    pub index: usize,
}

/// What a value hatch is handed.
///
/// A struct rather than a tuple because `index` and `dataset_index` are
/// adjacent `usize`s: swapped positionally, they compile and draw a quietly
/// wrong chart.
#[derive(Debug)]
pub struct ValueItem {
    /// The value being drawn.
    pub item: f64,
    /// Which slot along the axis it belongs to.
    pub index: usize,
    /// Which series it belongs to.
    pub dataset_index: usize,
}

/// The thing a legend row stands for.
///
/// The other surface's `ChartDataset | PieChartDataPoint`. Rust has no
/// untagged union, so a caller here matches where a TypeScript caller can
/// write one function across both.
#[derive(Debug)]
pub enum LegendEntry<'a> {
    /// A cartesian chart's series.
    Series(&'a Dataset),
    /// A pie or doughnut's slice.
    Slice(&'a crate::chart::pie::Slice),
}

/// What a legend hatch is handed.
#[derive(Debug)]
pub struct LegendItem<'a> {
    /// The series or slice this row stands for.
    pub item: LegendEntry<'a>,
    /// Its position in the legend.
    pub index: usize,
    /// The colour drawn in its swatch, resolved from the palette if the
    /// caller gave none.
    pub color: &'a str,
}

/// Draws the label under a slot yourself.
///
/// **`Rc` rather than `Arc`.** An unbounded `dyn Fn` is un-`Send` behind
/// either -- `Arc<T>: Send` needs `T: Send + Sync` -- so `Arc` would pay for
/// atomics nothing uses. Bounding every closure `Send + Sync` would reject one
/// capturing `Rc` data, for a capability the scene cannot use: taffy's tree is
/// built and consumed on one thread. **The cost is that `Options` is not
/// `Send + Sync`.**
///
/// `Rc` rather than `Box` because `Options` is `Clone`, and rather than a
/// borrow because a lifetime on `Options` would reach every caller.
pub type LabelHatch = Rc<dyn Fn(LabelItem<'_>) -> Option<Element>>;

/// Draws the value against a bar yourself. See [`LabelHatch`] for why `Rc`.
pub type ValueHatch = Rc<dyn Fn(ValueItem) -> Option<Element>>;

/// Draws one legend row yourself. See [`LabelHatch`] for why `Rc`.
pub type LegendHatch = Rc<dyn Fn(LegendItem<'_>) -> Option<Element>>;

/// Formats a category label before it is drawn, given the label's index too.
pub type XAxisFormatter = Rc<dyn Fn(&str, usize) -> String>;

/// Formats a y-axis value before it is drawn.
pub type YAxisFormatter = Rc<dyn Fn(f64) -> String>;

/// A doughnut's hole when [`Options::inner_fraction`] is unset, as a fraction
/// of its outer radius.
pub const DEFAULT_INNER_FRACTION: f64 = 0.6;

/// What every chart understands.
///
/// Four `show_` flags rather than an enum or a bitflag, so this reads field
/// for field against the other surface's option bag and the two name each
/// switch the same way.
#[derive(Clone, Default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "an option bag mirroring the other surface's, one field per \
              switch it has"
)]
pub struct Options {
    /// Draw the strip of labels under the plot.
    pub show_labels: bool,
    /// Draw each value above its bar.
    pub show_values: bool,
    /// Draw the y-axis gutter.
    pub show_y_axis: bool,
    /// Draw the legend.
    pub show_legend: bool,
    /// Which side the legend sits on. Below the plot when unset.
    pub legend_position: LegendPosition,
    /// The grid behind the plot.
    pub grid: Grid,
    /// Point size for the labels under the plot.
    pub label_font_size: Option<f32>,
    /// Point size for the values above the bars.
    pub value_font_size: Option<f32>,
    /// Point size for the y-axis labels.
    pub y_axis_font_size: Option<f32>,
    /// Colour for the labels under the plot.
    pub label_color: Option<Color>,
    /// Colour for the values above the bars.
    pub value_color: Option<Color>,
    /// Colour for the y-axis labels, falling back to `axis_color`.
    pub y_axis_color: Option<Color>,
    /// Colour for axis text generally.
    pub axis_color: Option<Color>,
    /// The family every piece of chart text is set in.
    pub font_family: Option<String>,
    /// A doughnut's hole, as a fraction of its outer radius.
    /// [`DEFAULT_INNER_FRACTION`] when unset.
    pub inner_fraction: Option<f64>,
    /// Draw the label under each slot yourself.
    ///
    /// The returned node is **placed** -- centred in the slot by ordinary
    /// layout -- rather than measured and drawn detached.
    pub render_label_item: Option<LabelHatch>,
    /// Draw the value against each bar yourself. Placed, as above.
    pub render_value_item: Option<ValueHatch>,
    /// Draw each legend row yourself. Placed, as above.
    pub render_legend_item: Option<LegendHatch>,
    /// Format a category label before it is drawn.
    pub x_axis_label_formatter: Option<XAxisFormatter>,
    /// Format a y-axis value before it is drawn.
    pub y_axis_label_formatter: Option<YAxisFormatter>,
}

/// Written by hand because a closure has no useful `Debug`.
///
/// **Presence rather than identity**: what a reader can act on is whether a
/// hatch is set, and nothing can be printed about which one it is.
impl core::fmt::Debug for Options {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        /// `Some(..)` or `None`, without claiming to know what the closure is.
        const fn hatch<T>(value: Option<&T>) -> &'static str {
            if value.is_some() {
                "Some(<fn>)"
            } else {
                "None"
            }
        }
        f.debug_struct("Options")
            .field("show_labels", &self.show_labels)
            .field("show_values", &self.show_values)
            .field("show_y_axis", &self.show_y_axis)
            .field("show_legend", &self.show_legend)
            .field("legend_position", &self.legend_position)
            .field("grid", &self.grid)
            .field("label_font_size", &self.label_font_size)
            .field("value_font_size", &self.value_font_size)
            .field("y_axis_font_size", &self.y_axis_font_size)
            .field("label_color", &self.label_color)
            .field("value_color", &self.value_color)
            .field("y_axis_color", &self.y_axis_color)
            .field("axis_color", &self.axis_color)
            .field("font_family", &self.font_family)
            .field("inner_fraction", &self.inner_fraction)
            .field("render_label_item", &hatch(self.render_label_item.as_ref()))
            .field("render_value_item", &hatch(self.render_value_item.as_ref()))
            .field(
                "render_legend_item",
                &hatch(self.render_legend_item.as_ref()),
            )
            .field(
                "x_axis_label_formatter",
                &hatch(self.x_axis_label_formatter.as_ref()),
            )
            .field(
                "y_axis_label_formatter",
                &hatch(self.y_axis_label_formatter.as_ref()),
            )
            .finish()
    }
}

/// The gridline colour when none is given.
const GRID_COLOR: Color = crate::hex_rgb(0xe0_e0_e0);
/// The default for every piece of chart text.
const TEXT_COLOR: Color = crate::hex_rgb(0x00_00_00);
/// Chart text's point size when none is given.
const TEXT_SIZE: f32 = 12.0;
/// The gap between a bar's top and its value label, in pixels.
const VALUE_LIFT: f32 = 5.0;

/// A bar chart of `labels` against `datasets`.
///
/// # A dataset that is not as long as the labels
///
/// **The label count decides how many bars there are.** A dataset with more
/// values than there are labels has the extra ones **dropped**; one with fewer
/// draws a **zero-height** bar in the empty slot. Neither is refused, because
/// neither mis-draws: a caller sees the mismatch rather than a chart that lies
/// about it, which is what separates this from a negative value.
///
/// **This is not what [`crate::chart::line::line`] does with the same input.**
/// A line chart iterates the data rather than the labels, so its extra points
/// are drawn past the right edge instead of dropped.
///
/// # Errors
///
/// Returns [`Error::Chart`] for a negative value, which this geometry cannot
/// draw, and for a colour that cannot be read -- on a dataset or on the grid.
pub fn bar(
    labels: &[String],
    datasets: &[Dataset],
    options: &Options,
) -> Result<Element, Error> {
    let values: Vec<Vec<f64>> =
        datasets.iter().map(|set| set.data.clone()).collect();
    let max_value = values.iter().flatten().copied().fold(0.0_f64, f64::max);
    let placed = bar_layout(labels.len(), datasets.len(), &values, max_value)?;

    let mut bars: Vec<Element> = Vec::new();
    for (index, group) in placed.iter().enumerate() {
        for (dataset, bar) in group.iter().enumerate() {
            bars.push(one_bar(
                *bar, index, dataset, &values, datasets, options,
            )?);
        }
    }

    let mut body: Vec<Element> = vec![plot_area(options, max_value, bars)?];
    if options.show_labels {
        body.push(label_strip(labels, options));
    }

    Ok(framed(
        options,
        // The column direction is pinned by
        // `the_label_strip_sits_under_the_plot_rather_than_beside_it`.
        Column::new().name("body").flex_grow(1.0).children(body),
        legend(options, &series_labels(datasets))?,
        "bar chart",
    ))
}

/// One bar, placed by percentage and anchored to the plot's floor.
fn one_bar(
    bar: Bar,
    index: usize,
    dataset: usize,
    values: &[Vec<f64>],
    datasets: &[Dataset],
    options: &Options,
) -> Result<Element, Error> {
    let colour = series_color(dataset, datasets[dataset].color.as_deref());
    let value = values
        .get(dataset)
        .and_then(|row| row.get(index))
        .copied()
        .unwrap_or(0.0);

    let mut drawn = BoxElement::new()
        .name(format!("bar {index}.{dataset}"))
        .with_style(
            Style::new()
                .position_type(PositionType::Absolute)
                .position(sides(
                    None,
                    None,
                    Some(px(0.0)),
                    Some(fraction(bar.x)),
                ))
                .width(fraction(bar.width))
                .height(fraction(bar.height))
                .background_color(
                    meo_canvas_core::parse_color(&colour).ok_or(
                        Error::Chart("a bar colour could not be read"),
                    )?,
                ),
        );
    if options.show_values {
        drawn = drawn.children([value_label(value, index, dataset, options)]);
    }
    Ok(drawn)
}

/// A value sitting five pixels above its bar, centred on it.
fn value_label(
    value: f64,
    index: usize,
    dataset_index: usize,
    options: &Options,
) -> Element {
    let drawn = options.render_value_item.as_ref().and_then(|draw| {
        draw(ValueItem {
            item: value,
            index,
            dataset_index,
        })
    });
    BoxElement::new()
        .with_style(
            Style::new()
                .position_type(PositionType::Absolute)
                .position(sides(
                    None,
                    Some(px(0.0)),
                    Some(pct(100.0)),
                    Some(px(0.0)),
                ))
                .margin(sides(
                    Dimension::Points(0.0),
                    Dimension::Points(0.0),
                    Dimension::Points(VALUE_LIFT),
                    Dimension::Points(0.0),
                ))
                .align_items(Align::Center),
        )
        .children([drawn.unwrap_or_else(|| {
            // As written, not rounded: `format_number` is the y axis's, and
            // `2.35` for a bar of `2.345` is a number the caller never gave.
            // From 1e21 JavaScript switches to exponential and this does not;
            // `a_value_label_at_1e21_is_spelled_differently_on_each_surface`.
            text(
                &value.to_string(),
                options,
                options.value_font_size,
                options.value_color,
            )
        })])
}

/// The strip of labels under the plot, one equal share each.
pub(crate) fn label_strip(labels: &[String], options: &Options) -> Element {
    Row::new().name("labels").children(
        labels
            .iter()
            .enumerate()
            .map(|(index, label)| {
                // The hatch gets the raw label; the formatter feeds only the
                // fallback text, as on the other surface.
                let drawn = options
                    .render_label_item
                    .as_ref()
                    .and_then(|draw| draw(LabelItem { item: label, index }));
                let shown =
                    options.x_axis_label_formatter.as_ref().map_or_else(
                        || label.clone(),
                        |format| format(label, index),
                    );
                BoxElement::new()
                    .with_style(
                        Style::new()
                            .flex_grow(1.0)
                            .flex_basis(Dimension::Points(0.0))
                            // `justify_content` centres a label under its
                            // slot; in a row, `align_items` is the vertical.
                            .justify_content(Justify::Center)
                            .align_items(Align::Center),
                    )
                    .children([drawn.unwrap_or_else(|| {
                        text(
                            &shown,
                            options,
                            options.label_font_size,
                            options.label_color,
                        )
                    })])
            })
            .collect::<Vec<_>>(),
    )
}

/// The plot, with a y-axis gutter beside it when one is asked for.
///
/// Absolute labels centre on their gridlines but do not size the gutter, so a
/// zero-height in-flow copy of the widest label sets its width.
pub(crate) fn plot_area(
    options: &Options,
    max_value: f64,
    bars: Vec<Element>,
) -> Result<Element, Error> {
    let mut inside = grid(options)?;
    inside.extend(bars);
    let plot = BoxElement::new()
        .name("plot")
        .with_style(
            Style::new()
                .flex_grow(1.0)
                .position_type(PositionType::Relative),
        )
        .children(inside);

    if !options.show_y_axis {
        return Ok(plot);
    }

    // The first row is the maximum and the last is zero.
    let labels: Vec<String> = grid_lines(GRID_DIVISIONS)
        .into_iter()
        .map(|fraction| {
            let value = max_value - max_value * fraction;
            // Rounded, as the other surface's default is. The two spell the
            // number differently from 1e22, a decade after the value path:
            // `a_y_axis_label_at_1e22_is_spelled_differently_on_each_surface`.
            options
                .y_axis_label_formatter
                .as_ref()
                .map_or_else(|| format_number(value), |format| format(value))
        })
        .collect();
    // Widest by character count, since a builder cannot measure, and the sizer
    // only needs it close. The first of the widest rather than `max_by_key`'s
    // last: a five-division axis ties constantly, and the other surface keeps
    // the first, so the gutter is sized from the same string.
    let widest = labels
        .iter()
        .fold(None::<&String>, |best, label| match best {
            Some(held) if held.chars().count() >= label.chars().count() => {
                Some(held)
            }
            _ => Some(label),
        })
        .cloned()
        .unwrap_or_default();
    let colour = options.y_axis_color.or(options.axis_color);

    let mut gutter: Vec<Element> = vec![
        BoxElement::new()
            .name("gutter sizer")
            .with_style(Style::new().height(px(0.0)).overflow(Overflow::Hidden))
            .children([text(
                &widest,
                options,
                options.y_axis_font_size,
                colour,
            )]),
    ];
    for (index, label) in labels.iter().enumerate() {
        let top = grid_lines(GRID_DIVISIONS)[index];
        gutter.push(
            BoxElement::new()
                .name(format!("axis label {index}"))
                .with_style(
                    Style::new()
                        .position_type(PositionType::Absolute)
                        .position(sides(
                            Some(fraction(top)),
                            None,
                            None,
                            Some(px(0.0)),
                        ))
                        // Pulled up by half its own height, so the label
                        // centres on its gridline rather than hanging from it.
                        .transform(Transform {
                            translate_y: Length::Percent(-0.5),
                            ..Transform::default()
                        }),
                )
                .children([text(
                    label,
                    options,
                    options.y_axis_font_size,
                    colour,
                )]),
        );
    }

    Ok(Row::new().name("plot area").flex_grow(1.0).children([
        Column::new()
            .name("y axis")
            .position_type(PositionType::Relative)
            .children(gutter),
        plot,
    ]))
}

/// The gridlines behind the plot, or nothing.
fn grid(options: &Options) -> Result<Vec<Element>, Error> {
    if !options.grid.show {
        return Ok(Vec::new());
    }
    // Absent takes the default; unreadable is refused rather than drawn in a
    // colour the caller did not ask for.
    let colour = match options.grid.color.as_deref() {
        None => GRID_COLOR,
        Some(written) => meo_canvas_core::parse_color(written)
            .ok_or(Error::Chart("the grid colour could not be read"))?,
    };
    Ok(grid_lines(GRID_DIVISIONS)
        .into_iter()
        .map(|at| {
            BoxElement::new().name(format!("gridline {at}")).with_style(
                Style::new()
                    .position_type(PositionType::Absolute)
                    .position(sides(
                        Some(fraction(at)),
                        Some(px(0.0)),
                        None,
                        Some(px(0.0)),
                    ))
                    .height(px(1.0))
                    .background_color(colour),
            )
        })
        .collect())
}

/// A piece of chart text in the chart's own family, size and colour.
pub(crate) fn text(
    content: &str,
    options: &Options,
    size: Option<f32>,
    colour: Option<Color>,
) -> Element {
    let mut style = Style::new()
        .font_size(size.unwrap_or(TEXT_SIZE))
        .color(colour.unwrap_or(TEXT_COLOR));
    if let Some(family) = options.font_family.as_deref() {
        style = style.font_family(family);
    }
    Text::new(content).with_style(style)
}

/// A number to two decimals at most, with no trailing zeros.
fn format_number(value: f64) -> String {
    let rounded = (value * 100.0).round() / 100.0;
    let text = format!("{rounded}");
    text
}

/// What the legend calls each series, and in what colour.
///
/// **Shared by the two cartesian kinds**, which name their series the same
/// way: `Series 1`, `Series 2` and so on where a dataset gives no label.
pub(crate) fn series_labels(
    datasets: &[Dataset],
) -> Vec<(String, String, LegendEntry<'_>)> {
    datasets
        .iter()
        .enumerate()
        .map(|(index, set)| {
            (
                set.label
                    .clone()
                    .unwrap_or_else(|| format!("Series {}", index + 1)),
                series_color(index, set.color.as_deref()),
                // The row carries what it stands for, so a legend hatch can be
                // handed the series rather than the two strings drawn from it.
                LegendEntry::Series(set),
            )
        })
        .collect()
}
