//! A value over time: a range, a duration, and a curve to cross it with.

#![expect(
    clippy::suboptimal_flops,
    reason = "compared bit-for-bit against v1's own numbers; see \
              `animate::easing` for the rule and where it does not apply."
)]

use crate::{
    Error,
    animate::{easing::Easing, interpolate::Animatable, spring::Shape},
    encode::EncodeOptions,
};

/// What carries a track from one end of its range to the other.
///
/// **An enum rather than two optional fields.** v1 takes `ease` and `spring`
/// separately and raises when both are given, because a spring carries its own
/// curve and an easing would have nothing to apply. Here they are alternatives
/// in the type and the error cannot be written.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Motion {
    /// A timing curve over the track's own duration.
    Ease(Easing),
    /// Physics, solved over 0..1 and mapped onto the range.
    Spring(Shape),
}

/// A value animated from one end of a range to the other.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Track<T> {
    /// Where the value starts.
    pub from: T,
    /// Where it ends.
    pub to: T,
    /// How long the crossing takes, in seconds. `None` asks a spring for its
    /// own settling time, and is an error for an easing, which has none.
    pub duration: Option<f64>,
    /// Seconds before the motion starts.
    pub delay: f64,
    /// Extra delay per index, so a row of things can start in turn.
    pub stagger: f64,
    /// What carries it across.
    pub motion: Motion,
}

impl<T: Animatable> Track<T> {
    /// The value at `seconds`, for the `index`th of a staggered set.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Track`] for a negative delay, stagger or duration, and
    /// for an easing with no duration -- **a curve has no length of its own**,
    /// where a spring does. A spring's own parameters are checked too.
    pub fn at(&self, seconds: f64, index: usize) -> Result<T, Error> {
        if self.delay < 0.0 {
            return Err(Error::Track("delay cannot be negative"));
        }
        if self.stagger < 0.0 {
            return Err(Error::Track("stagger cannot be negative"));
        }
        let duration = self.duration()?;

        #[expect(
            clippy::cast_precision_loss,
            reason = "an index past 2^53 would have to be a staggered set \
                      larger than the arena can hold"
        )]
        let elapsed = seconds - self.delay - self.stagger * index as f64;

        // Finished is checked first so a zero-duration track reads as
        // instantaneous rather than as never having started: both conditions
        // are true at once when the duration is zero.
        if elapsed >= duration {
            return Ok(self.to);
        }
        if elapsed <= 0.0 {
            return Ok(self.from);
        }

        match self.motion {
            // The spring is solved over its own 0..1 and mapped onto the
            // endpoints, so the range stays the track's business and the
            // physics stays independent of the units.
            Motion::Spring(shape) => {
                Ok(self.from.mix(self.to, shape.over(0.0, 1.0).at(elapsed)?))
            }
            Motion::Ease(ease) => {
                Ok(self.from.mix(self.to, ease.at(elapsed / duration)))
            }
        }
    }

    /// How long this track runs for.
    ///
    /// **A spring settles rather than ending**, so its duration comes from the
    /// physics unless the caller states one.
    ///
    /// # Errors
    ///
    /// As [`Track::at`].
    pub fn duration(&self) -> Result<f64, Error> {
        let duration = match (self.duration, self.motion) {
            (Some(seconds), _) => seconds,
            (None, Motion::Spring(shape)) => shape
                .over(0.0, 1.0)
                .settles_after(crate::animate::spring::DEFAULT_REST_DELTA)?,
            (None, Motion::Ease(_)) => {
                return Err(Error::Track(
                    "an eased track needs a duration in seconds",
                ));
            }
        };
        if duration < 0.0 {
            return Err(Error::Track("duration cannot be negative"));
        }
        Ok(duration)
    }
}

/// When a page is shown, in seconds from the start of the animation.
///
/// **v1 handed every animated value a `PageInfo` carrying a clock. v2 has no
/// clock in the scene, and does not need one**: frame timing is already on the
/// wire, in the options that make an animated file animated at all. So a
/// page's time is derived from what the caller has already supplied rather
/// than stored twice.
///
/// `frame_delays` wins over `fps`, as it does at encoding time, and a page's
/// time is the sum of the delays before it. Returns `None` when neither is
/// set, which is the honest answer for a scene with no timing: **a still
/// format has no page times, and inventing zero would animate everything at
/// once.**
#[must_use]
pub fn page_time(options: &EncodeOptions, index: usize) -> Option<f64> {
    if !options.frame_delays.is_empty() {
        let milliseconds: u64 = options
            .frame_delays
            .iter()
            .take(index)
            .map(|&delay| u64::from(delay))
            .sum();
        // Exact: a sum of milliseconds reaches 2^53 after nearly 300,000
        // years, and every value below it is representable.
        #[expect(clippy::cast_precision_loss, reason = "see above")]
        return Some(milliseconds as f64 / 1000.0);
    }
    let fps = options.fps?;
    if fps <= 0.0 {
        return None;
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "an index past 2^53 would exceed the arena's page limit"
    )]
    Some(index as f64 / f64::from(fps))
}

#[cfg(test)]
mod tests {
    use super::{Motion, Track, page_time};
    use crate::{
        Error,
        animate::{easing::Easing, spring::Shape},
        encode::EncodeOptions,
    };

    /// A one-second linear crossing from 0 to 10.
    fn linear() -> Track<f64> {
        Track {
            from: 0.0,
            to: 10.0,
            duration: Some(1.0),
            delay: 0.0,
            stagger: 0.0,
            motion: Motion::Ease(Easing::Linear),
        }
    }

    #[test]
    fn a_track_holds_at_both_ends() {
        let track = linear();
        assert!(
            (track
                .at(-1.0, 0)
                .unwrap_or_else(|error| unreachable!("{error}")))
            .abs()
                < f64::EPSILON
        );
        assert!(
            (track
                .at(0.5, 0)
                .unwrap_or_else(|error| unreachable!("{error}"))
                - 5.0)
                .abs()
                < f64::EPSILON
        );
        assert!(
            (track
                .at(9.0, 0)
                .unwrap_or_else(|error| unreachable!("{error}"))
                - 10.0)
                .abs()
                < f64::EPSILON
        );
    }

    #[test]
    fn a_zero_duration_track_is_instantaneous_rather_than_never_started() {
        // Both conditions are true at once when the duration is zero, so the
        // order they are checked in is the behaviour.
        let track = Track {
            duration: Some(0.0),
            ..linear()
        };
        assert!(
            (track
                .at(0.0, 0)
                .unwrap_or_else(|error| unreachable!("{error}"))
                - 10.0)
                .abs()
                < f64::EPSILON
        );
    }

    #[test]
    fn delay_and_stagger_shift_the_start() {
        let track = Track {
            delay: 1.0,
            stagger: 0.5,
            ..linear()
        };
        // The first member starts at 1s, the third half a second later each.
        assert!(
            (track
                .at(1.0, 0)
                .unwrap_or_else(|error| unreachable!("{error}")))
            .abs()
                < f64::EPSILON
        );
        assert!(
            (track
                .at(1.5, 0)
                .unwrap_or_else(|error| unreachable!("{error}"))
                - 5.0)
                .abs()
                < f64::EPSILON
        );
        assert!(
            (track
                .at(1.5, 1)
                .unwrap_or_else(|error| unreachable!("{error}")))
            .abs()
                < f64::EPSILON
        );
        assert!(
            (track
                .at(2.5, 2)
                .unwrap_or_else(|error| unreachable!("{error}"))
                - 5.0)
                .abs()
                < f64::EPSILON
        );
    }

    #[test]
    fn a_spring_track_takes_its_length_from_the_physics() {
        let track = Track {
            duration: None,
            motion: Motion::Spring(Shape::default()),
            ..linear()
        };
        let seconds = track
            .duration()
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert!(seconds > 0.0, "a spring settled in no time at all");
        // And it ends where it was told to, not where the physics drifts.
        assert!(
            (track
                .at(seconds + 1.0, 0)
                .unwrap_or_else(|error| unreachable!("{error}"))
                - 10.0)
                .abs()
                < f64::EPSILON
        );
    }

    #[test]
    fn an_eased_track_without_a_duration_is_refused() {
        // A curve has no length of its own, where a spring does.
        let track = Track {
            duration: None,
            ..linear()
        };
        assert!(matches!(track.duration(), Err(Error::Track(_))));
        assert!(matches!(track.at(0.5, 0), Err(Error::Track(_))));
    }

    #[test]
    fn negative_timings_are_refused() {
        assert!(matches!(
            Track {
                delay: -1.0,
                ..linear()
            }
            .at(0.0, 0),
            Err(Error::Track(_))
        ));
        assert!(matches!(
            Track {
                stagger: -1.0,
                ..linear()
            }
            .at(0.0, 0),
            Err(Error::Track(_))
        ));
        assert!(matches!(
            Track {
                duration: Some(-1.0),
                ..linear()
            }
            .duration(),
            Err(Error::Track(_))
        ));
    }

    #[test]
    fn a_page_takes_its_time_from_the_encoding_options() {
        let fps = EncodeOptions {
            fps: Some(25.0),
            ..EncodeOptions::default()
        };
        assert!(
            (page_time(&fps, 0)
                .unwrap_or_else(|| unreachable!("fps gives a time")))
            .abs()
                < f64::EPSILON
        );
        assert!(
            (page_time(&fps, 5)
                .unwrap_or_else(|| unreachable!("fps gives a time"))
                - 0.2)
                .abs()
                < f64::EPSILON
        );

        // Delays win over fps, as they do at encoding time, and a page's time
        // is the sum of what came before it rather than its own duration.
        let delays = EncodeOptions {
            fps: Some(25.0),
            frame_delays: vec![100, 200, 400],
            ..EncodeOptions::default()
        };
        assert!(
            (page_time(&delays, 0)
                .unwrap_or_else(|| unreachable!("delays give a time")))
            .abs()
                < f64::EPSILON
        );
        assert!(
            (page_time(&delays, 2)
                .unwrap_or_else(|| unreachable!("delays give a time"))
                - 0.3)
                .abs()
                < f64::EPSILON
        );
    }

    #[test]
    fn a_scene_with_no_timing_has_no_page_times() {
        // Zero would read as "every page at once" rather than as "no clock".
        assert!(page_time(&EncodeOptions::default(), 3).is_none());
        let stopped = EncodeOptions {
            fps: Some(0.0),
            ..EncodeOptions::default()
        };
        assert!(page_time(&stopped, 3).is_none());
    }
}
