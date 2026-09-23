//! The legend, and the frame that puts it on one of four sides.
//!
//! The legend wraps with `flex-wrap`, its height is whatever it wraps to, and
//! the plot takes the rest through `flex-grow` -- so nothing is measured to
//! size the plot.

use meo_canvas_scene::style::{Dimension, paint::Color};

use crate::{
    Align, Box as BoxElement, Column, Element, Error, FlexWrap, Row, Style,
    Styled, Text,
    chart::bar::{LegendEntry, LegendItem, Options},
    hex_rgb, pct, px,
    unit::sides,
};

/// A swatch's side in pixels: a square of the series colour.
const SWATCH: f32 = 15.0;
/// The gap between a swatch and its label, and between stacked items.
const GAP: f32 = 5.0;
/// The padding between one legend item and the next along a row.
const PADDING: f32 = 20.0;
/// The default for every piece of chart text.
const TEXT_COLOR: Color = hex_rgb(0x00_00_00);
/// Chart text's point size when none is given.
const TEXT_SIZE: f32 = 12.0;

/// Which side of the chart the legend sits on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LegendPosition {
    /// Above the plot.
    Top,
    /// Below the plot, and the default.
    #[default]
    Bottom,
    /// Left of the plot, stacked.
    Left,
    /// Right of the plot, stacked.
    Right,
}

impl LegendPosition {
    /// Whether the legend stacks rather than runs along: beside the plot it
    /// has a column's width to fill, where a row would push the plot out.
    const fn upright(self) -> bool {
        matches!(self, Self::Left | Self::Right)
    }
}

/// One swatch-and-label pair per series, or nothing.
///
/// `None` rather than an empty row, so [`framed`] does not hold an invisible
/// sibling that still takes a gap.
pub(crate) fn legend(
    options: &Options,
    entries: &[(String, String, LegendEntry<'_>)],
) -> Result<Option<Element>, Error> {
    if !options.show_legend || entries.is_empty() {
        return Ok(None);
    }
    let upright = options.legend_position.upright();
    let items: Vec<Element> = entries
        .iter()
        .enumerate()
        .map(
            |(index, (label, colour, source))| -> Result<Element, Error> {
                // A caller drawing the row themselves still gets the resolved
                // colour, because the palette fallback happened before this
                // point and they cannot recompute it.
                if let Some(draw) = options.render_legend_item.as_ref()
                    && let Some(drawn) = draw(LegendItem {
                        item: match source {
                            LegendEntry::Series(set) => {
                                LegendEntry::Series(set)
                            }
                            LegendEntry::Slice(slice) => {
                                LegendEntry::Slice(slice)
                            }
                        },
                        index,
                        color: colour,
                    })
                {
                    return Ok(drawn);
                }
                let spacing = if upright {
                    sides(
                        Dimension::Points(0.0),
                        Dimension::Points(0.0),
                        Dimension::Points(GAP),
                        Dimension::Points(0.0),
                    )
                } else {
                    sides(
                        Dimension::Points(0.0),
                        Dimension::Points(PADDING),
                        Dimension::Points(0.0),
                        Dimension::Points(0.0),
                    )
                };
                Ok(Row::new()
                    .name(format!("legend item {index}"))
                    .align_items(Align::Center)
                    .gap(px(GAP))
                    .margin(spacing)
                    .children([
                        BoxElement::new().name("swatch").with_style(
                            Style::new()
                                .width(px(SWATCH))
                                .height(px(SWATCH))
                                .background_color(
                                    meo_canvas_core::parse_color(colour)
                                        .ok_or(Error::Chart(
                                            "a legend colour could not be read",
                                        ))?,
                                ),
                        ),
                        Text::new(label).with_style(text_style(options)),
                    ]))
            },
        )
        .collect::<Result<Vec<Element>, Error>>()?;

    Ok(Some(if upright {
        Column::new().name("legend").children(items)
    } else {
        Row::new()
            .name("legend")
            .flex_wrap(FlexWrap::Wrap)
            .children(items)
    }))
}

/// The chart's own frame: the legend on whichever side, and everything else.
///
/// The legend is a sibling rather than an overlay, so the plot's `flex-grow`
/// takes what the legend leaves.
pub(crate) fn framed(
    options: &Options,
    body: Element,
    legend: Option<Element>,
    name: &'static str,
) -> Element {
    let Some(legend) = legend else {
        return Column::new()
            .name(name)
            .width(pct(100.0))
            .height(pct(100.0))
            .children([body]);
    };
    let children = match options.legend_position {
        LegendPosition::Top | LegendPosition::Left => vec![legend, body],
        LegendPosition::Bottom | LegendPosition::Right => vec![body, legend],
    };
    if options.legend_position.upright() {
        Row::new()
            .name(name)
            .width(pct(100.0))
            .height(pct(100.0))
            .children(children)
    } else {
        Column::new()
            .name(name)
            .width(pct(100.0))
            .height(pct(100.0))
            .children(children)
    }
}

/// A legend label in the chart's own family, size and colour.
fn text_style(options: &Options) -> Style {
    let mut style = Style::new()
        .font_size(options.label_font_size.unwrap_or(TEXT_SIZE))
        .color(options.label_color.unwrap_or(TEXT_COLOR));
    if let Some(family) = options.font_family.as_deref() {
        style = style.font_family(family);
    }
    style
}
