//! The shape everything samplable shares.

use crate::Error;

/// A motion that can be asked for its value at a time, and for how long it
/// runs.
///
/// **One shape for a track, a sequence and a group**, so a caller holding any
/// of them can ask the same three questions. The JavaScript surface has had
/// this as its `Sampled<T>` interface since it was written -- `track`,
/// `sequence` and `parallel` all return it -- while Rust had three types with
/// method sets that resembled each other by coincidence: `Track` had `at` and
/// `duration`, `Plan` had those and `total_duration`, and there was no group
/// type at all. That asymmetry was found by the animation audit of 4 September
/// 2026 and closed by this trait.
///
/// The inherent methods stay, and are what a caller reaches first. This exists
/// for code that is generic over what it is animating, and to make the
/// omission of a method from one of the three a compile error rather than
/// something to notice.
///
/// **It is open, so a caller's own motion can implement it**, and that decides
/// how it may grow: a method added here arrives with a provided body, or it
/// breaks every implementor outside this crate. `total_duration` already has
/// one. If a future method genuinely cannot have a sensible default, the
/// honest move is to seal the trait then rather than to add it and hope.
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
    /// **`usize` rather than a float, deliberately** (4 September 2026). The
    /// JavaScript surface's `totalDuration(2.5)` answers unfloored, because a
    /// JavaScript number is what it is rather than because half an item was
    /// designed for. A count of things is an integer here, in the same way
    /// that a refusal is a `Result` here and a throw there: a difference in
    /// the shape, chosen, and not a capability one surface has and the other
    /// lacks.
    ///
    /// # Errors
    ///
    /// As [`Sampled::at`].
    ///
    /// **Provided, and that is deliberate rather than a convenience.** A trait
    /// whose every method is required breaks every implementor outside this
    /// crate the day a fourth is added, and this one is meant to be
    /// implemented outside it: the reason it exists is that a caller can be
    /// generic over what they are animating, which is only worth having if
    /// their own motion can join in.
    /// [`crate::animate::interpolate::Animatable`] is the neighbour with
    /// the same shape, and `Styled` on the facade is the one that gets it
    /// most right -- one required method and sixty-eight provided.
    ///
    /// The default is the answer for a motion that does not stagger: a set of
    /// them is as long as one of them. A type that staggers overrides it.
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

    /// A motion of a caller's own, which is the case the trait exists for and
    /// the case a required fourth method would break.
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
        // **`Blink` implements `at` and `duration` and not `total_duration`.**
        // That it compiles is the assertion: a trait whose every method is
        // required breaks a type like this the day a fourth arrives, and the
        // provided body is what stops that.
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
        // The control for the test above: the default is a default rather than
        // the only answer, and `Track` still lengthens with the count.
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
