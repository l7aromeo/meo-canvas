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
    /// # Errors
    ///
    /// As [`Sampled::at`].
    fn total_duration(&self, count: usize) -> Result<f64, Error>;
}
