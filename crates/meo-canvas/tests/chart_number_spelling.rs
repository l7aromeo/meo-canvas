//! Where the two chart surfaces deliberately spell a number differently:
//! JavaScript turns exponential at `1e21`, `Display` never does. Left open,
//! since closing it means reimplementing another language's formatting; pinned
//! both ways so closing it fails here. The rounded y axis parts at `1e22`.

use meo_canvas::{
    Root,
    chart::bar::{Dataset, Options, bar},
    scene::codec,
};

/// A chart's encoded bytes, for one value and one label switched on.
fn encoded(value: f64, options: &Options) -> Vec<u8> {
    let labels = ["a".to_owned()];
    let datasets = [Dataset {
        label: None,
        color: None,
        data: vec![value],
    }];
    let chart = bar(&labels, &datasets, options).unwrap_or_else(|error| {
        unreachable!("the chart did not build: {error}")
    });
    let (scene, _) = Root::new(200.0)
        .height(120.0)
        .children(chart)
        .into_scene()
        .unwrap_or_else(|error| {
            unreachable!("the scene did not assemble: {error}")
        });
    codec::encode(&scene)
}

/// The value drawn against the bar, which is written as the caller gave it.
fn value_label(value: f64) -> Vec<u8> {
    encoded(
        value,
        &Options {
            show_values: true,
            ..Options::default()
        },
    )
}

/// The y-axis scale, whose divisions are rounded to two decimals first.
fn y_axis_label(value: f64) -> Vec<u8> {
    encoded(
        value,
        &Options {
            show_y_axis: true,
            ..Options::default()
        },
    )
}

/// Whether the scene carries `text` as a string of its own, matched with its
/// length prefix, since `1000000000000000000000` is a substring of
/// `10000000000000000000000`.
fn spells(bytes: &[u8], text: &str) -> bool {
    let mut needle = (text.len() as u32).to_le_bytes().to_vec();
    needle.extend_from_slice(text.as_bytes());
    bytes.windows(needle.len()).any(|window| window == needle)
}

#[test]
fn a_value_label_at_1e21_is_spelled_differently_on_each_surface() {
    let bytes = value_label(1e21);
    assert!(
        spells(&bytes, "1000000000000000000000"),
        "this surface no longer writes 1e21 in full. If that is a deliberate \
         change, the divergence this file records has moved rather than closed"
    );
    assert!(
        !spells(&bytes, "1e+21"),
        "this surface now spells 1e21 the way the TypeScript one does. If the \
         gap has been closed, delete this file and the pointer to it"
    );
}

#[test]
fn a_y_axis_label_at_1e22_is_spelled_differently_on_each_surface() {
    let bytes = y_axis_label(1e22);
    assert!(
        spells(&bytes, "10000000000000000000000"),
        "this surface no longer writes the top y-axis division at 1e22 in full"
    );
    assert!(
        !spells(&bytes, "1e+22"),
        "this surface now spells the top y-axis division the way the \
         TypeScript one does"
    );
}

#[test]
fn a_y_axis_label_at_1e21_is_spelled_the_same_on_both_surfaces() {
    let bytes = y_axis_label(1e21);
    assert!(
        spells(&bytes, "999999999999999900000"),
        "the y-axis division at 1e21 no longer rounds to a number below \
         JavaScript's exponential threshold, so the two paths no longer part \
         company a decade apart and the pin above is describing one condition \
         rather than two"
    );
}

#[test]
fn a_value_label_below_1e21_is_spelled_the_same_on_both_surfaces() {
    let bytes = value_label(1e20);
    assert!(
        spells(&bytes, "100000000000000000000"),
        "a value a decade below the threshold is no longer written in full, so \
         the divergence is wider than the threshold this file names"
    );
}
