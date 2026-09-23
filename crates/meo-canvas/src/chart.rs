//! Charts, built out of layout rather than draw calls.
//!
//! **A colour this crate cannot read is refused, not defaulted.** A chart
//! colour is a `String` -- the only colour on this surface that is not a typed
//! value -- so a chart drawn in `"not a colour"` returns
//! [`Error::Chart`](crate::Error::Chart) rather than drawing a black bar the
//! caller never asked for.

pub mod bar;
pub mod frame;
pub mod geometry;
pub mod line;
pub mod pie;
