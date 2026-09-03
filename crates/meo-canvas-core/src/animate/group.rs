//! Motions that run together, and what the group of them lasts.

use crate::{
    Error,
    animate::{
        interpolate::Animatable,
        sampled::Sampled,
        sequence::{Plan, longest},
        track::Track,
    },
};

/// One thing running inside a [`Parallel`].
///
/// **An enum rather than a boxed trait object.** The three samplable types are
/// the three there are, the crate already spells this choice as an enum in
/// [`Motion`](crate::animate::track::Motion), and it keeps `Parallel` `Clone`,
/// `Debug` and `PartialEq` where a `dyn` member would not be.
#[derive(Debug, Clone, PartialEq)]
pub enum Member<T> {
    /// A single motion between two values.
    Track(Track<T>),
    /// A run of motions, already planned.
    Sequence(Plan<T>),
    /// Another group, so groups nest.
    Group(Parallel<T>),
}

/// Several motions started together, sampled as one.
///
/// **What v1's `parallel` is, in a language with no mapped types.** There a
/// group is a record of named members and `at` returns a record of their
/// values, assembled by TypeScript from whatever was passed in. Rust cannot
/// build that type, so the values come back in declaration order and
/// [`Parallel::names`] gives the names in the same order. **The numbers match
/// the JavaScript surface exactly; the container does not, and cannot.**
///
/// Until 4 September 2026 this crate had no group at all, only
/// [`longest`], on the argument that a caller with three tracks writes a
/// struct with three fields and calls each. That argument was wrong about the
/// timing -- which is why `longest` existed -- and the user overruled it in
/// the animation audit: the two surfaces now offer the same three operations
/// on the same three things.
///
/// ```
/// use meo_canvas_core::animate::{
///     easing::Easing,
///     group::{Member, Parallel},
///     track::{Motion, Track},
/// };
///
/// let bar = |to: f64, seconds: f64| Track {
///     from: 0.0,
///     to,
///     duration: Some(seconds),
///     delay: 0.0,
///     stagger: 0.0,
///     motion: Motion::Ease(Easing::Linear),
/// };
///
/// let group = Parallel::new(vec![
///     ("left".to_owned(), Member::Track(bar(100.0, 1.0))),
///     ("right".to_owned(), Member::Track(bar(50.0, 2.0))),
/// ])?;
///
/// // Halfway through the first second: the short bar is half done, the long
/// // one a quarter.
/// assert_eq!(group.at(0.5, 0)?, vec![50.0, 12.5]);
/// // The group is over when its longest member is.
/// assert_eq!(group.duration()?, 2.0);
/// # Ok::<(), meo_canvas_core::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Parallel<T> {
    /// The members, in the order the caller gave them.
    members: Vec<(String, Member<T>)>,
}

impl<T: Animatable> Parallel<T> {
    /// A group of named members.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Track`] for a group with no members. **A group of
    /// nothing has no duration rather than a duration of zero**, and zero
    /// would read as finished to every caller that checks -- which is why this
    /// refuses rather than answering, as v1 does.
    ///
    /// ```
    /// use meo_canvas_core::animate::group::{Member, Parallel};
    ///
    /// let empty: Result<Parallel<f64>, _> = Parallel::new(Vec::new());
    /// assert!(empty.is_err());
    /// ```
    pub fn new(members: Vec<(String, Member<T>)>) -> Result<Self, Error> {
        if members.is_empty() {
            return Err(Error::Track(
                "a parallel group needs at least one member",
            ));
        }
        Ok(Self { members })
    }

    /// The members' names, in the order [`Parallel::at`] returns their values.
    ///
    /// A nested group contributes its members rather than itself, named
    /// `outer.inner`, so the names and the values stay in step at any depth.
    ///
    /// ```
    /// use meo_canvas_core::animate::{
    ///     easing::Easing,
    ///     group::{Member, Parallel},
    ///     track::{Motion, Track},
    /// };
    ///
    /// let bar = Track {
    ///     from: 0.0,
    ///     to: 1.0,
    ///     duration: Some(1.0),
    ///     delay: 0.0,
    ///     stagger: 0.0,
    ///     motion: Motion::Ease(Easing::Linear),
    /// };
    /// let inner = Parallel::new(vec![("deep".to_owned(), Member::Track(bar))])?;
    /// let outer = Parallel::new(vec![
    ///     ("here".to_owned(), Member::Track(bar)),
    ///     ("nested".to_owned(), Member::Group(inner)),
    /// ])?;
    ///
    /// assert_eq!(outer.names(), vec!["here", "nested.deep"]);
    /// # Ok::<(), meo_canvas_core::Error>(())
    /// ```
    #[must_use]
    pub fn names(&self) -> Vec<String> {
        let mut names = Vec::new();
        self.write_names("", &mut names);
        names
    }

    /// The names under `prefix`, depth first, in declaration order.
    fn write_names(&self, prefix: &str, out: &mut Vec<String>) {
        for (name, member) in &self.members {
            let path = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}.{name}")
            };
            match member {
                Member::Group(group) => group.write_names(&path, out),
                Member::Track(_) | Member::Sequence(_) => out.push(path),
            }
        }
    }

    /// Every member's value at `seconds`, in declaration order.
    ///
    /// The `index` reaches every member, so a group of staggered tracks
    /// staggers as one.
    ///
    /// # Errors
    ///
    /// Whatever the first member that refuses reports.
    pub fn at(&self, seconds: f64, index: usize) -> Result<Vec<T>, Error> {
        let mut values = Vec::with_capacity(self.members.len());
        self.write_values(seconds, index, &mut values)?;
        Ok(values)
    }

    /// The values, depth first, appended in declaration order.
    fn write_values(
        &self,
        seconds: f64,
        index: usize,
        out: &mut Vec<T>,
    ) -> Result<(), Error> {
        for (_, member) in &self.members {
            match member {
                Member::Track(track) => out.push(track.at(seconds, index)?),
                Member::Sequence(plan) => out.push(plan.at(seconds, index)),
                Member::Group(group) => {
                    group.write_values(seconds, index, out)?;
                }
            }
        }
        Ok(())
    }

    /// How long the group runs for: **as long as its longest member.**
    ///
    /// # Errors
    ///
    /// Whatever the first member that refuses reports.
    pub fn duration(&self) -> Result<f64, Error> {
        let lengths = self
            .members
            .iter()
            .map(|(_, member)| member.duration())
            .collect::<Result<Vec<_>, _>>()?;
        longest(&lengths)
            .ok_or(Error::Track("a parallel group needs at least one member"))
    }

    /// How long a staggered set of `count` groups runs for.
    ///
    /// **The count reaches each member and the longest answer wins**, rather
    /// than the count being applied to the group's own length. A group whose
    /// members stagger differently is as long as whichever member the stagger
    /// stretches furthest, which is not in general the member that is longest
    /// for a single item.
    ///
    /// # Errors
    ///
    /// Whatever the first member that refuses reports.
    pub fn total_duration(&self, count: usize) -> Result<f64, Error> {
        let lengths = self
            .members
            .iter()
            .map(|(_, member)| member.total_duration(count))
            .collect::<Result<Vec<_>, _>>()?;
        longest(&lengths)
            .ok_or(Error::Track("a parallel group needs at least one member"))
    }
}

impl<T: Animatable> Member<T> {
    /// How long this member runs for.
    ///
    /// Not a [`Sampled`] implementation, deliberately: that trait's `at`
    /// returns one value, and a nested group's returns one per member of it.
    /// A `Member` is what a group is built from rather than something a caller
    /// samples, so it answers only the two questions a group needs of it.
    ///
    /// # Errors
    ///
    /// Whatever the underlying motion reports.
    pub fn duration(&self) -> Result<f64, Error> {
        match self {
            Self::Track(track) => track.duration(),
            Self::Sequence(plan) => Ok(plan.duration()),
            Self::Group(group) => group.duration(),
        }
    }

    /// How long a staggered set of `count` of this member runs for.
    ///
    /// # Errors
    ///
    /// Whatever the underlying motion reports.
    pub fn total_duration(&self, count: usize) -> Result<f64, Error> {
        match self {
            Self::Track(track) => track.total_duration(count),
            Self::Sequence(plan) => Ok(plan.total_duration(count)),
            Self::Group(group) => group.total_duration(count),
        }
    }
}

impl<T: Animatable> Sampled for Parallel<T> {
    type Value = Vec<T>;

    fn at(&self, seconds: f64, index: usize) -> Result<Vec<T>, Error> {
        Self::at(self, seconds, index)
    }

    fn duration(&self) -> Result<f64, Error> {
        Self::duration(self)
    }

    fn total_duration(&self, count: usize) -> Result<f64, Error> {
        Self::total_duration(self, count)
    }
}

#[cfg(test)]
#[expect(
    clippy::float_cmp,
    reason = "every expected value here is exact in binary -- halves, \
              quarters and the eighths `outCubic` lands on -- and each came \
              from v1 through the JavaScript surface. The exact comparison is \
              the assertion, as in `tests/animate_vectors.rs`; an epsilon \
              would hide the disagreement these exist to find."
)]
mod tests {
    use super::{Member, Parallel};
    use crate::animate::{
        easing::Easing,
        sequence::{Sequence, Step},
        track::{Motion, Track},
    };

    /// A linear track from 0 to `to` over `seconds`, staggering by `stagger`.
    fn track(to: f64, seconds: f64, stagger: f64) -> Track<f64> {
        Track {
            from: 0.0,
            to,
            duration: Some(seconds),
            delay: 0.0,
            stagger,
            motion: Motion::Ease(Easing::Linear),
        }
    }

    fn group(members: Vec<(&str, Member<f64>)>) -> Parallel<f64> {
        Parallel::new(
            members
                .into_iter()
                .map(|(name, member)| (name.to_owned(), member))
                .collect(),
        )
        .unwrap_or_else(|error| unreachable!("{error}"))
    }

    #[test]
    fn a_group_samples_where_v1_samples() {
        // Measured from v1 through the JavaScript surface:
        // `parallel({x: track({from:0,to:100,duration:1,ease:'outCubic'}),
        //            y: sequence({from:0,steps:[{to:4,duration:2}]})})`
        // at time 0.25 gives `{x: 57.8125, y: 0.5}`, and its duration is 2 --
        // the longer member.
        let curve = Track {
            motion: Motion::Ease(Easing::OutCubic),
            ..track(100.0, 1.0, 0.0)
        };
        let run = Sequence {
            from: 0.0,
            steps: vec![Step {
                to: 4.0,
                duration: Some(2.0),
                motion: Motion::Ease(Easing::Linear),
                hold: 0.0,
            }],
            delay: 0.0,
            stagger: 0.0,
        }
        .plan()
        .unwrap_or_else(|error| unreachable!("{error}"));

        let both = group(vec![
            ("x", Member::Track(curve)),
            ("y", Member::Sequence(run)),
        ]);

        assert_eq!(
            both.at(0.25, 0)
                .unwrap_or_else(|error| unreachable!("{error}")),
            vec![57.8125, 0.5]
        );
        assert_eq!(
            both.duration()
                .unwrap_or_else(|error| unreachable!("{error}")),
            2.0
        );
    }

    #[test]
    fn a_set_of_groups_takes_the_count_to_each_member() {
        // **The case that separates propagating the count from ignoring it.**
        // A member lasting 1s and staggering by 1s, beside one lasting 2s and
        // not staggering: for a single item the second is longer, and for
        // three items the first is. An implementation that applied the count
        // to the group's own length would answer 2 here and pass every
        // less careful test.
        //
        // Measured from v1 through the JavaScript surface: duration 2,
        // totalDuration(3) 3, totalDuration(5) 5.
        let uneven = group(vec![
            ("staggering", Member::Track(track(5.0, 1.0, 1.0))),
            ("long", Member::Track(track(9.0, 2.0, 0.0))),
        ]);

        let length = |count| {
            uneven
                .total_duration(count)
                .unwrap_or_else(|error| unreachable!("{error}"))
        };
        assert_eq!(
            uneven
                .duration()
                .unwrap_or_else(|error| unreachable!("{error}")),
            2.0,
            "one item: the longer member decides"
        );
        assert_eq!(length(1), 2.0);
        assert_eq!(length(3), 3.0, "three items: the staggering member wins");
        assert_eq!(length(5), 5.0);
    }

    #[test]
    fn a_group_hands_the_index_to_every_member() {
        // Without this a group of staggered tracks would sample every item at
        // the first item's time, and a staggered row would animate as one.
        let staggered =
            group(vec![("bar", Member::Track(track(10.0, 1.0, 0.5)))]);

        assert_eq!(
            staggered
                .at(0.5, 0)
                .unwrap_or_else(|error| unreachable!("{error}")),
            vec![5.0]
        );
        // The second item is half a second behind, so at the same moment it
        // has not started.
        assert_eq!(
            staggered
                .at(0.5, 1)
                .unwrap_or_else(|error| unreachable!("{error}")),
            vec![0.0]
        );
    }

    #[test]
    fn an_empty_group_is_refused_rather_than_answering_zero() {
        let empty: Result<Parallel<f64>, _> = Parallel::new(Vec::new());
        assert!(empty.is_err(), "a group of nothing has no duration");
    }

    #[test]
    fn groups_nest_and_their_names_stay_in_step_with_their_values() {
        let inner = group(vec![
            ("a", Member::Track(track(10.0, 1.0, 0.0))),
            ("b", Member::Track(track(20.0, 1.0, 0.0))),
        ]);
        let outer = group(vec![
            ("top", Member::Track(track(4.0, 1.0, 0.0))),
            ("in", Member::Group(inner)),
        ]);

        assert_eq!(outer.names(), vec!["top", "in.a", "in.b"]);
        assert_eq!(
            outer
                .at(0.5, 0)
                .unwrap_or_else(|error| unreachable!("{error}")),
            vec![2.0, 5.0, 10.0],
            "one value per name, in the same order"
        );
        assert_eq!(outer.names().len(), 3);
    }
}
