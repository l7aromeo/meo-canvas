//! The legend, and the frame that puts it on one of four sides.
//!
//! **v1 measures every label and wraps by hand** -- `currentX + itemWidth >
//! totalWidth` and a running row count -- so that it can subtract the
//! legend's height from the plot's. A builder needs none of that: wrapping is
//! `flex-wrap`, the row's height is whatever it wraps to, and the plot takes
//! the rest through `flex-grow`. **The measuring v1 does in order to size the
//! plot is the thing layout was going to do anyway.**

use meo_canvas_scene::style::{Dimension, paint::Color};

use crate::{
    Align, Box as BoxElement, Column, Element, FlexWrap, Row, Style, Styled,
    Text, chart::bar::Options, hex_rgb, pct, px, unit::sides,
};

/// v1's swatch: a fifteen-pixel square of the series colour.
const SWATCH: f32 = 15.0;
/// v1's gap between a swatch and its label, and between stacked items.
const GAP: f32 = 5.0;
/// v1's padding between one legend item and the next along a row.
const PADDING: f32 = 20.0;
/// The default for every piece of chart text.
const TEXT_COLOR: Color = hex_rgb(0x00_00_00);
/// v1's default point size for chart text.
const TEXT_SIZE: f32 = 12.0;

/// Which side of the chart the legend sits on, as v1 spells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LegendPosition {
    /// Above the plot.
    Top,
    /// Below the plot, which is v1's default.
    #[default]
    Bottom,
    /// Left of the plot, stacked.
    Left,
    /// Right of the plot, stacked.
    Right,
}

impl LegendPosition {
    /// Whether the legend stacks rather than runs along.
    ///
    /// **The side decides the legend's own direction, not just where it
    /// goes.** A legend beside the plot has a column's width to fill and a
    /// row's would push the plot out; one above or below has the width and
    /// wraps within it.
    const fn upright(self) -> bool {
        matches!(self, Self::Left | Self::Right)
    }
}

/// One swatch-and-label pair per series, or nothing.
///
/// `None` rather than an empty row when there is nothing to show, so
/// [`framed`] puts the chart in a plain column instead of one holding an
/// invisible sibling that still takes a gap.
pub(crate) fn legend(
    options: &Options,
    entries: &[(String, String)],
) -> Option<Element> {
    if !options.show_legend || entries.is_empty() {
        return None;
    }
    let upright = options.legend_position.upright();
    let items: Vec<Element> = entries
        .iter()
        .enumerate()
        .map(|(index, (label, colour))| {
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
            Row::new()
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
                                    .unwrap_or(TEXT_COLOR),
                            ),
                    ),
                    Text::new(label).with_style(text_style(options)),
                ])
        })
        .collect();

    Some(if upright {
        Column::new().name("legend").children(items)
    } else {
        Row::new()
            .name("legend")
            .flex_wrap(FlexWrap::Wrap)
            .children(items)
    })
}

/// The chart's own frame: the legend on whichever side, and everything else.
///
/// **The legend is a sibling rather than an overlay**, so the plot's
/// `flex-grow` takes what the legend leaves -- which is v1's
/// `chartHeight - legendHeight` arrived at by layout instead of by
/// subtraction.
pub(crate) fn framed(
    options: &Options,
    body: Element,
    legend: Option<Element>,
    name: &'static str,
) -> Element {
    // Flat setters after the constructor, since `with_style` would replace
    // the direction it just set -- see `Element::with_style`.
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
