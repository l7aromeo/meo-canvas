//! Something a caller wrote that the renderer could not use.
//!
//! # Why this is not an error and not a warning
//!
//! An error stops the render and a caller handles it. A
//! [`crate::ImageWarning`] reports that a fetch failed, which is a fact about
//! the world. **A diagnostic is a fact about the caller's own input**: they
//! wrote something, it was not usable, and the render continued without it.
//!
//! The case that makes the channel necessary is one where the render is
//! *correct*. `<color=#ff00>` is conformant -- CSS's four-digit `#RGBA`, so
//! yellow at alpha zero -- and a caller who truncated `#ff0000` sees blank
//! text either way. **Correct behaviour and silent failure with the same
//! observable**, which no amount of rendering accurately can close.
//!
//! # Where one is raised
//!
//! **Where the distinction still exists.** A value that was dropped and a
//! value that was never written are the same absence one layer down, so the
//! only site that can tell them apart is the one that did the dropping.
//! Raising it later is not worse, it is impossible.

use core::fmt;

/// Something the caller wrote that could not be used.
///
/// Carries the caller's own path to the value rather than the field name the
/// scene stores it under, because a path is what they can search for in their
/// own source. `segments[0].fontSize` locates a mistake; `font_size` does not.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Diagnostic {
    /// Where the value was, as the caller spelled it.
    ///
    /// A path rather than a name: a value nested inside another property has
    /// no single property name that would find it. `transform.translateX` and
    /// `segments[2].color` are paths; `translate_x` is a field.
    pub path: String,
    /// What was wrong with it, and what was done instead.
    pub detail: String,
}

impl Diagnostic {
    /// One diagnostic, naming the path and what went wrong.
    #[must_use]
    pub fn new(path: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            detail: detail.into(),
        }
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path, self.detail)
    }
}

#[cfg(test)]
mod tests {
    use super::Diagnostic;

    /// The path leads the message, because it is what a caller searches for.
    #[test]
    fn a_diagnostic_reads_as_a_place_then_a_reason() {
        let one = Diagnostic::new("segments[0].fontSize", "not a length");
        assert_eq!(one.to_string(), "segments[0].fontSize: not a length");
    }

    /// Two diagnostics about different paths are different diagnostics.
    ///
    /// The pair is the point: a type that compared equal on the reason alone
    /// would collapse two places into one, and the place is the half a caller
    /// cannot recover from the render.
    #[test]
    fn the_path_is_part_of_the_identity() {
        let reason = "not a length";
        assert_ne!(
            Diagnostic::new("segments[0].fontSize", reason),
            Diagnostic::new("segments[1].fontSize", reason)
        );
    }
}
