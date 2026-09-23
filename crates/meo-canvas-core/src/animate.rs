//! Animation helpers: pure functions a caller uses to compute what to draw.
//!
//! Nothing here renders -- they take a time and return a number, a colour or a
//! set of them, for the caller to put into a scene. The same functions exist in
//! TypeScript, written independently against `tests/assets/animate/*.tsv`,
//! recorded from v9 at tag `v9.0.2`, so a misreading of the predecessor cannot
//! reach both surfaces and look like agreement.

pub mod color;
pub mod easing;
pub mod group;
pub mod interpolate;
pub mod sampled;
pub mod sequence;
pub mod spring;
pub mod track;
