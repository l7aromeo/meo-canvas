//! Where a chart's marks go, as fractions of the plot.
//!
//! # One derivation, two callers
//!
//! **This is a port of `packages/meo-canvas/src/chart.ts`, not a second
//! derivation from v1.** The TypeScript side worked the geometry out from v1
//! and verified it by rendering; deriving it again here would produce a third
//! set of numbers that could drift from the first two, **and nothing could
//! referee the difference -- Chrome has no charts.** So the TypeScript is the
//! specification and `tests/assets/chart/geometry.tsv` is it made checkable.
//!
//! # `f64`, and no fused multiply-add
//!
//! Both surfaces compute these and the two are compared, so this is the same
//! case as `animate`: **agreement is the objective and accuracy is not.**

#![expect(
    clippy::suboptimal_flops,
    reason = "compared against the TypeScript surface's own numbers; a fused \
              multiply-add rounds once where JavaScript rounds twice."
)]

use crate::Error;

/// How many bands a plot's gridlines divide it into.
pub const GRID_DIVISIONS: u32 = 5;

/// The share of a group's width left empty between groups.
///
/// v1: `barSpacing = groupWidth * 0.2`, half at each end, so a group's bars
/// occupy the middle 80% of their slot.
pub const BAR_GROUP_SPACING: f64 = 0.2;

/// v1's default series colours, in order.
const PALETTE: [&str; 8] = [
    "#4e79a7", "#f28e2c", "#e15759", "#76b7b2", "#59a14f", "#edc949",
    "#af7aa1", "#ff9da7",
];

/// The colour a series takes when it names none.
#[must_use]
pub fn series_color(index: usize, given: Option<&str>) -> String {
    given.map_or_else(
        || PALETTE[index % PALETTE.len()].to_owned(),
        ToOwned::to_owned,
    )
}

/// Where each gridline falls, as a fraction from the top of the plot.
///
/// **`divisions + 1` fractions, and the last one is never seen.** A fraction
/// of `1.0` puts a one-pixel rule with its *top* on the plot's bottom edge --
/// one row past the last row there is -- so a plot with five divisions shows
/// **five** lines rather than six. v1 does the same, stroking at
/// `chartY + finalChartHeight`, equally outside.
///
/// Kept rather than trimmed, because it is what the other surface emits and
/// this is a port. **But a test that counts six has counted emitted nodes and
/// not drawn lines**, which is the distinction a tree-shaped assertion cannot
/// make.
#[must_use]
pub fn grid_lines(divisions: u32) -> Vec<f64> {
    (0..=divisions)
        .map(|line| f64::from(line) / f64::from(divisions))
        .collect()
}

/// One bar's place in the plot, as fractions of it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bar {
    /// Distance from the plot's left edge.
    pub x: f64,
    /// How wide the bar is.
    pub width: f64,
    /// How tall, as a share of the plot's height.
    pub height: f64,
}

/// Where every bar of a cartesian chart sits.
///
/// # Errors
///
/// Returns [`Error::Chart`] for a negative value. **v1 mis-draws these three
/// different ways** -- a bar below the plot, a bar five times the height for
/// the *most* negative value, and nothing at all when every value is zero --
/// so they are refused rather than reproduced. A stated divergence, not an
/// omission.
///
/// # Panics
///
/// Never: `labels` and `series` are counts and the indices are drawn from
/// them.
pub fn bar_layout(
    labels: usize,
    series: usize,
    values: &[Vec<f64>],
    max_value: f64,
) -> Result<Vec<Vec<Bar>>, Error> {
    if values.iter().flatten().any(|value| *value < 0.0) {
        return Err(Error::Chart("a chart cannot draw a negative value"));
    }

    #[expect(
        clippy::cast_precision_loss,
        reason = "a label or series count past 2^53 is not a chart"
    )]
    let (labels_f, series_f) = (labels as f64, series as f64);
    let group_width = 1.0 / labels_f;
    let spacing = group_width * BAR_GROUP_SPACING;
    let width = (group_width - spacing) / series_f;

    Ok((0..labels)
        .map(|index| {
            (0..series)
                .map(|s| {
                    #[expect(
                        clippy::cast_precision_loss,
                        reason = "as the counts above"
                    )]
                    let (index_f, s_f) = (index as f64, s as f64);
                    Bar {
                        x: index_f * group_width + spacing / 2.0 + s_f * width,
                        width,
                        // **A deliberate divergence.** v1 divides by
                        // `Math.max(...)`, so an all-zero chart divides zero
                        // by zero; `NaN` reaches layout as an absent height
                        // and the chart draws *nothing*, which reads as a
                        // broken renderer rather than as an empty chart.
                        height: if max_value == 0.0 {
                            0.0
                        } else {
                            values
                                .get(s)
                                .and_then(|row| row.get(index))
                                .copied()
                                .unwrap_or(0.0)
                                / max_value
                        },
                    }
                })
                .collect()
        })
        .collect())
}

/// The space a pie is drawn in, before it is scaled into its box.
const PIE_SPACE: f64 = 100.0;

/// Where one slice begins and ends, in radians clockwise from twelve.
#[must_use]
pub fn slice_angles(values: &[f64]) -> Vec<(f64, f64)> {
    let total: f64 = values.iter().sum();
    // v1 starts at `-PI / 2` -- twelve o'clock -- and sweeps clockwise.
    let mut cursor = -std::f64::consts::PI / 2.0;
    values
        .iter()
        .map(|value| {
            // A total of zero has no angles to divide: every slice is empty
            // rather than `NaN`, for the same reason a zero maximum gives a
            // zero height.
            let sweep = if total == 0.0 {
                0.0
            } else {
                (value / total) * std::f64::consts::PI * 2.0
            };
            let slice = (cursor, cursor + sweep);
            cursor += sweep;
            slice
        })
        .collect()
}

/// One slice as SVG path data, in the pie's own hundred-unit space.
///
/// **The string is matched, not the numbers.** Four decimals is what makes two
/// independently-computed paths comparable at all, and a path built by a
/// different route -- different trailing zeros, a different separator --
/// diverges in bytes with no numeric difference behind it. This mirrors
/// `chart.ts` exactly: four decimals on every computed coordinate, and the
/// centre printed bare because it is a constant rather than a computation.
#[must_use]
pub fn slice_path(start: f64, end: f64, outer: f64, inner: f64) -> String {
    let centre = PIE_SPACE / 2.0;
    let at = |radius: f64, angle: f64| {
        format!(
            "{:.4} {:.4}",
            centre + angle.cos() * radius,
            centre + angle.sin() * radius
        )
    };
    // A sweep past half a turn needs SVG's large-arc flag, or the renderer
    // draws the short way round and a 300-degree slice comes out as 60.
    let large = i32::from(end - start > std::f64::consts::PI);

    if inner <= 0.0 {
        return format!(
            "M {centre} {centre} L {} A {outer} {outer} 0 {large} 1 {} Z",
            at(outer, start),
            at(outer, end)
        );
    }
    format!(
        "M {} A {outer} {outer} 0 {large} 1 {} L {} A {inner} {inner} 0 {large} 0 {} Z",
        at(outer, start),
        at(outer, end),
        at(inner, end),
        at(inner, start)
    )
}
