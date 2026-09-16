//! Every chart option, swept across every kind, compared as bytes with the
//! TypeScript surface.
//!
//! # What this asks that `chart_agreement.rs` does not
//!
//! That file asks whether the two implementations agree about six charts. This
//! one asks whether they agree about a *combination* -- an option paired with a
//! kind, or with a shape of data, that no single pinned chart happens to carry.
//!
//! **Field coverage is not combination coverage**, and the gap is not
//! hypothetical. `font_family` is set on one pinned case, a bar chart, and the
//! bar chart is the kind that routes it correctly; a pie's slice label built
//! its own style and dropped the family, and every pinned pie case passed
//! because none of them set one. The second defect had the same shape: the
//! pinned values are whole on purpose, and whole numbers are the one class for
//! which rounding to two decimals and not rounding at all give the same string.
//!
//! So a suite can reach twenty of twenty fields, stay green, and say nothing
//! about either. What closes that is varying one thing at a time against every
//! kind, which is what [`cases`] does.
//!
//! # What is compared, and why it is not the bytes
//!
//! The bytes of every case run to about two megabytes, against roughly a
//! hundred and fifty kilobytes for every other committed chart asset together.
//! Each row of `assets/chart/differential-digests.txt` carries a hash and a
//! **byte length**: a bare hash mismatch tells a reader nothing, and a length
//! does -- both defects above were diagnosed from the four- and five-byte
//! difference a dropped string leaves.
//!
//! The asset is written by `chart.differential.test.ts` and only by it. `ci`
//! runs the Rust tests first, so a Rust test that wrote it would leave that
//! side comparing against the previous run's output.
//!
//! # Why the two sides build their own cases
//!
//! A single case list read by both would make them agree by construction about
//! *what* to build, which is the part worth testing. Each side constructs the
//! table itself; the hashes hold them to the same chart, and
//! [`the_asset_names_exactly_the_cases_this_file_builds`] refuses a case that
//! exists on one side only.

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

/// FNV-1a, 32-bit.
///
/// The offset basis and the prime are the values the FNV specification names
/// (Fowler/Noll/Vo, as published in `draft-eastlake-fnv`). A hash rather than
/// anything stronger because the other surface computes the same function from
/// the same two constants, and this crate's dev-dependencies carry none;
/// thirty-two bits is enough because a row passes only if its bytes hash equal
/// *and* are the same length, over a fixed committed set.
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

/// Long enough to wrap and to dwarf its slot, which is where a label's box
/// stops agreeing.
fn long_label() -> String {
    "l".repeat(200)
}

/// The whole sweep, assembled from the five tables below.
fn cases() -> Vec<Case> {
    let mut out: Vec<Case> = Vec::new();
    push_data_cases(&mut out);
    push_slice_cases(&mut out);
    push_mark_collision_cases(&mut out);
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
/// appears three times in the bytes instead of once.
///
/// The mark does two jobs -- it locates the comparison and it is part of what
/// is compared -- and the collision itself is measured by
/// [`a_label_spelling_the_mark_does_not_move_the_slice`]. What these cases add
/// is the encoding of the repeated string, compared across the two surfaces.
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

/// What a case produced.
///
/// **A missing mark is a divergence, not a crash.** The node's name is part of
/// what is encoded, so two surfaces can disagree about it as readily as about
/// a byte -- and a harness that treated an absent mark as impossible would
/// report that disagreement as its own defect.
enum Encoded {
    /// The chart refused to build.
    Refused,
    /// The chart built and named itself something else, which is named here.
    Misnamed(&'static str),
    /// The chart's bytes, from its own node on.
    Bytes(Vec<u8>),
}

/// One chart's bytes from its own node on.
///
/// The page frame is not part of the comparison: `Root::new` here and a page
/// root handed to `encodeScene` there are different framings with different
/// default styles, and their disagreement is about the harness rather than
/// either chart.
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

/// Every case, and **every** divergence rather than the first.
///
/// A run that stopped at the first mismatch would report one case where a
/// change had moved twenty, and a reader cannot tell a truncated list from a
/// narrow break -- which is the same reason `just test` passes
/// `--no-fail-fast`.
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

/// The comparison has to be able to fail.
///
/// A hash compared against a constant passes for a scene that encoded nothing,
/// and a row read from the wrong column passes for everything. Each of these is
/// a real chart one option away from a committed one, and none may match the
/// row it is named after.
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

/// Which options a kind can see, stated rather than assumed.
///
/// A case that varies an option the kind ignores agrees for free and reads as
/// coverage it is not. This pins both directions: an option that stops being
/// observable fails here, and so does one that starts. The inert entries are
/// not gaps -- a line chart draws no per-datum value and a pie has no y axis --
/// and naming them is what stops the list being read as one.
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

/// Which options a kind can see, stated rather than assumed.
///
/// A case that varies an option the kind ignores agrees for free and reads as
/// coverage it is not. This pins both directions: an option that stops being
/// observable fails here, and so does one that starts. The inert entries are
/// not gaps -- a line chart draws no per-datum value and a pie has no y axis --
/// and naming them is what stops the list being read as one.
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

/// An axis colour is reachable only with `y_axis_color` unset.
///
/// Which is why a sweep that switches every option on at once reports it inert
/// on every kind: the fallback is masked by the option that overrides it, and
/// a table built from that sweep would record a live option as dead.
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

/// A label that spells the mark does not move where the slice begins.
///
/// The mark does two jobs: it locates the comparison and it is part of what is
/// compared. A label is user text and can carry it, so the question is whether
/// the first occurrence is still the chart's own node. It is -- the node's name
/// is encoded before its subtree's strings -- and this measures that rather
/// than assuming it: the same chart with a label of the same byte length that
/// does not spell the mark must slice to the same number of bytes. A slice that
/// began at the label would be shorter.
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

/// The framing here is the framing the pinned assets were written with.
///
/// Everything above compares against an asset the other surface generates from
/// the same framing, so a change to the page wrapper or to where the slice
/// begins would move both sides together and pass. This is the same bar chart
/// `chart_agreement.rs` pins, checked against the bytes *that* file committed.
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
