//! The same chart, built on both surfaces, compared as bytes.
//!
//! # Why two implementations rather than one and a reference
//!
//! **`Chart` has no external adjudicator.** Chrome has no charts, v1 is both
//! baselines, and the arithmetic is the specification. So the strongest check
//! available is that two independent implementations produce the same scene —
//! and where they differ, one is wrong and the comparison says so **without
//! either being trusted.**
//!
//! # What this closes, and what it does not
//!
//! **It closes the port and not the geometry.** Both surfaces agreeing on a
//! wrong bar edge passes every byte here. The numbers are guarded by rendering
//! — `chart.render.test.ts` on the TypeScript side and its equivalent here —
//! which check the arithmetic against pixels rather than against itself.
//! Three checks, three questions, and none substitutes for another.
//!
//! # Why the bytes are committed rather than generated
//!
//! `ci` runs these tests **before** the JavaScript ones, so a suite that wrote
//! the asset would leave this comparing against the previous run's output —
//! the stale-artifact trap with the staleness manufactured by the suite. The
//! bytes are committed; both sides assert against them; a deliberate change is
//! `UPDATE_CHART_BYTES=1 npx vitest run chart.agreement`.

use std::rc::Rc;

use meo_canvas::{
    Box as BoxElement, Element, Root, Style,
    chart::{
        bar::{Dataset, Grid, LabelItem, LegendItem, Options, ValueItem, bar},
        frame::LegendPosition,
        line::line,
        pie::{Slice, doughnut, pie},
    },
    hex_rgb, px,
    scene::codec,
};

/// The bytes the TypeScript surface writes for the same chart.
const THEIRS: &str = include_str!("assets/chart/bar-bytes.txt");

/// The chart both surfaces build.
///
/// **Every option switched on**, because an option left at its default is one
/// the comparison never sees: two implementations agree trivially about a
/// branch neither takes.
fn ours() -> Vec<u8> {
    let labels = ["a".to_owned(), "b".to_owned()];
    let datasets = [
        Dataset {
            label: Some("Sales".to_owned()),
            color: Some("#3366cc".to_owned()),
            data: vec![1.0, 2.0],
        },
        Dataset {
            label: None,
            color: None,
            data: vec![2.0, 1.0],
        },
    ];
    let options = Options {
        show_labels: true,
        show_values: true,
        show_y_axis: true,
        grid: Grid {
            show: true,
            color: Some("#e0e0e0".to_owned()),
        },
        label_font_size: Some(11.0),
        value_font_size: Some(10.0),
        y_axis_font_size: Some(9.0),
        label_color: Some(hex_rgb(0x11_22_33)),
        value_color: Some(hex_rgb(0x44_55_66)),
        y_axis_color: Some(hex_rgb(0x77_88_99)),
        font_family: Some("Fixture".to_owned()),
        ..Options::default()
    };

    let chart = bar(&labels, &datasets, &options).unwrap_or_else(|error| {
        unreachable!("the chart did not build: {error}")
    });
    let scene = Root::new(200.0)
        .height(120.0)
        .children(chart)
        .into_scene()
        .unwrap_or_else(|error| {
            unreachable!("the scene did not assemble: {error}")
        });
    codec::encode(&scene)
}

/// Where the chart's own node begins, by its name in the byte stream.
///
/// **The page frame is not part of the comparison.** `Root::new` here and a
/// page root handed to `encodeScene` there are different framings with
/// different default styles, and their disagreement is about the harness
/// rather than about either chart. Everything from the `bar chart` node
/// onward is the chart.
fn from_the_chart<'a>(bytes: &'a [u8], name: &str) -> &'a [u8] {
    let needle = name.as_bytes();
    let at = bytes
        .windows(needle.len())
        .position(|window| window == needle)
        .unwrap_or_else(|| unreachable!("the scene has no `{name}` node"));
    &bytes[at..]
}

#[test]
fn both_surfaces_encode_the_same_chart_to_the_same_bytes() {
    let theirs = THEIRS.trim();
    assert!(
        !theirs.is_empty(),
        "the committed bytes are empty, so this would compare against nothing"
    );

    let encoded = ours();
    let ours = hex(from_the_chart(&encoded, "bar chart"));
    let theirs = hex(from_the_chart(
        &(0..theirs.len() / 2)
            .map(|index| {
                u8::from_str_radix(&theirs[index * 2..index * 2 + 2], 16)
                    .unwrap_or_else(|error| {
                        unreachable!("the asset is not hex: {error}")
                    })
            })
            .collect::<Vec<u8>>(),
        "bar chart",
    ));
    assert_eq!(
        ours, theirs,
        "the two chart implementations disagree. One of them is wrong and this \
         comparison cannot say which -- read `chart.render.test.ts` and the \
         rendered checks here, which measure the geometry rather than the \
         agreement"
    );
}

/// Hex, because it has no tail cases to get right.
///
/// This was base64 first, and the pad arithmetic was correct **by a
/// coincidence between `<=` and the tail length** rather than by saying so —
/// and which tail a chart ever exercises depends on `codec::encode`'s output
/// length modulo three, so today's chart might only ever reach one of them.
/// A case that cannot discriminate is not a case that agreed. Hex has one
/// rule and no remainder.
fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from(DIGITS[usize::from(byte >> 4)]));
        out.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    out
}

/// The bytes the TypeScript surface writes for each of the other three kinds.
const THEIR_LINE: &str = include_str!("assets/chart/line-bytes.txt");
/// As [`THEIR_LINE`], for the pie.
const THEIR_PIE: &str = include_str!("assets/chart/pie-bytes.txt");
/// As [`THEIR_LINE`], for the doughnut.
const THEIR_DOUGHNUT: &str = include_str!("assets/chart/doughnut-bytes.txt");

/// The same line chart with the legend on the right, which is the one frame
/// branch no other case reaches.
/// The five function-valued options, whose effect is what gets compared.
const THEIR_HATCHES: &str = include_str!("assets/chart/hatches-bytes.txt");

const THEIR_LINE_RIGHT: &str =
    include_str!("assets/chart/line-legend-right-bytes.txt");

/// Every option switched on, and a legend on a stated side.
///
/// **A default is a branch neither surface takes**, so a case that leaves one
/// alone has the two agreeing about nothing. The legend position differs per
/// case because it is the one option that changes the chart's *root* node from
/// a column to a row — a branch the bar case cannot reach, since it can only
/// take one side at a time.
fn everything(position: LegendPosition) -> Options {
    Options {
        show_labels: true,
        show_values: true,
        show_y_axis: true,
        show_legend: true,
        legend_position: position,
        grid: Grid {
            show: true,
            color: Some("#e0e0e0".to_owned()),
        },
        label_font_size: Some(11.0),
        value_font_size: Some(10.0),
        y_axis_font_size: Some(9.0),
        label_color: Some(hex_rgb(0x11_22_33)),
        value_color: Some(hex_rgb(0x44_55_66)),
        y_axis_color: Some(hex_rgb(0x77_88_99)),
        ..Options::default()
    }
}

/// The three slices the pie and the doughnut both draw.
///
/// **Colours on the first and third only**, so the palette fallback is inside
/// the comparison rather than beside it. Whole numbers throughout: the pie's
/// legend spells a slice `label (value)` with the value unrounded, and
/// `Display` and JavaScript's number-to-string part company at both ends of
/// the range -- at or above `1e21` and below `1e-6`. Keeping the values whole
/// keeps that out of what is being asked.
fn three_slices() -> Vec<Slice> {
    vec![
        Slice {
            label: "a".to_owned(),
            value: 3.0,
            color: Some("#3366cc".to_owned()),
        },
        Slice {
            label: "b".to_owned(),
            value: 2.0,
            color: None,
        },
        Slice {
            label: "c".to_owned(),
            value: 1.0,
            color: Some("#cc6633".to_owned()),
        },
    ]
}

/// One chart, encoded as a page the way `ours` does.
fn encoded(chart: Element) -> Vec<u8> {
    let scene = Root::new(200.0)
        .height(120.0)
        .children(chart)
        .into_scene()
        .unwrap_or_else(|error| {
            unreachable!("the scene did not assemble: {error}")
        });
    codec::encode(&scene)
}

/// Compares one kind, and fails first if there is nothing to compare.
///
/// **An agreement between two nothings is an agreement.** A byte comparison
/// that passes says the ports match; it does not say the case had a subject.
/// So the asset is checked for content and the scene for the chart's own node
/// -- `from_the_chart` cannot find a name that was never encoded -- before
/// either is compared.
fn agrees(name: &str, ours: &[u8], theirs: &str) {
    let theirs = theirs.trim();
    assert!(
        !theirs.is_empty(),
        "the committed bytes for the {name} are empty, so this would compare \
         against nothing -- generate them from the TypeScript surface"
    );
    let decoded: Vec<u8> = (0..theirs.len() / 2)
        .map(|index| {
            u8::from_str_radix(&theirs[index * 2..index * 2 + 2], 16)
                .unwrap_or_else(|error| {
                    unreachable!("the {name} asset is not hex: {error}")
                })
        })
        .collect();

    let mine = hex(from_the_chart(ours, name));
    assert!(
        mine.len() > 64,
        "the {name} encodes to {} hex digits from its own node on, which is \
         not a chart",
        mine.len()
    );
    assert_eq!(
        mine,
        hex(from_the_chart(&decoded, name)),
        "the two {name} implementations disagree. One of them is wrong and \
         this comparison cannot say which -- read the rendered checks, which \
         measure the picture rather than the agreement"
    );
}

/// The three labels and two series both cartesian cases draw.
///
/// Shared rather than written twice, because the legend-position case exists to
/// isolate **one property**: if the data could differ, a disagreement there
/// would have two possible causes and the case would stop being about the
/// branch.
fn cartesian() -> ([String; 3], [Dataset; 2]) {
    (
        ["a".to_owned(), "b".to_owned(), "c".to_owned()],
        [
            Dataset {
                label: Some("Sales".to_owned()),
                color: Some("#3366cc".to_owned()),
                data: vec![1.0, 3.0, 2.0],
            },
            Dataset {
                label: None,
                color: None,
                data: vec![3.0, 1.0, 2.0],
            },
        ],
    )
}

#[test]
fn both_surfaces_encode_the_same_line_chart() {
    let (labels, datasets) = cartesian();
    let chart = line(&labels, &datasets, &everything(LegendPosition::Left))
        .unwrap_or_else(|error| {
            unreachable!("the chart did not build: {error}")
        });
    agrees("line chart", &encoded(chart), THEIR_LINE);
}

/// The fourth frame branch, which nothing compared until this case.
///
/// `framed` sends `Left` and `Right` down its `Row` arm and `Top` and `Bottom`
/// down its `Column` arm, and picks the child order from the same match. Three
/// of the four positions ride on a kind above; **`Right` rode on nothing**, and
/// the bar case carries no legend at all.
///
/// **Checked to render before it was pinned.** On the TypeScript surface a
/// 240-wide bar chart's plot spans 216px with no legend and 176px with the
/// legend at either `left` or `right` -- the same width both ways, with the
/// legend taking its own side. So the branch was uncovered rather than broken,
/// and this is a test rather than a fix wearing one.
#[test]
fn both_surfaces_encode_the_same_line_chart_with_the_legend_on_the_right() {
    let (labels, datasets) = cartesian();
    let chart = line(&labels, &datasets, &everything(LegendPosition::Right))
        .unwrap_or_else(|error| {
            unreachable!("the chart did not build: {error}")
        });
    agrees("line chart", &encoded(chart), THEIR_LINE_RIGHT);
}

#[test]
fn both_surfaces_encode_the_same_pie() {
    let chart = pie(&three_slices(), &everything(LegendPosition::Top))
        .unwrap_or_else(|error| {
            unreachable!("the chart did not build: {error}")
        });
    agrees("pie chart", &encoded(chart), THEIR_PIE);
}

#[test]
fn both_surfaces_encode_the_same_doughnut() {
    // v1's `chartOptions?.innerRadius ?? 0.6`, which is what the TypeScript
    // surface passes and what my own builder has no default for.
    // **No `0.6` here any more, and that is the point.** The suites used to
    // pass it explicitly on this side, which meant they agreed about a number
    // they were both being told rather than about a default only one surface
    // had. `doughnut` now carries v1's default itself.
    let chart = doughnut(&three_slices(), &everything(LegendPosition::Bottom))
        .unwrap_or_else(|error| {
            unreachable!("the chart did not build: {error}")
        });
    agrees("doughnut chart", &encoded(chart), THEIR_DOUGHNUT);
}

/// The five hooks, compared by what they build rather than by what they are.
///
/// **A function cannot be encoded**, so this pins their *effect*: the same
/// formatter and the same hatch on both surfaces must produce the same tree.
/// Each hatch takes its index into the node it returns, so calling them in the
/// wrong order, or calling one of them once, encodes differently.
///
/// The formatters round before they stringify. A y-axis division arrives as
/// something like `2.4000000000000004`, and `Display` and JavaScript's
/// number-to-string part company on exactly that kind of value -- rounding
/// first keeps the languages' spelling rules out of a comparison that is about
/// the hook.
#[test]
fn both_surfaces_encode_the_same_chart_through_the_same_hooks() {
    let (labels, datasets) = cartesian();
    let options = Options {
        x_axis_label_formatter: Some(Rc::new(|label: &str, index: usize| {
            format!("{label}#{index}")
        })),
        y_axis_label_formatter: Some(Rc::new(|value: f64| {
            format!("${}", value.round())
        })),
        render_label_item: Some(Rc::new(|item: LabelItem<'_>| {
            Some(
                BoxElement::new()
                    .name(format!("hatch label {}", item.index))
                    .with_style(
                        Style::new()
                            .width(px(4.0 + item.index as f32))
                            .height(px(4.0))
                            .background_color(hex_rgb(0xff_00_00)),
                    ),
            )
        })),
        render_value_item: Some(Rc::new(|item: ValueItem| {
            Some(
                BoxElement::new()
                    .name(format!(
                        "hatch value {}.{}",
                        item.index, item.dataset_index
                    ))
                    .with_style(
                        Style::new()
                            .width(px(3.0))
                            .height(px(3.0))
                            .background_color(hex_rgb(0x00_ff_00)),
                    ),
            )
        })),
        render_legend_item: Some(Rc::new(|item: LegendItem<'_>| {
            Some(
                BoxElement::new()
                    .name(format!("hatch legend {}", item.index))
                    .with_style(
                        Style::new()
                            .width(px(6.0))
                            .height(px(6.0))
                            .background_color(
                                meo_canvas_core::parse_color(item.color)
                                    .unwrap_or(hex_rgb(0x00_00_00)),
                            ),
                    ),
            )
        })),
        ..everything(LegendPosition::Bottom)
    };
    let chart = bar(&labels, &datasets, &options).unwrap_or_else(|error| {
        unreachable!("the chart did not build: {error}")
    });
    agrees("bar chart", &encoded(chart), THEIR_HATCHES);
}
