//! Charts, built out of layout rather than draw calls.
//!
//! **A colour a caller writes and this crate cannot read is refused, not
//! defaulted.** Every other style here arrives as a typed value the caller
//! cannot get wrong; a chart colour is a `String`, and it is the only place on
//! this surface where one is. So the builders are a **writer** in the sense
//! `AGENTS.md` uses -- the writer refuses what the type forbids, the
//! consumption side clamps what arrives as bytes -- and a chart drawn in
//! `"not a colour"` stops rather than drawing a black bar the caller never
//! asked for. The other surface already answers that input the same way: the
//! string crosses the boundary unparsed and is rejected there.

pub mod bar;
pub mod frame;
pub mod geometry;
pub mod line;
pub mod pie;
