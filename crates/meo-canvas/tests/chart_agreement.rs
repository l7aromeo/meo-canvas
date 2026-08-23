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

use meo_canvas::{
    Root,
    chart::bar::{Dataset, Grid, Options, bar},
    hex_rgb,
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
    let scene = Root::new(200.0, 120.0)
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
fn from_the_chart(bytes: &[u8]) -> &[u8] {
    let needle = b"bar chart";
    let at = bytes
        .windows(needle.len())
        .position(|window| window == needle)
        .unwrap_or_else(|| unreachable!("the scene has no `bar chart` node"));
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
    let ours = hex(from_the_chart(&encoded));
    let theirs = hex(from_the_chart(
        &(0..theirs.len() / 2)
            .map(|index| {
                u8::from_str_radix(&theirs[index * 2..index * 2 + 2], 16)
                    .unwrap_or_else(|error| {
                        unreachable!("the asset is not hex: {error}")
                    })
            })
            .collect::<Vec<u8>>(),
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
