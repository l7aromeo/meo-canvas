//! A bar chart, built out of layout rather than draw calls.
//!
//! The plot is `flex-grow: 1` inside a column, so its height is whatever the
//! label strip and legend leave -- which is v1's `finalChartHeight` arrived at
//! by subtraction rather than by measurement. Bars are absolutely positioned
//! inside it in percentages, anchored to the bottom, which is what v1's
//! `barY = chartY + finalChartHeight - barHeight` says.

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
    /// The rule's colour. v1's `#e0e0e0` when absent.
    pub color: Option<String>,
}

/// What a label hatch is handed.
///
/// **A struct rather than positional arguments, because TypeScript's is a
/// named object** -- `{ item, index }` -- and matching it shape for shape is
/// what lets the two surfaces be read against each other. It also makes a
/// later field non-breaking on both sides.
#[derive(Debug)]
pub struct LabelItem<'a> {
    /// The label being drawn.
    pub item: &'a str,
    /// Which slot it sits in.
    pub index: usize,
}

/// What a value hatch is handed.
///
/// **The two `usize`s are the reason this is a struct and not a tuple.**
/// Positionally, `index` and `dataset_index` are adjacent and identically
/// typed, so a caller who swaps them gets no error, no warning and a chart
/// that is quietly wrong. v1 uses a named object at exactly this signature.
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
/// **TypeScript spells this `ChartDataset | PieChartDataPoint`, and Rust has no
/// untagged union.** So a TypeScript caller can write one function that ducks
/// across both and a Rust caller must match. That asymmetry is forced by the
/// languages rather than chosen here, and it cannot be removed.
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
/// **`Rc` rather than `Arc`, deliberately.** The only two spellings without a
/// `Send + Sync` bound on the closure are `Rc<dyn Fn>` and `Arc<dyn Fn>`, and
/// **they are equally un-`Send`** -- `Arc<T>: Send` requires `T: Send + Sync`
/// -- so `Arc` here would pay for atomics that nothing can use. Keeping
/// `Options: Send + Sync` would mean bounding every closure, which rejects one
/// capturing `Rc` data to buy a capability the scene cannot exercise: taffy's
/// tree is neither `Send` nor `Sync` and is built and consumed on one thread.
///
/// **The cost is real and named here rather than discovered**: `Options` was
/// `Send + Sync` before these fields and is not now.
///
/// `Rc` rather than `Box` because `Options` is `Clone`, and rather than a
/// borrow because a lifetime on `Options` would infect every caller and
/// anything that stores one.
pub type LabelHatch = Rc<dyn Fn(LabelItem<'_>) -> Option<Element>>;

/// Draws the value against a bar yourself. See [`LabelHatch`] for why `Rc`.
pub type ValueHatch = Rc<dyn Fn(ValueItem) -> Option<Element>>;

/// Draws one legend row yourself. See [`LabelHatch`] for why `Rc`.
pub type LegendHatch = Rc<dyn Fn(LegendItem<'_>) -> Option<Element>>;

/// Formats a category label before it is drawn, as v1 does, index included.
pub type XAxisFormatter = Rc<dyn Fn(&str, usize) -> String>;

/// Formats a y-axis value before it is drawn.
pub type YAxisFormatter = Rc<dyn Fn(f64) -> String>;

/// v1's `outerRadius * (innerRadius ?? 0.6)`, and the default both surfaces
/// now share.
pub const DEFAULT_INNER_FRACTION: f64 = 0.6;

/// What every chart understands, as v1 spells it.
///
/// **Four `show_` flags, because v1 and the TypeScript surface have four.**
/// Grouping them into an enum or a bitflag would make a caller port their
/// options object rather than spell it, and the two surfaces would then name
/// the same switch differently -- which is the thing the byte comparison is
/// there to catch.
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
    /// Which side the legend sits on. Below the plot when unset, as v1 does.
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
    ///
    /// **Moved here from a positional argument on
    /// [`crate::chart::pie::pie`]**, where TypeScript has always had it as an
    /// option. It defaulted to nothing on this surface and to `0.6` on the
    /// other, so a caller who said nothing got a pie here and a doughnut
    /// there -- and both agreement suites passed `0.6` explicitly, which is a
    /// test written around the gap rather than one that could see it.
    pub inner_fraction: Option<f64>,
    /// Draw the label under each slot yourself.
    ///
    /// The returned node is **placed** -- centred in the slot by ordinary
    /// layout -- rather than measured and drawn detached, which is v1's
    /// contract as the other surface has it.
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

/// v1's default gridline colour.
const GRID_COLOR: Color = crate::hex_rgb(0xe0_e0_e0);
/// The default for every piece of chart text.
const TEXT_COLOR: Color = crate::hex_rgb(0x00_00_00);
/// v1's default point size for chart text.
const TEXT_SIZE: f32 = 12.0;
/// v1 puts a value five pixels above its bar.
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
/// Returns [`Error::Chart`] for a negative value, which v1 mis-draws three
/// different ways rather than supporting.
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
            ));
        }
    }

    let mut body: Vec<Element> = vec![plot_area(options, max_value, bars)];
    if options.show_labels {
        body.push(label_strip(labels, options));
    }

    Ok(framed(
        options,
        // Flat setters rather than `with_style`, which replaces the whole
        // style and would discard the `flex-direction: column` that
        // `Column::new` just set -- laying the body and the label strip out
        // side by side. Caught by the cross-surface byte comparison, which is
        // the only one of the three checks that could see it.
        Column::new().name("body").flex_grow(1.0).children(body),
        legend(options, &series_labels(datasets)),
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
) -> Element {
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
                    meo_canvas_core::parse_color(&colour).unwrap_or(TEXT_COLOR),
                ),
        );
    if options.show_values {
        drawn = drawn.children([value_label(value, index, dataset, options)]);
    }
    drawn
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
            text(
                &format_number(value),
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
                // **The hatch is handed the RAW label and the formatter
                // feeds only the fallback text.** I wrote it the other way
                // round first -- format, then hand the formatted string to the
                // hatch -- which reads as the more sensible pipeline and is
                // not what the other surface does: `renderLabelItem?.({ item:
                // label, index })` takes `label`, and `shown` is computed
                // beside it for the `drawn ?? Text(shown)` fallback. A caller
                // supplying both gets the unformatted label here, and matching
                // that is the whole point of the comparison.
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
                            // **`justify_content` is the one that centres a
                            // label under its slot.** The strip is a row, so
                            // `align_items` is the cross axis and centres it
                            // vertically -- which is what both surfaces had,
                            // and it left every label against the left edge
                            // of its slot where v1 draws it centred. Neither
                            // the byte comparison nor a geometry row could
                            // see it: the two surfaces made the same mistake.
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
/// # How the gutter measures without measuring
///
/// Three properties are wanted at once: the gutter sizes to its widest label,
/// the labels centre on the gridlines, and the plot is a sibling so a bar
/// never covers the gutter. **Absolute children give the last two and do not
/// size their parent; in-flow children give the first and drift on the
/// second.** So the gutter holds both -- one zero-height in-flow copy of the
/// widest label, which sets the width and draws nothing, and the visible
/// labels absolutely positioned at their gridline fractions and pulled up by
/// half their own height.
pub(crate) fn plot_area(
    options: &Options,
    max_value: f64,
    bars: Vec<Element>,
) -> Element {
    let mut inside = grid(options);
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
        return plot;
    }

    // v1: `maxValue - (maxValue / 5) * i`, so the first row is the maximum
    // and the last is zero.
    let labels: Vec<String> = grid_lines(GRID_DIVISIONS)
        .into_iter()
        .map(|fraction| {
            let value = max_value - max_value * fraction;
            options
                .y_axis_label_formatter
                .as_ref()
                .map_or_else(|| format_number(value), |format| format(value))
        })
        .collect();
    // The widest by character count rather than by measurement, which is the
    // one thing a builder cannot do. A proportional face can make a shorter
    // string wider -- `111` against `00` -- so this is a heuristic, and the
    // sizer is why it only has to be close.
    // **The first of the widest, not the last.** `max_by_key` returns the
    // last maximum where the TypeScript side's scan keeps the first, and a
    // five-division axis ties constantly -- `1.6`, `1.2`, `0.8` and `0.4` are
    // all three characters. The two surfaces then size the gutter from
    // different strings, which a proportional face makes a different width.
    // Found by the byte comparison; no rendered check would have asked.
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
                        // centres on its gridline rather than hanging from
                        // it. The doc above always said this and the code did
                        // not -- the byte comparison is what noticed.
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

    Row::new().name("plot area").flex_grow(1.0).children([
        Column::new()
            .name("y axis")
            .position_type(PositionType::Relative)
            .children(gutter),
        plot,
    ])
}

/// The gridlines behind the plot, or nothing.
fn grid(options: &Options) -> Vec<Element> {
    if !options.grid.show {
        return Vec::new();
    }
    let colour = options
        .grid
        .color
        .as_deref()
        .and_then(meo_canvas_core::parse_color)
        .unwrap_or(GRID_COLOR);
    grid_lines(GRID_DIVISIONS)
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
        .collect()
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

/// A number as the other surface writes it: two decimals at most, and no
/// trailing zeros.
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
