//! Several motions one after another, and what a group of them lasts.

#![expect(
    clippy::suboptimal_flops,
    reason = "compared bit-for-bit against v1's own numbers; see \
              `animate::easing` for the rule and where it does not apply."
)]

use crate::{
    Error,
    animate::{interpolate::Animatable, track::Motion},
};

/// One leg of a sequence: where to go, how long to take, and how long to wait
/// afterwards.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Step<T> {
    /// Where this leg ends. It starts wherever the last one finished.
    pub to: T,
    /// How long the motion takes. `None` asks a spring for its settling time.
    pub duration: Option<f64>,
    /// What carries it.
    pub motion: Motion,
    /// Stillness after the motion, before the next leg begins.
    pub hold: f64,
}

/// A run of steps from one starting value.
#[derive(Debug, Clone, PartialEq)]
pub struct Sequence<T> {
    /// Where the whole run starts.
    pub from: T,
    /// The legs, in order.
    pub steps: Vec<Step<T>>,
    /// Seconds before the first leg.
    pub delay: f64,
    /// Extra delay per index, so a row of things can run in turn.
    pub stagger: f64,
}

/// One leg with its timing worked out.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Leg<T> {
    from: T,
    to: T,
    start: f64,
    duration: f64,
    motion: Motion,
}

/// A sequence whose timing has been resolved and checked.
///
/// **Planned once rather than per sample.** v1 validates while building and
/// throws there, then samples cheaply; the same split here means
/// [`Plan::at`] cannot fail, so a caller sampling a hundred pages checks the
/// arithmetic once rather than a hundred times.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan<T> {
    from: T,
    legs: Vec<Leg<T>>,
    delay: f64,
    stagger: f64,
    end: f64,
}

impl<T: Animatable> Sequence<T> {
    /// Works out when each leg runs, refusing a sequence that does not describe
    /// a motion.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Track`] for an empty sequence, a negative delay,
    /// stagger, duration or hold, or an eased step with no duration -- a curve
    /// has no length of its own where a spring does.
    pub fn plan(&self) -> Result<Plan<T>, Error> {
        if self.steps.is_empty() {
            return Err(Error::Track("sequence needs at least one step"));
        }
        if self.delay < 0.0 {
            return Err(Error::Track("delay cannot be negative"));
        }
        if self.stagger < 0.0 {
            return Err(Error::Track("stagger cannot be negative"));
        }

        let mut legs = Vec::with_capacity(self.steps.len());
        let mut cursor = self.delay;
        let mut previous = self.from;

        for step in &self.steps {
            if step.hold < 0.0 {
                return Err(Error::Track("a step's hold cannot be negative"));
            }
            let duration = match (step.duration, step.motion) {
                (Some(seconds), _) => seconds,
                (None, Motion::Spring(shape)) => {
                    shape.over(0.0, 1.0).settles_after(
                        crate::animate::spring::DEFAULT_REST_DELTA,
                    )?
                }
                (None, Motion::Ease(_)) => {
                    return Err(Error::Track(
                        "an eased step needs a duration in seconds",
                    ));
                }
            };
            if duration < 0.0 {
                return Err(Error::Track(
                    "a step's duration cannot be negative",
                ));
            }

            legs.push(Leg {
                from: previous,
                to: step.to,
                start: cursor,
                duration,
                motion: step.motion,
            });
            // The hold sits after the motion, so the next leg begins once the
            // rest is over.
            cursor += duration + step.hold;
            previous = step.to;
        }

        let last = legs[legs.len() - 1];
        Ok(Plan {
            from: self.from,
            // **The trailing hold is not part of the length**: nothing moves
            // during it, so counting it would pad every render that sizes
            // itself from the duration.
            end: last.start + last.duration,
            legs,
            delay: self.delay,
            stagger: self.stagger,
        })
    }
}

impl<T: Animatable> Plan<T> {
    /// The value at `seconds`, for the `index`th of a staggered set.
    #[must_use]
    pub fn at(&self, seconds: f64, index: usize) -> T {
        #[expect(
            clippy::cast_precision_loss,
            reason = "an index past 2^53 would be a staggered set larger than \
                      the arena can hold"
        )]
        let elapsed = seconds - self.stagger * index as f64;

        if elapsed <= self.delay {
            return self.from;
        }
        let last = self.legs[self.legs.len() - 1];
        if elapsed >= self.end {
            return last.to;
        }

        // A linear scan rather than a search: a sequence is a handful of legs,
        // and the scan keeps both boundary rules -- holding before a leg,
        // moving during one -- in one readable place.
        for leg in &self.legs {
            if elapsed < leg.start {
                return leg.from;
            }
            if elapsed >= leg.start + leg.duration {
                continue;
            }
            let local = elapsed - leg.start;
            return match leg.motion {
                Motion::Spring(shape) => shape
                    .over(0.0, 1.0)
                    .at(local)
                    .map_or(leg.to, |unit| leg.from.mix(leg.to, unit)),
                Motion::Ease(ease) => {
                    let t = if leg.duration == 0.0 {
                        1.0
                    } else {
                        local / leg.duration
                    };
                    leg.from.mix(leg.to, ease.at(t))
                }
            };
        }
        last.to
    }

    /// How long one member of the set runs for.
    #[must_use]
    pub const fn duration(&self) -> f64 {
        self.end
    }

    /// How long a staggered set of `count` of these runs for.
    #[must_use]
    pub const fn total_duration(&self, count: usize) -> f64 {
        #[expect(clippy::cast_precision_loss, reason = "as `Plan::at`")]
        let last = count.saturating_sub(1) as f64;
        self.end + self.stagger * last
    }
}

/// How long a group of motions started together lasts.
///
/// **This is what survives of v1's `parallel`.** There, a group is a record of
/// named members whose `at` returns a record of their values -- and that
/// record is a *type*, assembled by TypeScript's mapped types from whatever
/// was passed in. Rust has no need of it: a caller with three tracks writes a
/// struct with three fields and calls each, which is what the mapped type was
/// reconstructing. **What does not fall out for free is the timing**, because
/// the members have different value types and only their durations are
/// comparable.
///
/// So the group is over when its longest member is. Returns `None` for an
/// empty group, which v1 refuses outright -- **a group of nothing has no
/// duration rather than a duration of zero**, and zero would read as
/// "finished" to every caller that checks.
#[must_use]
pub fn longest(durations: &[f64]) -> Option<f64> {
    durations.iter().copied().reduce(f64::max)
}

#[cfg(test)]
mod tests {
    use super::{Sequence, Step, longest};
    use crate::{
        Error,
        animate::{easing::Easing, spring::Shape, track::Motion},
    };

    /// A step that eases to `to` over a second, holding for `hold` after.
    fn step(to: f64, hold: f64) -> Step<f64> {
        Step {
            to,
            duration: Some(1.0),
            motion: Motion::Ease(Easing::Linear),
            hold,
        }
    }

    fn two_legs() -> Sequence<f64> {
        Sequence {
            from: 0.0,
            steps: vec![step(10.0, 1.0), step(20.0, 0.0)],
            delay: 0.0,
            stagger: 0.0,
        }
    }

    #[test]
    fn a_hold_sits_between_the_legs_and_stills_the_value() {
        let plan = two_legs()
            .plan()
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert!((plan.at(0.5, 0) - 5.0).abs() < f64::EPSILON);
        // The first leg lands at 1s and the second does not start until 2s.
        assert!((plan.at(1.0, 0) - 10.0).abs() < f64::EPSILON);
        assert!((plan.at(1.5, 0) - 10.0).abs() < f64::EPSILON);
        assert!((plan.at(2.5, 0) - 15.0).abs() < f64::EPSILON);
        assert!((plan.at(3.0, 0) - 20.0).abs() < f64::EPSILON);
    }

    #[test]
    fn a_trailing_hold_is_not_part_of_the_length() {
        // Nothing moves during it, so counting it would pad every render that
        // sizes itself from the duration.
        let padded = Sequence {
            steps: vec![step(10.0, 5.0)],
            ..two_legs()
        };
        let plan = padded
            .plan()
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert!((plan.duration() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn a_staggered_set_runs_longer_than_one_of_it() {
        let plan = Sequence {
            stagger: 0.25,
            ..two_legs()
        }
        .plan()
        .unwrap_or_else(|error| unreachable!("{error}"));
        assert!((plan.duration() - 3.0).abs() < f64::EPSILON);
        assert!((plan.total_duration(1) - 3.0).abs() < f64::EPSILON);
        assert!((plan.total_duration(5) - 4.0).abs() < f64::EPSILON);
        // A count of zero is a set with nothing in it rather than a negative
        // stagger applied backwards.
        assert!((plan.total_duration(0) - 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn delay_holds_the_start_value_and_shifts_everything() {
        let plan = Sequence {
            delay: 2.0,
            ..two_legs()
        }
        .plan()
        .unwrap_or_else(|error| unreachable!("{error}"));
        assert!((plan.at(1.9, 0)).abs() < f64::EPSILON);
        assert!((plan.at(2.5, 0) - 5.0).abs() < f64::EPSILON);
        assert!((plan.duration() - 5.0).abs() < f64::EPSILON);
    }

    #[test]
    fn a_spring_leg_takes_its_length_from_the_physics() {
        let plan = Sequence {
            steps: vec![Step {
                to: 10.0,
                duration: None,
                motion: Motion::Spring(Shape::default()),
                hold: 0.0,
            }],
            ..two_legs()
        }
        .plan()
        .unwrap_or_else(|error| unreachable!("{error}"));
        assert!(plan.duration() > 0.0);
        assert!(
            (plan.at(plan.duration() + 1.0, 0) - 10.0).abs() < f64::EPSILON
        );
    }

    #[test]
    fn a_zero_duration_leg_lands_rather_than_dividing_by_nothing() {
        let plan = Sequence {
            steps: vec![
                Step {
                    duration: Some(0.0),
                    ..step(10.0, 1.0)
                },
                step(20.0, 0.0),
            ],
            ..two_legs()
        }
        .plan()
        .unwrap_or_else(|error| unreachable!("{error}"));
        assert!(
            plan.at(0.0, 0).is_finite(),
            "a zero-length leg divided by zero"
        );
        assert!((plan.at(1.5, 0) - 15.0).abs() < f64::EPSILON);
    }

    #[test]
    fn a_sequence_that_is_not_a_motion_is_refused() {
        let empty = Sequence {
            steps: Vec::new(),
            ..two_legs()
        };
        assert!(matches!(empty.plan(), Err(Error::Track(_))));
        assert!(matches!(
            Sequence {
                delay: -1.0,
                ..two_legs()
            }
            .plan(),
            Err(Error::Track(_))
        ));
        assert!(matches!(
            Sequence {
                stagger: -1.0,
                ..two_legs()
            }
            .plan(),
            Err(Error::Track(_))
        ));
        assert!(matches!(
            Sequence {
                steps: vec![step(1.0, -1.0)],
                ..two_legs()
            }
            .plan(),
            Err(Error::Track(_))
        ));
        assert!(matches!(
            Sequence {
                steps: vec![Step {
                    duration: Some(-1.0),
                    ..step(1.0, 0.0)
                }],
                ..two_legs()
            }
            .plan(),
            Err(Error::Track(_))
        ));
        assert!(matches!(
            Sequence {
                steps: vec![Step {
                    duration: None,
                    ..step(1.0, 0.0)
                }],
                ..two_legs()
            }
            .plan(),
            Err(Error::Track(_))
        ));
    }

    #[test]
    fn a_group_of_nothing_has_no_duration() {
        // Not zero: zero reads as finished to every caller that checks.
        assert!(longest(&[]).is_none());
        assert!(
            (longest(&[1.0, 3.5, 2.0]).unwrap_or_else(|| unreachable!(
                "a non-empty group has a duration"
            )) - 3.5)
                .abs()
                < f64::EPSILON
        );
    }
}
