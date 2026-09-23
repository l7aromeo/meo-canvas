//! The shape everything samplable shares.

use crate::Error;

/// A motion that can be asked for its value at a time, and for how long it
/// runs.
///
/// One shape for a track, a sequence and a group, so a caller holding any of
/// them can ask the same three questions. It is the JavaScript surface's
/// `Sampled<T>`, which `track`, `sequence` and `parallel` all return.
///
/// The inherent methods are what a caller reaches first. The trait is for code
/// generic over what it animates, and it makes a method missing from one of
/// the three a compile error.
///
/// It is open, so a caller's own motion can implement it. A method added here
/// therefore needs a provided body, as `total_duration` has, or it breaks
/// every implementor outside this crate.
///
/// ```
/// use meo_canvas_core::animate::{
///     easing::Easing,
///     sampled::Sampled,
///     track::{Motion, Track},
/// };
///
/// /// How long a row of `count` of these takes, whatever it is.
/// fn row_length<M: Sampled>(
///     motion: &M,
///     count: usize,
/// ) -> Result<f64, meo_canvas_core::Error> {
///     motion.total_duration(count)
/// }
///
/// let slide = Track {
///     from: 0.0,
///     to: 1.0,
///     duration: Some(1.0),
///     delay: 0.0,
///     stagger: 0.25,
///     motion: Motion::Ease(Easing::Linear),
/// };
///
/// assert_eq!(row_length(&slide, 3)?, 1.5);
/// # Ok::<(), meo_canvas_core::Error>(())
/// ```
pub trait Sampled {
    /// What sampling it gives back: one value for a track or a sequence, and
    /// one per member for a group.
    type Value;

    /// The value at `seconds`, for the `index`th of a staggered set.
    ///
    /// # Errors
    ///
    /// Whatever the underlying motion reports for parameters that do not
    /// describe one.
    fn at(&self, seconds: f64, index: usize) -> Result<Self::Value, Error>;

    /// How long one of these runs for, delay included.
    ///
    /// # Errors
    ///
    /// As [`Sampled::at`].
    fn duration(&self) -> Result<f64, Error>;

    /// How long a staggered set of `count` of these runs for.
    ///
    /// A `count` of zero is the same length as a count of one: **none stated
    /// means one**, and a length of zero would read as finished to every
    /// caller that checks.
    ///
    /// The count is a `usize`, where the JavaScript surface's
    /// `totalDuration(2.5)` answers unfloored: a count of things is an integer
    /// here, as a refusal is a `Result` here and a throw there. The shape
    /// differs; the capability does not.
    ///
    /// The provided body is the answer for a motion that does not stagger: a
    /// set of them is as long as one of them. A type that staggers overrides
    /// it.
    ///
    /// # Errors
    ///
    /// As [`Sampled::at`].
    fn total_duration(&self, count: usize) -> Result<f64, Error> {
        let _ = count;
        self.duration()
    }
}

#[cfg(test)]
#[expect(
    clippy::float_cmp,
    reason = "every expected value here is a whole number or a half, exact in \
              binary, and the exact comparison is the assertion"
)]
mod tests {
    use super::Sampled;
    use crate::{
        Error,
        animate::{
            easing::Easing,
            track::{Motion, Track},
        },
    };

    /// A motion of a caller's own, the implementor a required fourth method
    /// would break.
    struct Blink {
        seconds: f64,
    }

    impl Sampled for Blink {
        type Value = f64;

        fn at(&self, seconds: f64, _index: usize) -> Result<f64, Error> {
            Ok(if seconds.rem_euclid(self.seconds * 2.0) < self.seconds {
                1.0
            } else {
                0.0
            })
        }

        fn duration(&self) -> Result<f64, Error> {
            Ok(self.seconds * 2.0)
        }
    }

    #[test]
    fn an_outside_motion_needs_two_methods_and_gets_the_third() {
        // `Blink` implements `at` and `duration` and not `total_duration`, so
        // that it compiles is the assertion that the third is provided.
        let blink = Blink { seconds: 0.5 };
        assert_eq!(
            blink
                .duration()
                .unwrap_or_else(|error| unreachable!("{error}")),
            1.0
        );
        // The default is "a set of these is as long as one of them", which is
        // right for a motion that does not stagger.
        assert_eq!(
            blink
                .total_duration(9)
                .unwrap_or_else(|error| unreachable!("{error}")),
            1.0
        );
    }

    #[test]
    fn a_motion_that_staggers_overrides_it() {
        // The control for the test above: `Track` overrides the default and
        // still lengthens with the count.
        let staggered = Track {
            from: 0.0,
            to: 1.0,
            duration: Some(1.0),
            delay: 0.0,
            stagger: 0.5,
            motion: Motion::Ease(Easing::Linear),
        };
        assert_eq!(
            Sampled::total_duration(&staggered, 3)
                .unwrap_or_else(|error| unreachable!("{error}")),
            2.0
        );
    }
}
