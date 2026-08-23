//! Timing curves: the catalogue, `cubic-bezier` and `steps`.
//!
//! # Why `f64` and not `f32`
//!
//! Everything the renderer measures is `f32`, and this module is the one place
//! that is wrong. These functions exist on both surfaces and **the point of
//! having them twice is that the two can be checked against each other
//! exactly** -- JavaScript has one number type and it is `f64`, so a `f32`
//! port could never agree to more than seven digits and every cross-surface
//! test would need a tolerance nobody could derive. The value crosses into
//! `f32` when it reaches a style, which is one narrowing at the end rather
//! than a different arithmetic all the way through.
//!
//! # Ported rather than rewritten
//!
//! The constants are v1's, and v1's are the ones CSS and every animation
//! library settled on. A curve rewritten from its name looks right and differs
//! in the fourth decimal, which is the sort of difference nobody notices until
//! two surfaces are compared.

//! # Why this module refuses `mul_add`
//!
//! `a.mul_add(b, c)` is a *fused* multiply-add: one rounding where `a * b + c`
//! has two, so it is both faster and more accurate -- and **its last bit
//! differs from what JavaScript computes**, because JavaScript has no fused
//! form. Clippy's `suboptimal_flops` asks for it and this module declines,
//! throughout, because bit-exact agreement with the other surface is the
//! entire reason these functions are `f64`. A fused operation here would turn
//! an exact comparison into a tolerance nobody could derive, and the tolerance
//! would look like a property of the algorithm rather than of one call.
//!
//! **The rule is narrow, and it is about the assertion rather than the
//! architecture: fused arithmetic is forbidden where the comparison is exact,
//! and permitted where the comparison's slack is larger than a last-bit
//! difference.** This module is compared with `==` on `f64`, so the last bit
//! is the whole assertion. `chrome_blend.rs` also holds a second
//! implementation of what it checks — the blend formulae, in `f64`, against
//! what Skia drew — and its `mul_add` is fine, because its stated tolerance is
//! 1 in 255 and a fused rounding moves about `1e-16`, fourteen orders below
//! the slack. `paint.rs` likewise.
//!
//! Phrasing it about the comparison rather than about how many
//! implementations exist is what keeps it true later: **tighten a tolerant
//! comparison to an exact one and its fused arithmetic silently becomes
//! wrong**, while nothing about the architecture has changed. The tolerance
//! is also readable in the same file, where the number of implementations is
//! not. Accuracy and agreement are different goals; the tooling optimises for
//! accuracy.

#![expect(
    clippy::float_cmp,
    clippy::while_float,
    clippy::manual_midpoint,
    reason = "v1's arithmetic, kept in v1's shape. The equality tests are \
              exact by design -- `t == 0.0` and `t == 1.0` are the endpoints a \
              curve is pinned to, not approximations of them -- and the \
              bisection loop's `high - low > EPSILON` and `(low + high) / 2.0` \
              are the solver v1 runs. `f64::midpoint` computes a midpoint by a \
              different route and may round elsewhere, which is a difference \
              this module is compared bit-for-bit on."
)]
#![expect(
    clippy::suboptimal_flops,
    reason = "the lint wants a fused multiply-add, which rounds once where \
              JavaScript rounds twice. This module is implemented twice and \
              compared bit-for-bit against `tests/assets/animate/*.tsv`, so \
              agreement is the objective and accuracy is not. `expect` rather \
              than `allow`: if these expressions ever stop tripping the lint, \
              this stops being load-bearing and should be removed."
)]

use crate::Error;

/// Time and progress both run 0 to 1, though `back` and `elastic` overshoot.
pub type Curve = fn(f64) -> f64;

/// Clamped, because a track running past its own duration should hold at its
/// end value rather than accelerate off the curve.
const fn clamp01(t: f64) -> f64 {
    t.clamp(0.0, 1.0)
}

/// The overshoot the `back` family pulls back by, which is CSS's own constant.
const BACK_OVERSHOOT: f64 = 1.701_58;
/// The elastic oscillation's period, as a fraction of the duration.
const ELASTIC_PERIOD: f64 = 0.3;
/// The bounce parabola's height and the divisions of its four landings.
const BOUNCE_N: f64 = 7.562_5;
/// How the bounce's timeline divides: the four landings are fractions of this.
const BOUNCE_D: f64 = 2.75;

/// The `out` counterpart of an `in` curve: the same shape, reversed twice.
fn out_of(ease_in: impl Fn(f64) -> f64, t: f64) -> f64 {
    1.0 - ease_in(1.0 - t)
}

/// The `inOut` counterpart: the `in` curve to the midpoint, mirrored after it.
fn in_out_of(ease_in: impl Fn(f64) -> f64 + Copy, t: f64) -> f64 {
    if t < 0.5 {
        ease_in(t * 2.0) / 2.0
    } else {
        1.0 - ease_in((1.0 - t) * 2.0) / 2.0
    }
}

fn quad(t: f64) -> f64 {
    t * t
}

fn cubic(t: f64) -> f64 {
    t * t * t
}

fn quart(t: f64) -> f64 {
    t * t * t * t
}

fn quint(t: f64) -> f64 {
    t * t * t * t * t
}

fn sine(t: f64) -> f64 {
    1.0 - (t * std::f64::consts::PI / 2.0).cos()
}

/// Zero is a special case rather than an approximation: `2^-10` is `0.000977`,
/// which is a visible offset at the start of a curve that should begin at rest.
fn expo(t: f64) -> f64 {
    if t == 0.0 {
        0.0
    } else {
        (10.0 * t - 10.0).exp2()
    }
}

fn circ(t: f64) -> f64 {
    1.0 - (1.0 - t * t).sqrt()
}

fn back(t: f64) -> f64 {
    (BACK_OVERSHOOT + 1.0) * t * t * t - BACK_OVERSHOOT * t * t
}

/// Both ends are exact rather than computed: the oscillation's envelope is
/// zero at neither end by arithmetic, and a curve that starts at `-0.0` or
/// lands a hair off 1 is one that does not settle.
fn elastic(t: f64) -> f64 {
    if t == 0.0 || t == 1.0 {
        return t;
    }
    -(10.0 * t - 10.0).exp2()
        * ((t * 10.0 - 10.75) * (2.0 * std::f64::consts::PI) / ELASTIC_PERIOD)
            .sin()
}

/// The bounce is written in its `out` form -- the piecewise decay everyone
/// recognises -- and the other two are mirrors of it.
fn out_bounce(t: f64) -> f64 {
    if t < 1.0 / BOUNCE_D {
        return BOUNCE_N * t * t;
    }
    if t < 2.0 / BOUNCE_D {
        let t = t - 1.5 / BOUNCE_D;
        return BOUNCE_N * t * t + 0.75;
    }
    if t < 2.5 / BOUNCE_D {
        let t = t - 2.25 / BOUNCE_D;
        return BOUNCE_N * t * t + 0.937_5;
    }
    let t = t - 2.625 / BOUNCE_D;
    BOUNCE_N * t * t + 0.984_375
}

fn bounce(t: f64) -> f64 {
    1.0 - out_bounce(1.0 - t)
}

/// How close the solver has to get to a time before it accepts the parameter.
///
/// **This is the derived tolerance for every cross-surface comparison of
/// [`cubic_bezier`]**: two implementations agreeing to `1e-6` in `x` is the
/// most the algorithm promises, so a test asserting more is asserting an
/// accident.
pub const BEZIER_EPSILON: f64 = 1e-6;
/// Newton's iterations before the bisection fallback takes over.
const BEZIER_MAX_ITERATIONS: usize = 12;

/// The CSS `cubic-bezier(x1, y1, x2, y2)` curve.
///
/// The curve is parametric, so drawing it at a time means first finding the
/// parameter whose x is that time. **Newton converges in a few steps and
/// bisection covers what it cannot**: a near-flat section where the derivative
/// approaches zero stalls Newton, and so does a curve that has not converged
/// within twelve iterations. Both fall through to the same bisection,
/// which cannot stall.
pub fn cubic_bezier(x1: f64, y1: f64, x2: f64, y2: f64) -> impl Fn(f64) -> f64 {
    move |time: f64| {
        let target = clamp01(time);
        if target == 0.0 || target == 1.0 {
            return target;
        }

        let mut t = target;
        for _ in 0..BEZIER_MAX_ITERATIONS {
            let x = bezier(x1, x2, t) - target;
            if x.abs() < BEZIER_EPSILON {
                return bezier(y1, y2, t);
            }
            let slope = bezier_slope(x1, x2, t);
            if slope.abs() < BEZIER_EPSILON {
                break;
            }
            t -= x / slope;
        }

        let (mut low, mut high) = (0.0, 1.0);
        t = target;
        while high - low > BEZIER_EPSILON {
            if bezier(x1, x2, t) < target {
                low = t;
            } else {
                high = t;
            }
            t = (low + high) / 2.0;
        }
        bezier(y1, y2, t)
    }
}

/// One axis of the curve at parameter `t`, in Horner form.
fn bezier(a: f64, b: f64, t: f64) -> f64 {
    let c = 3.0 * a;
    let b_term = 3.0 * (b - a) - c;
    let a_term = 1.0 - c - b_term;
    ((a_term * t + b_term) * t + c) * t
}

/// That axis's derivative, which is what Newton steps along.
fn bezier_slope(a: f64, b: f64, t: f64) -> f64 {
    let c = 3.0 * a;
    let b_term = 3.0 * (b - a) - c;
    let a_term = 1.0 - c - b_term;
    (3.0 * a_term * t + 2.0 * b_term) * t + c
}

/// How many steps a [`steps`] curve may be built with.
///
/// One is the floor: a zero-step curve has no width to hold a value for.
pub const MIN_STEPS: u32 = 1;

/// Quantises progress into `count` equal jumps, as CSS `steps(count, end)`.
///
/// **Floors rather than rounds**, which is what holds each step for its full
/// width: a value a hair below the next boundary belongs to the step it is
/// still inside. The final instant is pinned to 1 so the animation lands on
/// its end value rather than a step short of it.
///
/// # Errors
///
/// Returns [`Error::Steps`] for a count below [`MIN_STEPS`]. v1 throws here
/// and a `Result` is the same refusal spelled the way this crate spells them.
pub fn steps(count: u32) -> Result<impl Fn(f64) -> f64, Error> {
    if count < MIN_STEPS {
        return Err(Error::Steps(count));
    }
    let count = f64::from(count);
    Ok(move |t: f64| {
        let clamped = clamp01(t);
        if clamped >= 1.0 {
            1.0
        } else {
            (clamped * count).floor() / count
        }
    })
}

/// The standard easing catalogue, by name.
///
/// **Every entry clamps its input**, so a track running past its own duration
/// holds at its end value rather than continuing off the curve. `Back` and
/// `Elastic` still overshoot *within* that range, which is what they are for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[expect(
    missing_docs,
    reason = "thirty variants whose names are their definitions: `OutCubic` \
              is the cubic curve's out form, and a line saying so on each \
              would be thirty lines nobody reads. The families are documented \
              where they are implemented."
)]
pub enum Easing {
    /// No easing: a constant rate from start to finish.
    Linear,
    InQuad,
    OutQuad,
    InOutQuad,
    InCubic,
    OutCubic,
    InOutCubic,
    InQuart,
    OutQuart,
    InOutQuart,
    InQuint,
    OutQuint,
    InOutQuint,
    InSine,
    OutSine,
    InOutSine,
    InExpo,
    OutExpo,
    InOutExpo,
    InCirc,
    OutCirc,
    InOutCirc,
    InBack,
    OutBack,
    InOutBack,
    InElastic,
    OutElastic,
    InOutElastic,
    InBounce,
    OutBounce,
    InOutBounce,
}

impl Easing {
    /// Every curve, so a test can walk the catalogue rather than list it.
    pub const ALL: [Self; 31] = [
        Self::Linear,
        Self::InQuad,
        Self::OutQuad,
        Self::InOutQuad,
        Self::InCubic,
        Self::OutCubic,
        Self::InOutCubic,
        Self::InQuart,
        Self::OutQuart,
        Self::InOutQuart,
        Self::InQuint,
        Self::OutQuint,
        Self::InOutQuint,
        Self::InSine,
        Self::OutSine,
        Self::InOutSine,
        Self::InExpo,
        Self::OutExpo,
        Self::InOutExpo,
        Self::InCirc,
        Self::OutCirc,
        Self::InOutCirc,
        Self::InBack,
        Self::OutBack,
        Self::InOutBack,
        Self::InElastic,
        Self::OutElastic,
        Self::InOutElastic,
        Self::InBounce,
        Self::OutBounce,
        Self::InOutBounce,
    ];

    /// The curve's value at a normalised time.
    #[must_use]
    pub fn at(self, time: f64) -> f64 {
        let t = clamp01(time);
        match self {
            Self::Linear => t,
            Self::InQuad => quad(t),
            Self::OutQuad => out_of(quad, t),
            Self::InOutQuad => in_out_of(quad, t),
            Self::InCubic => cubic(t),
            Self::OutCubic => out_of(cubic, t),
            Self::InOutCubic => in_out_of(cubic, t),
            Self::InQuart => quart(t),
            Self::OutQuart => out_of(quart, t),
            Self::InOutQuart => in_out_of(quart, t),
            Self::InQuint => quint(t),
            Self::OutQuint => out_of(quint, t),
            Self::InOutQuint => in_out_of(quint, t),
            Self::InSine => sine(t),
            Self::OutSine => out_of(sine, t),
            Self::InOutSine => in_out_of(sine, t),
            Self::InExpo => expo(t),
            Self::OutExpo => out_of(expo, t),
            Self::InOutExpo => in_out_of(expo, t),
            Self::InCirc => circ(t),
            Self::OutCirc => out_of(circ, t),
            Self::InOutCirc => in_out_of(circ, t),
            Self::InBack => back(t),
            Self::OutBack => out_of(back, t),
            Self::InOutBack => in_out_of(back, t),
            Self::InElastic => elastic(t),
            Self::OutElastic => out_of(elastic, t),
            Self::InOutElastic => in_out_of(elastic, t),
            Self::InBounce => bounce(t),
            Self::OutBounce => out_of(bounce, t),
            Self::InOutBounce => in_out_of(bounce, t),
        }
    }

    /// The name v1 spells this curve with, which is the name the vector table
    /// and the TypeScript surface both use.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Linear => "linear",
            Self::InQuad => "inQuad",
            Self::OutQuad => "outQuad",
            Self::InOutQuad => "inOutQuad",
            Self::InCubic => "inCubic",
            Self::OutCubic => "outCubic",
            Self::InOutCubic => "inOutCubic",
            Self::InQuart => "inQuart",
            Self::OutQuart => "outQuart",
            Self::InOutQuart => "inOutQuart",
            Self::InQuint => "inQuint",
            Self::OutQuint => "outQuint",
            Self::InOutQuint => "inOutQuint",
            Self::InSine => "inSine",
            Self::OutSine => "outSine",
            Self::InOutSine => "inOutSine",
            Self::InExpo => "inExpo",
            Self::OutExpo => "outExpo",
            Self::InOutExpo => "inOutExpo",
            Self::InCirc => "inCirc",
            Self::OutCirc => "outCirc",
            Self::InOutCirc => "inOutCirc",
            Self::InBack => "inBack",
            Self::OutBack => "outBack",
            Self::InOutBack => "inOutBack",
            Self::InElastic => "inElastic",
            Self::OutElastic => "outElastic",
            Self::InOutElastic => "inOutElastic",
            Self::InBounce => "inBounce",
            Self::OutBounce => "outBounce",
            Self::InOutBounce => "inOutBounce",
        }
    }
}
