//! Every chart option, swept across every kind, compared with the TypeScript
//! surface: a field set on one pinned chart says nothing about the kinds that
//! route it differently. Each row of `differential-digests.txt` is a hash and a
//! byte length, written by `chart.differential.test.ts` alone.

use std::collections::BTreeMap;

use meo_canvas::{
    Root,
    chart::{
        bar::{Dataset, Grid, Options, bar},
        frame::LegendPosition,
        line::line,
        pie::{Slice, doughnut, pie},
    },
    hex_rgb,
    scene::codec,
};

/// What `chart.differential.test.ts` wrote.
const THEIRS: &str = include_str!("assets/chart/differential-digests.txt");

/// The bar chart `chart_agreement.rs` pins, as an independent check on framing.
const PINNED_BAR: &str = include_str!("assets/chart/bar-bytes.txt");

/// The column line the asset opens with, asserted rather than skipped.
const COLUMNS: &str = "# case\tdigest\tbytes";

/// What a row carries where a chart refused to build.
const REFUSED: &str = "refused";

/// FNV-1a, 32-bit, with the specification's offset basis and prime, since the
/// other surface computes the same function from the same constants. A row
/// passes only if hash and length both match, over a fixed set.
const FNV_OFFSET_BASIS: u32 = 0x811c_9dc5;
/// The multiplier of the same specification.
const FNV_PRIME: u32 = 0x0100_0193;

fn fnv1a(bytes: &[u8]) -> String {
    let hash = bytes.iter().fold(FNV_OFFSET_BASIS, |hash, byte| {
        (hash ^ u32::from(*byte)).wrapping_mul(FNV_PRIME)
    });
    format!("{hash:08x}")
}

/// The four kinds, and the node each names.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Bar,
    Line,
    Pie,
    Doughnut,
}

impl Kind {
    const fn mark(self) -> &'static str {
        match self {
            Self::Bar => "bar chart",
            Self::Line => "line chart",
            Self::Pie => "pie chart",
            Self::Doughnut => "doughnut chart",
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Bar => "bar",
            Self::Line => "line",
            Self::Pie => "pie",
            Self::Doughnut => "doughnut",
        }
    }

    /// Whether the kind draws a y axis, which is what an axis colour reaches.
    const fn draws_an_axis(self) -> bool {
        matches!(self, Self::Bar | Self::Line)
    }
}

const CARTESIAN_KINDS: [Kind; 2] = [Kind::Bar, Kind::Line];
const RADIAL_KINDS: [Kind; 2] = [Kind::Pie, Kind::Doughnut];
const EVERY_KIND: [Kind; 4] =
    [Kind::Bar, Kind::Line, Kind::Pie, Kind::Doughnut];

/// The two shapes a chart takes data in.
#[derive(Clone)]
enum Data {
    Cartesian(Vec<String>, Vec<Dataset>),
    Radial(Vec<Slice>),
}

/// One row of the sweep.
struct Case {
    name: String,
    kind: Kind,
    data: Data,
    options: Options,
}

fn label(text: &str) -> String {
    text.to_owned()
}

fn series(data: &[f64], name: Option<&str>, colour: Option<&str>) -> Dataset {
    Dataset {
        label: name.map(label),
        color: colour.map(label),
        data: data.to_vec(),
    }
}

fn slice(name: &str, value: f64, colour: Option<&str>) -> Slice {
    Slice {
        label: label(name),
        value,
        color: colour.map(label),
    }
}

fn cartesian(labels: &[&str], datasets: Vec<Dataset>) -> Data {
    Data::Cartesian(labels.iter().copied().map(label).collect(), datasets)
}

/// Every option switched on, so a case varies one thing against a known rest.
fn every() -> Options {
    Options {
        show_labels: true,
        show_values: true,
        show_y_axis: true,
        show_legend: true,
        grid: Grid {
            show: true,
            color: Some(label("#e0e0e0")),
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

/// The chart every option case varies, so a difference is the option.
fn bar_data() -> Data {
    cartesian(
        &["a", "b"],
        vec![
            series(&[1.0, 2.0], Some("Sales"), Some("#3366cc")),
            series(&[2.0, 1.0], None, None),
        ],
    )
}

fn pie_data() -> Vec<Slice> {
    vec![
        slice("a", 3.0, Some("#3366cc")),
        slice("b", 2.0, None),
        slice("c", 1.0, Some("#cc6633")),
    ]
}

fn data_for(kind: Kind) -> Data {
    if kind.draws_an_axis() {
        bar_data()
    } else {
        Data::Radial(pie_data())
    }
}

/// A string no CSS syntax spells as a colour.
const UNREADABLE: &str = "not-a-colour";

/// Long enough to wrap and to dwarf its slot, which is where a label's box
/// stops agreeing.
fn long_label() -> String {
    "l".repeat(200)
}

/// The whole sweep, assembled from the six tables below.
fn cases() -> Vec<Case> {
    let mut out: Vec<Case> = Vec::new();
    push_data_cases(&mut out);
    push_slice_cases(&mut out);
    push_mark_collision_cases(&mut out);
    push_unreadable_colour_cases(&mut out);
    push_option_cases(&mut out);
    push_combination_cases(&mut out);
    out
}

fn push(
    out: &mut Vec<Case>,
    name: String,
    kind: Kind,
    data: Data,
    options: Options,
) {
    out.push(Case {
        name,
        kind,
        data,
        options,
    });
}

/// One option bag against every kind, which is the pairing the pinned charts
/// cannot make: each of them carries one kind and one bag.
fn every_kind(out: &mut Vec<Case>, name: &str, options: &Options) {
    for kind in EVERY_KIND {
        push(
            out,
            format!("option/{name}/{}", kind.name()),
            kind,
            data_for(kind),
            options.clone(),
        );
    }
}

/// Shapes of data, on the two kinds that take a series. Each is a case the
/// pinned charts cannot reach: they carry one shape between them.
#[expect(
    clippy::too_many_lines,
    reason = "one table of one thing, and splitting it at an arbitrary index \
              to satisfy a line count would hide a case in a second list \
              nobody reads beside the first"
)]
fn push_data_cases(out: &mut Vec<Case>) {
    let long = long_label();
    let shapes: Vec<(&str, Data)> = vec![
        (
            "zeros",
            cartesian(
                &["a", "b", "c"],
                vec![series(&[0.0, 0.0, 0.0], Some("Zero"), None)],
            ),
        ),
        (
            "one-datum",
            cartesian(&["a"], vec![series(&[1.0], Some("One"), None)]),
        ),
        (
            "all-equal",
            cartesian(
                &["a", "b", "c"],
                vec![series(&[2.0, 2.0, 2.0], Some("Flat"), None)],
            ),
        ),
        (
            "negatives",
            cartesian(
                &["a", "b", "c"],
                vec![series(&[1.0, -2.0, 3.0], Some("Signed"), None)],
            ),
        ),
        (
            "one-negative",
            cartesian(&["a"], vec![series(&[-1.0], Some("Down"), None)]),
        ),
        // Nine series against a palette of eight, so the wrap is inside the
        // comparison rather than beside it.
        (
            "palette-wrap",
            cartesian(
                &["a", "b"],
                (0..9)
                    .map(|index| {
                        series(
                            &[f64::from(index + 1), f64::from(9 - index)],
                            Some(&format!("S{index}")),
                            None,
                        )
                    })
                    .collect(),
            ),
        ),
        (
            "empty-label",
            cartesian(&["", "b"], vec![series(&[1.0, 2.0], Some("E"), None)]),
        ),
        (
            "long-label",
            cartesian(
                &[long.as_str(), "b"],
                vec![series(&[1.0, 2.0], Some("L"), None)],
            ),
        ),
        (
            "non-ascii-label",
            cartesian(
                &["日本語", "émoji 🎉"],
                vec![series(&[1.0, 2.0], Some("U"), None)],
            ),
        ),
        (
            "empty-series-label",
            cartesian(&["a", "b"], vec![series(&[1.0, 2.0], Some(""), None)]),
        ),
        ("no-labels-no-data", cartesian(&[], vec![])),
        ("labels-without-datasets", cartesian(&["a", "b"], vec![])),
        (
            "datasets-without-labels",
            cartesian(&[], vec![series(&[1.0, 2.0], Some("X"), None)]),
        ),
        (
            "empty-data",
            cartesian(&["a", "b"], vec![series(&[], Some("Empty"), None)]),
        ),
        // A bar chart iterates the labels and a line chart iterates the data,
        // so the two mismatches are different branches rather than one.
        (
            "more-data-than-labels",
            cartesian(
                &["a"],
                vec![series(&[1.0, 2.0, 3.0], Some("Over"), None)],
            ),
        ),
        (
            "fewer-data-than-labels",
            cartesian(
                &["a", "b", "c"],
                vec![series(&[1.0], Some("Under"), None)],
            ),
        ),
        // A value with more than two decimals is where a rounded label and an
        // unrounded one part company; whole numbers cannot tell them apart.
        (
            "fractional",
            cartesian(
                &["a", "b"],
                vec![series(&[1.5, 2.25], Some("Frac"), None)],
            ),
        ),
        (
            "thirds",
            cartesian(
                &["a", "b"],
                vec![series(&[1.0 / 3.0, 2.0 / 3.0], Some("Third"), None)],
            ),
        ),
        (
            "large",
            cartesian(
                &["a", "b"],
                vec![series(&[1e6, 3e6], Some("Big"), None)],
            ),
        ),
        (
            "tiny",
            cartesian(
                &["a", "b"],
                vec![series(&[1e-3, 2e-3], Some("Small"), None)],
            ),
        ),
    ];
    for (shape, data) in shapes {
        for kind in CARTESIAN_KINDS {
            push(
                out,
                format!("data/{shape}/{}", kind.name()),
                kind,
                data.clone(),
                every(),
            );
        }
    }
}

/// A label spelled exactly like the node this file slices from, so the mark
/// appears three times in the bytes;
/// [`a_label_spelling_the_mark_does_not_move_the_slice`] measures the collision
/// itself.
fn push_mark_collision_cases(out: &mut Vec<Case>) {
    for kind in CARTESIAN_KINDS {
        push(
            out,
            format!("data/mark-collision/{}", kind.name()),
            kind,
            cartesian(
                &[kind.mark(), "b"],
                vec![series(&[1.0, 2.0], Some(kind.mark()), None)],
            ),
            every(),
        );
    }
    for kind in RADIAL_KINDS {
        push(
            out,
            format!("data/mark-collision/{}", kind.name()),
            kind,
            Data::Radial(vec![
                slice(kind.mark(), 3.0, None),
                slice("b", 1.0, None),
            ]),
            every(),
        );
    }
}

/// The same, for the two kinds that take slices.
fn push_slice_cases(out: &mut Vec<Case>) {
    let long = long_label();
    let slices: Vec<(&str, Vec<Slice>)> = vec![
        ("one-slice", vec![slice("only", 5.0, None)]),
        (
            "zero-value-slice",
            vec![
                slice("a", 3.0, None),
                slice("b", 0.0, None),
                slice("c", 1.0, None),
            ],
        ),
        (
            "all-zero",
            vec![slice("a", 0.0, None), slice("b", 0.0, None)],
        ),
        (
            "negative-slice",
            vec![slice("a", 3.0, None), slice("b", -1.0, None)],
        ),
        (
            "palette-wrap",
            (0..9)
                .map(|index| {
                    slice(&format!("s{index}"), f64::from(index + 1), None)
                })
                .collect(),
        ),
        (
            "empty-label",
            vec![slice("", 3.0, None), slice("b", 1.0, None)],
        ),
        (
            "long-label",
            vec![slice(&long, 3.0, None), slice("b", 1.0, None)],
        ),
        (
            "non-ascii-label",
            vec![slice("日本語", 3.0, None), slice("émoji 🎉", 1.0, None)],
        ),
        ("no-slices", vec![]),
        (
            "fractional",
            vec![slice("a", 1.5, None), slice("b", 2.25, None)],
        ),
        (
            "equal-slices",
            vec![
                slice("a", 1.0, None),
                slice("b", 1.0, None),
                slice("c", 1.0, None),
            ],
        ),
    ];
    for (shape, data) in slices {
        for kind in RADIAL_KINDS {
            push(
                out,
                format!("data/{shape}/{}", kind.name()),
                kind,
                Data::Radial(data.clone()),
                every(),
            );
        }
    }
}

/// A colour no CSS syntax spells, which both surfaces refuse. A pie has no
/// grid, so an unreadable grid colour there is an option nothing consumes.
fn push_unreadable_colour_cases(out: &mut Vec<Case>) {
    for kind in CARTESIAN_KINDS {
        push(
            out,
            format!("data/unreadable-series-colour/{}", kind.name()),
            kind,
            cartesian(
                &["a", "b"],
                vec![series(&[1.0, 2.0], Some("S"), Some(UNREADABLE))],
            ),
            every(),
        );
        push(
            out,
            format!("option/unreadable-grid-colour/{}", kind.name()),
            kind,
            bar_data(),
            Options {
                grid: Grid {
                    show: true,
                    color: Some(label(UNREADABLE)),
                },
                ..every()
            },
        );
    }
    for kind in RADIAL_KINDS {
        push(
            out,
            format!("data/unreadable-slice-colour/{}", kind.name()),
            kind,
            Data::Radial(vec![
                slice("a", 3.0, Some(UNREADABLE)),
                slice("b", 1.0, None),
            ]),
            every(),
        );
        push(
            out,
            format!("option/unreadable-grid-colour/{}", kind.name()),
            kind,
            Data::Radial(pie_data()),
            Options {
                grid: Grid {
                    show: true,
                    color: Some(label(UNREADABLE)),
                },
                ..every()
            },
        );
    }
}

/// Every option, one at a time, against every kind.
fn push_option_cases(out: &mut Vec<Case>) {
    every_kind(out, "all-default", &Options::default());
    every_kind(out, "empty-options", &Options::default());
    every_kind(
        out,
        "font-size-zero",
        &Options {
            label_font_size: Some(0.0),
            value_font_size: Some(0.0),
            y_axis_font_size: Some(0.0),
            ..every()
        },
    );
    every_kind(
        out,
        "font-size-negative",
        &Options {
            label_font_size: Some(-4.0),
            value_font_size: Some(-4.0),
            y_axis_font_size: Some(-4.0),
            ..every()
        },
    );
    every_kind(
        out,
        "font-size-fractional",
        &Options {
            label_font_size: Some(11.5),
            value_font_size: Some(10.25),
            y_axis_font_size: Some(9.75),
            ..every()
        },
    );
    every_kind(
        out,
        "grid-without-colour",
        &Options {
            grid: Grid {
                show: true,
                color: None,
            },
            ..every()
        },
    );
    every_kind(
        out,
        "grid-off-with-colour",
        &Options {
            grid: Grid {
                show: false,
                color: Some(label("#e0e0e0")),
            },
            ..every()
        },
    );
    every_kind(
        out,
        "grid-empty",
        &Options {
            grid: Grid {
                show: false,
                color: None,
            },
            ..every()
        },
    );

    // `axis_color` is the fallback `y_axis_color` overrides, so a case that
    // sets both never reaches it. These three separate the two.
    let without_y_axis_colour = Options {
        y_axis_color: None,
        ..every()
    };
    every_kind(
        out,
        "axis-colour-alone",
        &Options {
            axis_color: Some(hex_rgb(0x33_44_55)),
            ..without_y_axis_colour.clone()
        },
    );
    every_kind(
        out,
        "axis-colour-overridden",
        &Options {
            axis_color: Some(hex_rgb(0x33_44_55)),
            ..every()
        },
    );
    every_kind(out, "axis-colour-absent", &without_y_axis_colour);

    // A doughnut's hole, at both ends of its range and in the middle. The
    // other three kinds ignore it, which is itself a byte-checked claim. The
    // three names are written out rather than formatted from the number, so
    // the two surfaces are not relying on their float spellings agreeing.
    for (name, fraction) in [
        ("inner-radius-0", 0.0),
        ("inner-radius-0.25", 0.25),
        ("inner-radius-1", 1.0),
    ] {
        every_kind(
            out,
            name,
            &Options {
                inner_fraction: Some(fraction),
                ..every()
            },
        );
    }
}

/// The combinations, rather than the bags: every pairing of the four switches,
/// every side the legend can take, and the family, on every kind.
fn push_combination_cases(out: &mut Vec<Case>) {
    // The four booleans, every combination, so no pair hides behind another.
    for mask in 0_u8..16 {
        every_kind(
            out,
            &format!("toggles-{mask:04b}"),
            &Options {
                show_labels: mask & 1 != 0,
                show_values: mask & 2 != 0,
                show_y_axis: mask & 4 != 0,
                show_legend: mask & 8 != 0,
                ..every()
            },
        );
    }

    for (name, position) in [
        ("legend-top", LegendPosition::Top),
        ("legend-bottom", LegendPosition::Bottom),
        ("legend-left", LegendPosition::Left),
        ("legend-right", LegendPosition::Right),
    ] {
        every_kind(
            out,
            name,
            &Options {
                legend_position: position,
                ..every()
            },
        );
    }

    // The family lives in `Options` here and on `ChartProps` there, so this is
    // also the case that checks the two routes arrive at the same place.
    every_kind(
        out,
        "font-family",
        &Options {
            font_family: Some(label("Fixture")),
            ..every()
        },
    );
    // A legend switched on over series that name nothing for it to list.
    for kind in CARTESIAN_KINDS {
        push(
            out,
            format!("option/legend-without-labels/{}", kind.name()),
            kind,
            cartesian(
                &["a", "b"],
                vec![
                    series(&[1.0, 2.0], None, None),
                    series(&[2.0, 1.0], None, None),
                ],
            ),
            every(),
        );
    }
}

/// What a case produced. A missing mark is a divergence, not a crash: the
/// node's name is encoded, so two surfaces can disagree about it.
enum Encoded {
    /// The chart refused to build.
    Refused,
    /// The chart built and named itself something else, which is named here.
    Misnamed(&'static str),
    /// The chart's bytes, from its own node on.
    Bytes(Vec<u8>),
}

/// One chart's bytes from its own node on: the page frames of `Root::new` and
/// `encodeScene` differ, and that is the harness rather than the chart.
fn encoded(case: &Case) -> Encoded {
    let built = match (&case.data, case.kind) {
        (Data::Cartesian(labels, datasets), Kind::Bar) => {
            bar(labels, datasets, &case.options)
        }
        (Data::Cartesian(labels, datasets), Kind::Line) => {
            line(labels, datasets, &case.options)
        }
        (Data::Radial(slices), Kind::Pie) => pie(slices, &case.options),
        (Data::Radial(slices), Kind::Doughnut) => {
            doughnut(slices, &case.options)
        }
        _ => unreachable!(
            "the case `{}` pairs a kind with the other shape of data",
            case.name
        ),
    };
    let Ok(chart) = built else {
        return Encoded::Refused;
    };
    let (scene, _) = Root::new(200.0)
        .height(120.0)
        .children(chart)
        .into_scene()
        .unwrap_or_else(|error| {
            unreachable!("the scene did not assemble: {error}")
        });
    let bytes = codec::encode(&scene);
    from_the_chart(&bytes, case.kind.mark()).map_or_else(
        || {
            Encoded::Misnamed(
                EVERY_KIND
                    .into_iter()
                    .find(|other| {
                        from_the_chart(&bytes, other.mark()).is_some()
                    })
                    .map_or("no chart node at all", Kind::mark),
            )
        },
        Encoded::Bytes,
    )
}

fn from_the_chart(bytes: &[u8], mark: &str) -> Option<Vec<u8>> {
    let needle = mark.as_bytes();
    bytes
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|at| bytes[at..].to_vec())
}

/// The committed rows, with the column line asserted rather than skipped.
fn committed() -> BTreeMap<String, (String, String)> {
    let mut lines = THEIRS.trim().lines();
    assert_eq!(
        lines.next(),
        Some(COLUMNS),
        "the asset has lost its column line, so every row below it is being read at the wrong offset"
    );
    lines
        .map(|line| {
            let mut columns = line.split('\t');
            match (
                columns.next(),
                columns.next(),
                columns.next(),
                columns.next(),
            ) {
                (Some(name), Some(digest), Some(bytes), None) => {
                    (name.to_owned(), (digest.to_owned(), bytes.to_owned()))
                }
                _ => {
                    unreachable!("the asset row `{line}` is not three columns")
                }
            }
        })
        .collect()
}

#[test]
fn the_asset_names_exactly_the_cases_this_file_builds() {
    let rows = committed();
    let mine: Vec<String> = cases().into_iter().map(|case| case.name).collect();
    let theirs: Vec<String> = rows.keys().cloned().collect();
    let mut sorted = mine.clone();
    sorted.sort();
    assert_eq!(
        sorted, theirs,
        "the two surfaces do not build the same set of cases, so a case missing on one side would \
         otherwise pass by not being compared"
    );
    assert_eq!(mine.len(), rows.len(), "this file builds a case twice");
}

/// Every case, and every divergence rather than the first: a truncated list
/// cannot be told from a narrow break.
#[test]
fn every_case_encodes_to_the_bytes_the_other_surface_wrote() {
    let rows = committed();
    let mut divergences: Vec<String> = Vec::new();
    let mut compared = 0_usize;
    for case in cases() {
        let Some((digest, bytes)) = rows.get(&case.name) else {
            unreachable!("the asset has no row for `{}`", case.name)
        };
        let name = &case.name;
        match encoded(&case) {
            Encoded::Refused => {
                if digest != REFUSED {
                    divergences.push(format!(
                        "{name}: refused here, encoded to {bytes} bytes there"
                    ));
                }
            }
            Encoded::Misnamed(found) => divergences.push(format!(
                "{name}: names its node `{found}` here and `{}` there",
                case.kind.mark()
            )),
            Encoded::Bytes(encoded) => {
                let (mine, length) =
                    (fnv1a(&encoded), encoded.len().to_string());
                if digest == REFUSED {
                    divergences.push(format!(
                        "{name}: encoded to {length} bytes here, refused there"
                    ));
                } else if &length != bytes || &mine != digest {
                    // Both pairs, so the line is the whole comparison rather
                    // than a pointer to it: a reader holding the failure has
                    // what each side produced without re-running either.
                    divergences.push(format!(
                        "{name}: {mine} at {length} bytes here, {digest} at {bytes} there"
                    ));
                }
            }
        }
        compared += 1;
    }
    assert!(compared > 100, "only {compared} cases were compared");
    assert!(
        divergences.is_empty(),
        "{} of {compared} cases disagree with the other surface. One of the two is wrong and this \
         comparison cannot say which -- read the rendered checks, which measure the geometry rather \
         than the agreement.\n{}",
        divergences.len(),
        divergences.join("\n")
    );
}

/// The comparison has to be able to fail: each of these is a real chart one
/// option away from a committed one, and none may match the row it is named
/// after.
#[test]
fn a_chart_one_option_away_from_a_committed_one_does_not_match_it() {
    let rows = committed();
    let perturbations: Vec<(&str, Kind, Options)> = vec![
        (
            "option/toggles-1111/bar",
            Kind::Bar,
            Options {
                show_values: false,
                ..every()
            },
        ),
        (
            "option/legend-left/line",
            Kind::Line,
            Options {
                legend_position: LegendPosition::Right,
                ..every()
            },
        ),
        (
            "option/font-family/pie",
            Kind::Pie,
            Options {
                font_family: Some(label("Fixture")),
                label_color: Some(hex_rgb(0x01_02_03)),
                ..every()
            },
        ),
        (
            "option/inner-radius-0.25/doughnut",
            Kind::Doughnut,
            Options {
                inner_fraction: Some(0.75),
                ..every()
            },
        ),
    ];
    for (name, kind, options) in perturbations {
        let Some((digest, _)) = rows.get(name) else {
            unreachable!("no committed row named `{name}`")
        };
        let case = Case {
            name: name.to_owned(),
            kind,
            data: data_for(kind),
            options,
        };
        let Encoded::Bytes(bytes) = encoded(&case) else {
            unreachable!(
                "the perturbed `{name}` did not encode, so it compares nothing"
            )
        };
        assert_ne!(
            digest,
            &fnv1a(&bytes),
            "perturbing `{name}` did not change its bytes"
        );
    }
}

/// One option, how to vary it, and the kinds that cannot see it.
struct Variation {
    option: &'static str,
    vary: fn(Options) -> Options,
    inert: &'static [Kind],
}

/// The base every variation is measured against.
///
/// `inner_fraction` is set so that varying it is a change rather than an
/// arrival, which is a different thing to observe.
fn probe_base() -> Options {
    Options {
        inner_fraction: Some(0.5),
        ..every()
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "one row per option, and the point of the list is that it is \
              exhaustive; a split would make an option's absence from it \
              invisible"
)]
fn variations() -> Vec<Variation> {
    let entry = |option, vary, inert| Variation {
        option,
        vary,
        inert,
    };
    vec![
        entry(
            "show_labels",
            |o| Options {
                show_labels: false,
                ..o
            },
            &[],
        ),
        entry(
            "show_values",
            |o| Options {
                show_values: false,
                ..o
            },
            &[Kind::Line, Kind::Pie, Kind::Doughnut],
        ),
        entry(
            "show_y_axis",
            |o| Options {
                show_y_axis: false,
                ..o
            },
            &[Kind::Pie, Kind::Doughnut],
        ),
        entry(
            "show_legend",
            |o| Options {
                show_legend: false,
                ..o
            },
            &[],
        ),
        entry(
            "legend_position",
            |o| Options {
                legend_position: LegendPosition::Left,
                ..o
            },
            &[],
        ),
        entry(
            "grid",
            |o| Options {
                grid: Grid {
                    show: false,
                    color: None,
                },
                ..o
            },
            &[Kind::Pie, Kind::Doughnut],
        ),
        entry(
            "label_font_size",
            |o| Options {
                label_font_size: Some(22.0),
                ..o
            },
            &[],
        ),
        entry(
            "value_font_size",
            |o| Options {
                value_font_size: Some(20.0),
                ..o
            },
            &[Kind::Line, Kind::Pie, Kind::Doughnut],
        ),
        entry(
            "y_axis_font_size",
            |o| Options {
                y_axis_font_size: Some(18.0),
                ..o
            },
            &[Kind::Pie, Kind::Doughnut],
        ),
        entry(
            "label_color",
            |o| Options {
                label_color: Some(hex_rgb(0xff_00_00)),
                ..o
            },
            &[],
        ),
        entry(
            "value_color",
            |o| Options {
                value_color: Some(hex_rgb(0x00_ff_00)),
                ..o
            },
            &[Kind::Line, Kind::Pie, Kind::Doughnut],
        ),
        entry(
            "y_axis_color",
            |o| Options {
                y_axis_color: Some(hex_rgb(0x00_00_ff)),
                ..o
            },
            &[Kind::Pie, Kind::Doughnut],
        ),
        entry(
            "inner_fraction",
            |o| Options {
                inner_fraction: Some(0.2),
                ..o
            },
            &[Kind::Bar, Kind::Line, Kind::Pie],
        ),
        entry(
            "font_family",
            |o| Options {
                font_family: Some(label("Fixture")),
                ..o
            },
            &[],
        ),
    ]
}

/// Which options a kind can see, stated rather than assumed, in both
/// directions: an option that stops or starts being observable fails here.
/// Inert entries are not gaps -- a line chart draws no per-datum value.
#[test]
fn every_option_this_file_varies_can_be_seen_in_the_bytes() {
    for kind in EVERY_KIND {
        let first = bytes_of(kind, probe_base());
        for Variation {
            option,
            vary,
            inert,
        } in &variations()
        {
            let varied = bytes_of(kind, vary(probe_base()));
            if inert.contains(&kind) {
                assert_eq!(
                    first,
                    varied,
                    "`{option}` changed a {} chart, which this file records as ignoring it",
                    kind.name()
                );
            } else {
                assert_ne!(
                    first,
                    varied,
                    "`{option}` left a {} chart unchanged, so every case varying it agrees about nothing",
                    kind.name()
                );
            }
        }
    }
}

/// An axis colour is reachable only with `y_axis_color` unset, so a sweep
/// switching every option on reports it inert on every kind.
#[test]
fn an_axis_colour_is_reachable_only_through_the_fallback() {
    for kind in EVERY_KIND {
        let without = Options {
            y_axis_color: None,
            ..every()
        };
        let plain = bytes_of(kind, without.clone());
        let fallback = bytes_of(
            kind,
            Options {
                axis_color: Some(hex_rgb(0xff_00_ff)),
                ..without
            },
        );
        let overridden = bytes_of(
            kind,
            Options {
                axis_color: Some(hex_rgb(0xff_00_ff)),
                ..every()
            },
        );
        let y_only = bytes_of(kind, every());
        if kind.draws_an_axis() {
            assert_ne!(
                plain,
                fallback,
                "the axis colour did not reach a {} chart's y-axis text",
                kind.name()
            );
        } else {
            assert_eq!(
                plain,
                fallback,
                "a {} chart has no y axis and took an axis colour",
                kind.name()
            );
        }
        assert_eq!(
            overridden,
            y_only,
            "an explicit y-axis colour did not override the axis colour on a {} chart",
            kind.name()
        );
    }
}

fn bytes_of(kind: Kind, options: Options) -> Vec<u8> {
    let case = Case {
        name: format!("probe/{}", kind.name()),
        kind,
        data: data_for(kind),
        options,
    };
    match encoded(&case) {
        Encoded::Bytes(bytes) => bytes,
        Encoded::Refused | Encoded::Misnamed(_) => {
            unreachable!("the probe chart for {} did not encode", kind.name())
        }
    }
}

/// The unreadable colour is what makes those cases refuse: each refusing shape
/// is rebuilt with a readable colour and must encode. Which role the message
/// names is order of execution, not contract, and is not asserted.
#[test]
fn an_unreadable_colour_is_what_makes_a_chart_refuse() {
    /// The shapes an unreadable colour can reach, and whether each refuses.
    fn shapes(kind: Kind, colour: &str) -> Vec<(&'static str, Case, bool)> {
        let grid = Options {
            grid: Grid {
                show: true,
                color: Some(label(colour)),
            },
            ..every()
        };
        let case = |role: &'static str,
                    data: Data,
                    options: Options,
                    refuses: bool| {
            (
                role,
                Case {
                    name: format!("colour/{}/{role}", kind.name()),
                    kind,
                    data,
                    options,
                },
                refuses,
            )
        };
        if kind.draws_an_axis() {
            vec![
                case(
                    "series colour",
                    cartesian(
                        &["a", "b"],
                        vec![series(&[1.0, 2.0], Some("S"), Some(colour))],
                    ),
                    every(),
                    true,
                ),
                case("grid colour", bar_data(), grid, true),
            ]
        } else {
            vec![
                case(
                    "slice colour",
                    Data::Radial(vec![
                        slice("a", 3.0, Some(colour)),
                        slice("b", 1.0, None),
                    ]),
                    every(),
                    true,
                ),
                case("grid colour", Data::Radial(pie_data()), grid, false),
            ]
        }
    }

    for kind in EVERY_KIND {
        for ((role, unreadable, refuses), (_, readable, _)) in
            shapes(kind, UNREADABLE)
                .into_iter()
                .zip(shapes(kind, "#336699"))
        {
            assert!(
                !matches!(encoded(&readable), Encoded::Refused),
                "a {} chart with a {role} that reads is refused too, so the colour is not what this \
                 measures",
                kind.name()
            );
            assert_eq!(
                matches!(encoded(&unreadable), Encoded::Refused),
                refuses,
                "a {} chart with an unreadable {role} did not do what this file records",
                kind.name()
            );
        }
    }
}

/// A label spelling the mark does not move where the slice begins: the node's
/// name is encoded before its subtree's strings, so the same chart with an
/// equal-length label must slice to the same length.
#[test]
fn a_label_spelling_the_mark_does_not_move_the_slice() {
    for kind in EVERY_KIND {
        let mark = kind.mark();
        let decoy = match kind {
            Kind::Bar | Kind::Pie => "qqq qqqqq",
            Kind::Line => "qqqq qqqqq",
            Kind::Doughnut => "qqqqqqqq qqqqx",
        };
        assert_eq!(
            mark.len(),
            decoy.len(),
            "the decoy label must be the same length as the mark, or the two slices differ for that \
             reason instead of the one under test"
        );
        let build = |text: &str| Case {
            name: format!("collide/{}", kind.name()),
            kind,
            data: if kind.draws_an_axis() {
                cartesian(
                    &[text, "b"],
                    vec![series(&[1.0, 2.0], Some(text), None)],
                )
            } else {
                Data::Radial(vec![
                    slice(text, 3.0, None),
                    slice("b", 1.0, None),
                ])
            },
            options: every(),
        };
        let (Encoded::Bytes(colliding), Encoded::Bytes(plain)) =
            (encoded(&build(mark)), encoded(&build(decoy)))
        else {
            unreachable!("a probe chart for {} did not encode", kind.name())
        };
        assert_eq!(
            colliding.len(),
            plain.len(),
            "on a {} chart the slice began at the label rather than at the chart",
            kind.name()
        );
    }
}

/// The framing here is the one the pinned assets were written with: the same
/// bar chart `chart_agreement.rs` pins, checked against the bytes that file
/// committed.
#[test]
fn the_framing_agrees_with_the_pinned_bar_asset() {
    let theirs = PINNED_BAR.trim();
    assert!(
        !theirs.is_empty(),
        "the pinned bar asset is empty, so this compares against nothing"
    );
    let decoded: Vec<u8> = (0..theirs.len() / 2)
        .map(|index| {
            u8::from_str_radix(&theirs[index * 2..index * 2 + 2], 16)
                .unwrap_or_else(|error| {
                    unreachable!("the pinned bar asset is not hex: {error}")
                })
        })
        .collect();

    let case = Case {
        name: label("framing/bar"),
        kind: Kind::Bar,
        data: bar_data(),
        options: Options {
            show_legend: false,
            font_family: Some(label("Fixture")),
            ..every()
        },
    };
    let Encoded::Bytes(mine) = encoded(&case) else {
        unreachable!("the pinned bar chart did not encode")
    };
    assert_eq!(
        Some(mine),
        from_the_chart(&decoded, Kind::Bar.mark()),
        "this file frames a chart differently from the one the pinned assets were written with, so \
         every comparison above is against bytes of its own shape"
    );
}
