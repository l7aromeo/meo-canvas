//! Animation helpers: pure functions a caller uses to compute what to draw.
//!
//! **Nothing here renders.** These take a time and give back a number, a
//! colour or a set of them, and the caller puts the result into a scene. They
//! are in the core rather than beside the painter because both surfaces need
//! them and the core is what both surfaces are built on.
//!
//! # Two implementations on purpose
//!
//! The same functions exist in TypeScript, written independently against the
//! same vector table rather than translated from this one. **A misreading of
//! v1 that reached both surfaces would look exactly like agreement**, so
//! neither is written from the other: `tests/assets/animate/*.tsv` came from
//! v1 running at tag `v9.0.2`, and both surfaces answer to it.

pub mod color;
pub mod easing;
pub mod group;
pub mod interpolate;
pub mod sampled;
pub mod sequence;
pub mod spring;
pub mod track;
