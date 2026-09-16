//! Where the two chart surfaces deliberately spell a number differently.
//!
//! # The divergence
//!
//! A chart label is a number turned into text, and the two languages part
//! company at the top of the range: JavaScript switches to exponential notation
//! at `1e21` and Rust's `Display` never does. So a value of `1e21` is drawn as
//! `1e+21` by the TypeScript surface and as `1000000000000000000000` here, and
//! every other chart check is silent about it -- `chart_differential.rs` stops
//! short of the threshold on purpose, and the pinned agreement charts carry
//! whole numbers far below it.
//!
//! **It is left open rather than fixed.** Closing it means implementing another
//! language's number formatting from scratch: nothing in this workspace spells
//! a number the JavaScript way, so there is no helper to reach for and no
//! precedent to follow. That is more commitment than a label at `1e21` earns.
//!
//! # Why it is pinned rather than written down
//!
//! A deviation recorded only in prose is indistinguishable from a defect, and
//! nothing tells the next reader whether it still holds. Pinned, it is
//! self-reporting: **the day somebody closes the gap, these tests fail, and
//! that failure is the notification.** The same shape as
//! `crates/meo-canvas-core/tests/taffy_negative_margin.rs`, which asserts the
//! numbers a dependency gets wrong so that a fix upstream cannot land quietly.
//!
//! Each assertion pins **the spelling on this side and the absence of the
//! other's**, so a change to either surface reddens a row and the message says
//! which one moved. A pin reading only "the two differ" would pass for two
//! surfaces that had both become wrong.
//!
//! # Two paths, two thresholds
//!
//! The y-axis divisions are rounded to two decimals before they are spelled and
//! the value labels are not, which moves the boundary by a decade: rounding
//! `1e21` yields `999999999999999900000`, below JavaScript's threshold, so the
//! axis agrees there and parts at `1e22`. The two controls are what make that a
//! measurement rather than a story -- one path agreeing at a threshold where
//! the other does not is the evidence that these are two paths and not one
//! condition described twice.

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

/// Whether the scene carries `text` as a string of its own.
///
/// Matched with its length prefix rather than as a loose substring, because
/// `1000000000000000000000` is a substring of `10000000000000000000000` and a
/// bare search would have one threshold's expectation satisfied by the other's
/// output.
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
