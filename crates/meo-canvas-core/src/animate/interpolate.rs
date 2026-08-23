//! Blending values, and reading a keyframe track.

#![expect(
    clippy::suboptimal_flops,
    reason = "compared bit-for-bit against v1's own numbers; see \
              `animate::easing` for the rule and where it does not apply."
)]

use crate::{
    Error,
    animate::{color::Rgba, easing::Easing},
};

/// A value with a midpoint.
///
/// **Typed rather than dispatched.** v1's `mix` inspects what it was handed at
/// run time -- number, colour string, or an array of either -- because
/// JavaScript gives it no other way to be one function. Here the caller's type
/// already says which, so a mismatched pair is a compile error rather than the
/// run-time throw v1 has to raise.
pub trait Animatable: Copy {
    /// This value a fraction of the way to another.
    ///
    /// **Unclamped**, for both implementors: `OutBack` and `OutElastic` return
    /// values beyond 0..1 and clamping here would flatten exactly the
    /// overshoot those curves exist to produce.
    #[must_use]
    fn mix(self, to: Self, t: f64) -> Self;
}

impl Animatable for f64 {
    fn mix(self, to: Self, t: f64) -> Self {
        self + (to - self) * t
    }
}

impl Animatable for Rgba {
    fn mix(self, to: Self, t: f64) -> Self {
        crate::animate::color::mix(self, to, t)
    }
}

/// Rescales a value from one range to another.
///
/// An empty input range has no position to report, so the start of the output
/// range is the answer -- which also avoids handing back a `NaN` that would
/// surface later as an invalid layout value.
#[must_use]
pub fn map_range(
    value: f64,
    from: (f64, f64),
    to: (f64, f64),
    clamp: bool,
) -> f64 {
    let span = from.1 - from.0;
    if span == 0.0 {
        return to.0;
    }
    let mapped = to.0.mix(to.1, (value - from.0) / span);
    if !clamp {
        return mapped;
    }
    mapped.clamp(to.0.min(to.1), to.0.max(to.1))
}

/// Reads a keyframe track: `stops` are the positions, `values` what to be at
/// each.
///
/// **Values hold outside the declared range rather than extrapolating**, which
/// is what a keyframe track means -- the first and last frames are states, not
/// the start of a slope.
///
/// # Errors
///
/// Returns [`Error::Keyframes`] when the two lists differ in length, when
/// there are fewer than two stops, or when the stops are not strictly
/// ascending. **All three are the caller's arithmetic rather than a value out
/// of range**, and a track that silently reordered them would interpolate
/// backwards through a frame nobody wrote.
pub fn keyframes<T: Animatable>(
    t: f64,
    stops: &[f64],
    values: &[T],
    ease: Easing,
) -> Result<T, Error> {
    if stops.len() != values.len() {
        return Err(Error::Keyframes("needs one value per stop"));
    }
    let (Some(&first), Some(&last)) = (stops.first(), stops.last()) else {
        return Err(Error::Keyframes("needs at least two stops"));
    };
    if stops.len() < 2 {
        return Err(Error::Keyframes("needs at least two stops"));
    }
    if stops.windows(2).any(|pair| pair[1] <= pair[0]) {
        return Err(Error::Keyframes("needs stops in ascending order"));
    }

    if t <= first {
        return Ok(values[0]);
    }
    if t >= last {
        return Ok(values[values.len() - 1]);
    }

    let upper = stops
        .iter()
        .position(|&stop| stop > t)
        .unwrap_or(stops.len() - 1);
    let lower = upper - 1;
    let local = (t - stops[lower]) / (stops[upper] - stops[lower]);
    Ok(values[lower].mix(values[upper], ease.at(local)))
}

#[cfg(test)]
mod tests {
    use super::{Animatable, keyframes, map_range};
    use crate::{
        Error,
        animate::{color::Rgba, easing::Easing},
    };

    #[test]
    fn a_number_mixes_past_both_ends() {
        // Unclamped on purpose: `OutBack` returns values beyond 0..1 and
        // clamping here would flatten the overshoot it exists to produce.
        assert!((0.0_f64.mix(10.0, 0.25) - 2.5).abs() < f64::EPSILON);
        assert!((0.0_f64.mix(10.0, 1.5) - 15.0).abs() < f64::EPSILON);
        assert!((0.0_f64.mix(10.0, -0.5) + 5.0).abs() < f64::EPSILON);
    }

    #[test]
    fn a_colour_mixes_through_the_same_trait() {
        let black = Rgba {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        };
        let white = Rgba {
            r: 1.0,
            g: 1.0,
            b: 1.0,
            a: 1.0,
        };
        assert_eq!(black.mix(white, 0.5).to_color().g, 128);
    }

    #[test]
    fn an_empty_input_range_reports_the_output_start() {
        // No position to report, and `NaN` would surface later as an invalid
        // layout value rather than here where it happened.
        assert!(
            (map_range(5.0, (2.0, 2.0), (10.0, 20.0), false) - 10.0).abs()
                < f64::EPSILON
        );
    }

    #[test]
    fn a_range_maps_and_clamps_only_when_asked() {
        assert!(
            (map_range(50.0, (0.0, 100.0), (0.0, 1.0), false) - 0.5).abs()
                < f64::EPSILON
        );
        assert!(
            (map_range(150.0, (0.0, 100.0), (0.0, 1.0), false) - 1.5).abs()
                < f64::EPSILON
        );
        assert!(
            (map_range(150.0, (0.0, 100.0), (0.0, 1.0), true) - 1.0).abs()
                < f64::EPSILON
        );
        // A descending output range clamps to its own ends rather than to the
        // larger and smaller of the two arguments in order.
        assert!(
            (map_range(150.0, (0.0, 100.0), (1.0, 0.0), true)).abs()
                < f64::EPSILON
        );
    }

    #[test]
    fn a_track_holds_outside_its_stops_rather_than_extrapolating() {
        let stops = [0.0, 0.5, 1.0];
        let values = [0.0, 100.0, 0.0];
        let at = |t| keyframes(t, &stops, &values, Easing::Linear);
        assert!(
            (at(-1.0).unwrap_or_else(|error| unreachable!("{error}"))).abs()
                < f64::EPSILON
        );
        assert!(
            (at(2.0).unwrap_or_else(|error| unreachable!("{error}"))).abs()
                < f64::EPSILON
        );
        assert!(
            (at(0.25).unwrap_or_else(|error| unreachable!("{error}")) - 50.0)
                .abs()
                < f64::EPSILON
        );
        assert!(
            (at(0.75).unwrap_or_else(|error| unreachable!("{error}")) - 50.0)
                .abs()
                < f64::EPSILON
        );
    }

    /// A typed empty slice, so the call names its own value type.
    const EMPTY: [f64; 0] = [];

    #[test]
    fn a_track_that_is_not_a_track_is_refused() {
        let two = [0.0, 1.0];
        assert!(matches!(
            keyframes(0.5, &two, &[1.0], Easing::Linear),
            Err(Error::Keyframes(_))
        ));
        assert!(matches!(
            keyframes(0.5, &[0.0], &[1.0], Easing::Linear),
            Err(Error::Keyframes(_))
        ));
        assert!(matches!(
            keyframes(0.5, &[], &EMPTY, Easing::Linear),
            Err(Error::Keyframes(_))
        ));
        // Descending or repeated stops would interpolate backwards through a
        // frame nobody wrote.
        assert!(matches!(
            keyframes(0.5, &[1.0, 0.0], &[0.0, 1.0], Easing::Linear),
            Err(Error::Keyframes(_))
        ));
        assert!(matches!(
            keyframes(0.5, &[0.0, 0.0], &[0.0, 1.0], Easing::Linear),
            Err(Error::Keyframes(_))
        ));
    }

    #[test]
    fn the_easing_applies_within_a_segment_and_not_across_the_track() {
        // Halfway along a segment of an eased track is the curve's midpoint of
        // that segment, not of the whole track.
        let value = keyframes(0.25, &[0.0, 0.5], &[0.0, 1.0], Easing::InQuad)
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert!((value - 0.25).abs() < f64::EPSILON, "got {value}");
    }
}
