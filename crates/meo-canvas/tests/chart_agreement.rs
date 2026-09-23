//! The same chart, built on both surfaces, compared as bytes. A chart has no
//! external referee, so two independent implementations agreeing is the
//! strongest check; rendering guards the geometry itself. The bytes are
//! committed, since this runs before the TypeScript suite would write them.

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

/// The chart both surfaces build, with every option on: two implementations
/// agree trivially about a branch neither takes.
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
        .map_or_else(
            |error| unreachable!("the scene did not assemble: {error}"),
            |(scene, _)| scene,
        );
    codec::encode(&scene)
}

/// Where the chart's own node begins, by its name in the byte stream: the page
/// frames differ between `Root::new` and `encodeScene`, and that is the harness
/// rather than the chart.
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

/// Hex, because it has no tail cases: base64's padding depends on the output
/// length modulo three, so a chart might only ever reach one tail.
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

/// Every option switched on, and a legend on a stated side: the position is the
/// one option that turns the chart's root from a column into a row.
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

/// The three slices the pie and doughnut draw, coloured on the first and third
/// so the palette fallback is compared. Whole values, since `Display` and
/// JavaScript spell numbers differently at `1e21` and below `1e-6`.
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
        .map_or_else(
            |error| unreachable!("the scene did not assemble: {error}"),
            |(scene, _)| scene,
        );
    codec::encode(&scene)
}

/// Compares one kind, and fails first if there is nothing to compare: the asset
/// must have content and the scene the chart's node, since two nothings agree.
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

/// The labels and series both cartesian cases draw, shared so the
/// legend-position case isolates one property.
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

/// The fourth frame branch: `framed` sends `Left` and `Right` down its `Row`
/// arm, and `Right` rode on no other case. Checked to render first -- the plot
/// is 176px wide with the legend on either side.
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
    // No `0.6` passed here: `doughnut` carries the default itself, so this
    // compares the default rather than a number both sides were told.
    let chart = doughnut(&three_slices(), &everything(LegendPosition::Bottom))
        .unwrap_or_else(|error| {
            unreachable!("the chart did not build: {error}")
        });
    agrees("doughnut chart", &encoded(chart), THEIR_DOUGHNUT);
}

/// The five hooks, compared by what they build, since a function cannot be
/// encoded. Each hatch puts its index in its node, so a wrong order or a missed
/// call encodes differently; the formatters round first, keeping the languages'
/// number spellings out.
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

/// As [`THEIR_LINE`], for the chart whose y-axis colour comes from the
/// fallback.
const THEIR_AXIS_FALLBACK: &str =
    include_str!("assets/chart/axis-fallback-bytes.txt");
/// As [`THEIR_LINE`], for the doughnut at a caller-chosen hole.
const THEIR_DOUGHNUT_INNER: &str =
    include_str!("assets/chart/doughnut-inner-bytes.txt");

/// Every option of [`everything`] with `axis_color` in place of the y-axis
/// colour, written out as the TypeScript bag must be. `y_axis_color: None` is
/// the whole case, and the colour is neither arm's, so a wrong arm of
/// `y_axis_color.or(axis_color)` shows.
fn axis_fallback() -> Options {
    Options {
        show_labels: true,
        show_values: true,
        show_y_axis: true,
        show_legend: true,
        legend_position: LegendPosition::Bottom,
        grid: Grid {
            show: true,
            color: Some("#e0e0e0".to_owned()),
        },
        label_font_size: Some(11.0),
        value_font_size: Some(10.0),
        y_axis_font_size: Some(9.0),
        label_color: Some(hex_rgb(0x11_22_33)),
        value_color: Some(hex_rgb(0x44_55_66)),
        y_axis_color: None,
        axis_color: Some(hex_rgb(0x22_cc_88)),
        ..Options::default()
    }
}

/// The hole this side is told, rather than the `0.6` both surfaces default to:
/// `0.35` is given to both.
const CHOSEN_INNER_FRACTION: f64 = 0.35;

/// `axis_color` as the fallback under an absent `y_axis_color`, a branch no
/// other case reaches, since `everything` always sets `y_axis_color`.
#[test]
fn both_surfaces_encode_the_same_chart_through_the_axis_colour_fallback() {
    let (labels, datasets) = cartesian();
    let chart =
        bar(&labels, &datasets, &axis_fallback()).unwrap_or_else(|error| {
            unreachable!("the chart did not build: {error}")
        });
    agrees("bar chart", &encoded(chart), THEIR_AXIS_FALLBACK);
}

/// The doughnut at a hole the caller chose rather than the one both sides
/// default to.
#[test]
fn both_surfaces_encode_the_same_doughnut_at_a_chosen_inner_fraction() {
    let options = Options {
        inner_fraction: Some(CHOSEN_INNER_FRACTION),
        ..everything(LegendPosition::Bottom)
    };
    let chart = doughnut(&three_slices(), &options).unwrap_or_else(|error| {
        unreachable!("the chart did not build: {error}")
    });
    agrees("doughnut chart", &encoded(chart), THEIR_DOUGHNUT_INNER);
}

/// The two option cases above can fail: both agree with the option removed too,
/// so what makes them evidence is that taking it away moves the bytes here.
#[test]
fn the_two_new_options_reach_the_encoded_scene() {
    let (labels, datasets) = cartesian();
    let with_axis = encoded(
        bar(&labels, &datasets, &axis_fallback())
            .unwrap_or_else(|error| unreachable!("{error}")),
    );
    let without_axis = encoded(
        bar(
            &labels,
            &datasets,
            &Options {
                axis_color: None,
                ..axis_fallback()
            },
        )
        .unwrap_or_else(|error| unreachable!("{error}")),
    );
    assert_ne!(
        hex(from_the_chart(&with_axis, "bar chart")),
        hex(from_the_chart(&without_axis, "bar chart")),
        "`axis_color` does not change the encoded chart, so the case that \
         pins it would pass with the fallback arm never taken"
    );

    let chosen = encoded(
        doughnut(
            &three_slices(),
            &Options {
                inner_fraction: Some(CHOSEN_INNER_FRACTION),
                ..everything(LegendPosition::Bottom)
            },
        )
        .unwrap_or_else(|error| unreachable!("{error}")),
    );
    let defaulted = encoded(
        doughnut(&three_slices(), &everything(LegendPosition::Bottom))
            .unwrap_or_else(|error| unreachable!("{error}")),
    );
    assert_ne!(
        hex(from_the_chart(&chosen, "doughnut chart")),
        hex(from_the_chart(&defaulted, "doughnut chart")),
        "a chosen `inner_fraction` encodes the same as the default, so the \
         case that pins it is pinning the default a second time"
    );
}

/// Whether a run of bytes holds a piece of text: the codec writes strings
/// literally, so a label's spelling is greppable in the encoded page.
fn contains(bytes: &[u8], needle: &str) -> bool {
    bytes
        .windows(needle.len())
        .any(|window| window == needle.as_bytes())
}

/// A value label is the number the caller gave, not a rounded one. The
/// committed assets use whole numbers, so `2.345` is the case; the y axis is
/// off, since its default rounds and would put `2.35` in the scene.
#[test]
fn a_value_label_is_written_as_given_rather_than_rounded() {
    let labels = ["a".to_owned()];
    let datasets = [Dataset {
        label: None,
        color: None,
        data: vec![2.345],
    }];
    let options = Options {
        show_values: true,
        show_y_axis: false,
        ..Options::default()
    };
    let chart = bar(&labels, &datasets, &options).unwrap_or_else(|error| {
        unreachable!("the chart did not build: {error}")
    });
    let bytes = encoded(chart);
    assert!(
        contains(&bytes, "2.345"),
        "the value label does not carry the value as written"
    );
    assert!(
        !contains(&bytes, "2.35"),
        "the value label is rounded to two decimals, where the other surface \
         writes the number it was given"
    );
}

/// A slice label is drawn in the chart's own font family. The legend is off,
/// since it sets the family and would satisfy the assertion whatever the slice
/// did.
#[test]
fn a_slice_label_takes_the_chart_font_family() {
    let options = Options {
        show_labels: true,
        show_legend: false,
        font_family: Some("Fixture".to_owned()),
        ..Options::default()
    };
    let chart = pie(&three_slices(), &options).unwrap_or_else(|error| {
        unreachable!("the chart did not build: {error}")
    });
    assert!(
        contains(&encoded(chart), "Fixture"),
        "a slice label does not carry the chart's font family, so a pie drawn \
         in a stated family renders its labels in the default one"
    );
}

/// A colour this crate cannot read is refused, as the other surface refuses it,
/// rather than drawn black. Each row names a different site; a dataset's colour
/// reaches three from one string, so the first to run reports.
#[test]
fn an_unreadable_colour_is_refused_rather_than_drawn_in_black() {
    let labels = ["a".to_owned()];
    let bad = "not a colour";

    let coloured = |colour: Option<&str>| {
        [Dataset {
            label: None,
            color: colour.map(ToOwned::to_owned),
            data: vec![1.0],
        }]
    };
    let slices = |colour: Option<&str>| {
        vec![Slice {
            label: "a".to_owned(),
            value: 1.0,
            color: colour.map(ToOwned::to_owned),
        }]
    };
    let with_grid = |colour: &str| Options {
        grid: Grid {
            show: true,
            color: Some(colour.to_owned()),
        },
        ..Options::default()
    };

    assert!(
        bar(&labels, &coloured(Some(bad)), &Options::default()).is_err(),
        "a bar drawn in an unreadable colour is not refused"
    );
    assert!(
        bar(&labels, &coloured(None), &with_grid(bad)).is_err(),
        "an unreadable grid colour is not refused"
    );
    assert!(
        line(&labels, &coloured(Some(bad)), &Options::default()).is_err(),
        "a series drawn in an unreadable colour is not refused"
    );
    assert!(
        pie(&slices(Some(bad)), &Options::default()).is_err(),
        "a slice drawn in an unreadable colour is not refused"
    );

    // **The control, and it is what makes the four above about the colour.**
    // Every one of these builds the same chart with a colour that reads, so a
    // refusal that fired on the shape rather than on the string would show up
    // here as a chart that no longer builds at all.
    assert!(
        bar(&labels, &coloured(Some("#3366cc")), &Options::default()).is_ok()
    );
    assert!(bar(&labels, &coloured(None), &with_grid("#e0e0e0")).is_ok());
    assert!(
        line(&labels, &coloured(Some("#3366cc")), &Options::default()).is_ok()
    );
    assert!(pie(&slices(Some("#3366cc")), &Options::default()).is_ok());

    // An unset grid colour keeps its default rather than being refused: saying
    // nothing and saying something unreadable are different answers.
    assert!(
        bar(
            &labels,
            &coloured(None),
            &Options {
                grid: Grid {
                    show: true,
                    color: None,
                },
                ..Options::default()
            }
        )
        .is_ok(),
        "a chart with no grid colour is refused, so the absent case was \
         folded into the unreadable one"
    );
}

/// A doughnut is named for what the caller asked for, not for its hole:
/// `inner_fraction: Some(0.0)` draws a pie's circle, and the encoded name must
/// still say doughnut.
#[test]
fn a_doughnut_with_no_hole_is_still_a_doughnut() {
    let flat = Options {
        inner_fraction: Some(0.0),
        ..Options::default()
    };
    let chart = doughnut(&three_slices(), &flat).unwrap_or_else(|error| {
        unreachable!("the chart did not build: {error}")
    });
    assert!(
        contains(&encoded(chart), "doughnut chart"),
        "a doughnut with a hole of zero names its own node `pie chart`, so a \
         caller who asked for one cannot find it by name"
    );

    // The control: a pie is still a pie, so the repair is about the kind
    // rather than about naming everything a doughnut.
    let chart =
        pie(&three_slices(), &Options::default()).unwrap_or_else(|error| {
            unreachable!("the chart did not build: {error}")
        });
    let bytes = encoded(chart);
    assert!(
        contains(&bytes, "pie chart"),
        "a pie is no longer named as one"
    );
    assert!(
        !contains(&bytes, "doughnut chart"),
        "a pie is named as a doughnut"
    );
}
