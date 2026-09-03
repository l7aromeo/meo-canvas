//! A damped spring, solved in closed form.
//!
//! **Closed form rather than integrated step by step**, which is what makes it
//! usable from a page builder: any page can be evaluated on its own, in any
//! order, and asking twice gives the same answer. A stepwise simulation would
//! need every earlier frame first.
//!
//! The three regimes are genuinely different solutions of the same equation
//! rather than one formula with edge cases -- underdamped motion oscillates,
//! critically damped is the fastest approach that does not, and overdamped
//! crawls in without ever crossing.
//!
//! `f64` and no fused multiply-add, for the reason `animate::easing` gives:
//! this is compared against `tests/assets/animate/spring.tsv` with `==`.

#![expect(
    clippy::neg_cmp_op_on_partial_ord,
    clippy::while_float,
    reason = "`!(stiffness > 0.0)` is deliberate and not `<=`: it refuses a \
              NaN, which every ordinary comparison accepts. And the settle \
              scan walks `t += 1/240` because that is what v1 walks -- see \
              `SETTLE_STEP_SECONDS`."
)]
#![expect(
    clippy::suboptimal_flops,
    reason = "a fused multiply-add rounds once where JavaScript rounds twice, \
              and this module is compared bit-for-bit against v1's own \
              numbers. See `animate::easing` for the rule and where it does \
              not apply."
)]

use crate::Error;

/// The spring v1 defaults to, and the one every caller gets unasked.
pub const DEFAULT_STIFFNESS: f64 = 170.0;
/// Ditto damping: with the default stiffness this is very slightly underdamped.
pub const DEFAULT_DAMPING: f64 = 26.0;
/// Ditto mass.
pub const DEFAULT_MASS: f64 = 1.0;

/// How close to `zeta == 1` counts as critically damped.
///
/// **A band rather than an equality**, because the critical solution is a
/// limit the other two approach: at a damping ratio a hair under one the
/// underdamped form divides by `omega_d`, which is approaching zero, and the
/// arithmetic loses its footing before the physics does.
const CRITICAL_BAND: f64 = 1e-4;

/// A spring's physics, with no range of its own.
///
/// **Split from [`Spring`] because a track supplies the range.** v1 has one
/// object for both and raises at run time when a track is handed a spring
/// carrying `from` or `to` -- it has to, because dropping them silently would
/// animate to a value the caller never asked for while looking obeyed. Here
/// the type says it: a track takes a `Shape`, which has nowhere to put a
/// range, so the mistake cannot be written.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shape {
    /// The spring constant. Stiffer is faster.
    pub stiffness: f64,
    /// Resistance. More damping means less overshoot.
    pub damping: f64,
    /// The mass on the end of it. Heavier is slower.
    pub mass: f64,
    /// The initial velocity, in units per second.
    pub velocity: f64,
}

impl Default for Shape {
    fn default() -> Self {
        Self {
            stiffness: DEFAULT_STIFFNESS,
            damping: DEFAULT_DAMPING,
            mass: DEFAULT_MASS,
            velocity: 0.0,
        }
    }
}

impl Shape {
    /// This physics over a range.
    #[must_use]
    pub const fn over(self, from: f64, to: f64) -> Spring {
        Spring {
            from,
            to,
            stiffness: self.stiffness,
            damping: self.damping,
            mass: self.mass,
            velocity: self.velocity,
        }
    }
}

/// A spring's physical parameters and the range it moves over.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spring {
    /// Where the motion starts.
    pub from: f64,
    /// Where it settles.
    pub to: f64,
    /// The spring constant. Stiffer is faster.
    pub stiffness: f64,
    /// Resistance. More damping means less overshoot.
    pub damping: f64,
    /// The mass on the end of it. Heavier is slower.
    pub mass: f64,
    /// The initial velocity, in units per second.
    pub velocity: f64,
}

impl Default for Spring {
    fn default() -> Self {
        Self {
            from: 0.0,
            to: 1.0,
            stiffness: DEFAULT_STIFFNESS,
            damping: DEFAULT_DAMPING,
            mass: DEFAULT_MASS,
            velocity: 0.0,
        }
    }
}

impl Spring {
    /// The position at `t` seconds.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Spring`] for a stiffness or mass that is not positive,
    /// or a negative damping. **A spring with no stiffness has no equation**
    /// rather than a degenerate one: `omega0` would be zero and every regime
    /// divides by it or by something derived from it.
    pub fn at(self, t: f64) -> Result<f64, Error> {
        let (omega0, zeta) = self.resolved()?;

        // Before the motion starts there is nothing to report but the start.
        if t <= 0.0 {
            return Ok(self.from);
        }

        // Displacement is measured from the target, so it decays to zero and
        // the arithmetic stays symmetric.
        let x0 = -(self.to - self.from);
        let v0 = self.velocity;

        let displacement = if (zeta - 1.0).abs() < CRITICAL_BAND {
            let decay = (-omega0 * t).exp();
            (x0 + (v0 + omega0 * x0) * t) * decay
        } else if zeta < 1.0 {
            let omega_d = omega0 * (1.0 - zeta * zeta).sqrt();
            let decay = (-zeta * omega0 * t).exp();
            decay
                * (x0 * (omega_d * t).cos()
                    + ((v0 + zeta * omega0 * x0) / omega_d)
                        * (omega_d * t).sin())
        } else {
            let rate = omega0 * (zeta * zeta - 1.0).sqrt();
            let slow = -zeta * omega0 + rate;
            let fast = -zeta * omega0 - rate;
            let c2 = (v0 - slow * x0) / (fast - slow);
            let c1 = x0 - c2;
            c1 * (slow * t).exp() + c2 * (fast * t).exp()
        };

        Ok(self.to + displacement)
    }

    /// The undamped angular frequency and the damping ratio, which between
    /// them decide which of the three regimes the spring is in.
    fn resolved(self) -> Result<(f64, f64), Error> {
        if !(self.stiffness > 0.0) {
            return Err(Error::Spring("stiffness must be greater than 0"));
        }
        if !(self.damping >= 0.0) {
            return Err(Error::Spring("damping cannot be negative"));
        }
        if !(self.mass > 0.0) {
            return Err(Error::Spring("mass must be greater than 0"));
        }
        Ok((
            (self.stiffness / self.mass).sqrt(),
            self.damping / (2.0 * (self.stiffness * self.mass).sqrt()),
        ))
    }
}

/// The fraction of the travel a spring must be within to count as at rest.
pub const DEFAULT_REST_DELTA: f64 = 0.005;

/// Ceiling on the search, so a barely damped spring cannot run forever.
const MAX_SETTLE_SECONDS: f64 = 100.0;
/// The scan's step. **Accumulated rather than multiplied**: v1 walks
/// `t += 1/240` and repeated addition of a value with no exact binary form
/// does not land where `n / 240` lands. Same arithmetic in the same order is
/// what makes the two surfaces agree.
const SETTLE_STEP_SECONDS: f64 = 1.0 / 240.0;
/// Oscillations a spring must spend inside the threshold to count as finished.
const SETTLE_WINDOW_CYCLES: f64 = 1.0;
/// Samples of quiet required when there is no oscillation to wait out.
const SETTLE_WINDOW_SAMPLES: f64 = 2.0;

impl Spring {
    /// Seconds until this spring has settled.
    ///
    /// **A spring approaches its target asymptotically and has no natural
    /// end**, which is awkward when a page count has to be a number. This
    /// turns the physics into that number.
    ///
    /// Found by walking the closed form rather than solving the envelope,
    /// because the envelope is only an upper bound: an underdamped spring can
    /// sit inside it while still swinging through the target, and the honest
    /// answer is when it stops moving rather than when the bound gets small.
    ///
    /// # Why a window and not a sample
    ///
    /// A single sample near the target proves nothing -- an underdamped spring
    /// passes straight through twice per cycle. **A full oscillation does
    /// prove it**, because the envelope decays monotonically, so a spring that
    /// stayed inside the threshold for one whole cycle can never leave it.
    /// Without oscillation there is no cycle to wait out and two samples do.
    ///
    /// # Errors
    ///
    /// As [`Spring::at`]: a spring whose parameters have no equation.
    pub fn settles_after(self, rest_delta: f64) -> Result<f64, Error> {
        // **A threshold of zero asks when the spring is exactly at rest**,
        // which never happens: the closed form approaches its target and does
        // not arrive. A negative one asks nothing at all. Both walked to
        // `MAX_SETTLE_SECONDS` and answered 2.929 and 100.004 -- numbers that
        // read like measurements and were the scan giving up. `Spring::at`
        // checks its own parameters this way and this did not check its one.
        //
        // Written negated so `NaN` is refused by the same comparison, as the
        // stiffness and mass checks are.
        if !(rest_delta > 0.0) {
            return Err(Error::Spring("rest delta must be greater than 0"));
        }
        let (omega0, zeta) = self.resolved()?;
        let distance = {
            let travel = (self.to - self.from).abs();
            if travel == 0.0 { 1.0 } else { travel }
        };
        let threshold = rest_delta * distance;

        let rest_window = if zeta < 1.0 - CRITICAL_BAND {
            (2.0 * std::f64::consts::PI) / (omega0 * (1.0 - zeta * zeta).sqrt())
                * SETTLE_WINDOW_CYCLES
        } else {
            SETTLE_STEP_SECONDS * SETTLE_WINDOW_SAMPLES
        };

        let mut settled = 0.0;
        let mut t = 0.0;
        while t <= MAX_SETTLE_SECONDS {
            if (self.at(t)? - self.to).abs() > threshold {
                // Still moving at t, so it cannot have been at rest before it.
                settled = t;
            } else if t - settled >= rest_window {
                // At rest for a whole cycle, and the envelope only shrinks.
                break;
            }
            t += SETTLE_STEP_SECONDS;
        }

        Ok(settled + SETTLE_STEP_SECONDS)
    }
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_REST_DELTA, Spring};
    use crate::Error;

    #[test]
    fn a_rest_delta_that_is_not_a_threshold_is_refused() {
        // Each of these answered before: `0.0` gave 2.929 and `-1.0` gave
        // 100.004, both of them the scan reaching `MAX_SETTLE_SECONDS` and
        // stopping, and both of them shaped exactly like a settling time.
        //
        // Zero is the interesting one. It asks when the spring is *exactly* at
        // its target, and the closed form approaches without arriving, so
        // there is no answer rather than a large one.
        for delta in [0.0, -1.0, f64::NAN, f64::NEG_INFINITY] {
            assert!(
                matches!(
                    Spring::default().settles_after(delta),
                    Err(Error::Spring(_))
                ),
                "a rest delta of {delta} was accepted"
            );
        }

        // And the value every caller in this crate passes still works, with
        // the number v1 gives for it.
        let settled = Spring::default()
            .settles_after(DEFAULT_REST_DELTA)
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert!((settled - 0.566_666_666_666_665_9).abs() < f64::EPSILON);
    }
}
