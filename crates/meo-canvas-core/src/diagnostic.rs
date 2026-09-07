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
/// own source. `<color=zzz>` locates a mistake; `color` does not.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Diagnostic {
    /// Where the value was, as the caller spelled it.
    ///
    /// A path rather than a name: a value nested inside another property has
    /// no single property name that would find it. `<color=zzz>` and
    /// `segments[2].color` are paths; `color` is a field.
    ///
    /// **Every diagnostic this crate raises today is a markup tag**, so the
    /// path is the tag as it was written, with the value in it -- the whole
    /// of `<weight=1500>` rather than `<weight>`. The property-path spelling
    /// is what a diagnostic about a scene value would use, and nothing
    /// produces one yet; it is described here because the shape has to hold
    /// for both, and [`Diagnostic::offset`] is `None` for the second kind.
    pub path: String,
    /// What was wrong with it, and what was done instead.
    pub detail: String,
    /// Where in the markup it was, as a byte offset into the string the
    /// caller wrote, or `None` for a diagnostic that did not come from
    /// markup.
    ///
    /// The offset of the tag's `<`, in the caller's own string rather than in
    /// anything this crate derived from it -- so `input[offset..]` starts at
    /// the tag. **There is no end.** `unescape` runs over the whole string
    /// before a tag is scanned, and an escape inside a tag body changes its
    /// length, so `<color=re\\d>` is one byte longer where the caller wrote
    /// it than in the text the scanner measured. A length taken from the
    /// scanner would therefore be right only for tags containing no escapes,
    /// which is worse than absent: nothing would mark the cases where it
    /// lies.
    pub offset: Option<usize>,
}

impl Diagnostic {
    /// One diagnostic, naming the path and what went wrong.
    ///
    /// Carries no offset. [`Diagnostic::at`] is the same thing for a value
    /// that came from markup, where the position is known.
    #[must_use]
    pub fn new(path: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            detail: detail.into(),
            offset: None,
        }
    }

    /// [`Diagnostic::new`], and where in the caller's markup it was.
    ///
    /// `offset` is a byte index into the string the caller passed to the
    /// parser, pointing at the tag's `<`.
    #[must_use]
    pub fn at(
        path: impl Into<String>,
        detail: impl Into<String>,
        offset: usize,
    ) -> Self {
        Self {
            offset: Some(offset),
            ..Self::new(path, detail)
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

    /// A diagnostic that did not come from markup has no place to point at.
    ///
    /// The pair is the assertion: `new` leaving `None` is what makes the
    /// field additive rather than a break, and `at` setting it is what makes
    /// the field worth having. Neither alone says the two constructors differ.
    #[test]
    fn only_the_positioned_constructor_carries_an_offset() {
        assert_eq!(Diagnostic::new("<color=zzz>", "not a colour").offset, None);
        assert_eq!(
            Diagnostic::at("<color=zzz>", "not a colour", 20).offset,
            Some(20)
        );
    }

    /// Two diagnostics about the same tag in different places differ.
    ///
    /// This is the case the offset exists for: without it the two values are
    /// equal, and a caller with `<color=zzz>` written twice is told the same
    /// thing twice with nothing to separate the reports.
    #[test]
    fn the_offset_is_part_of_the_identity() {
        let (path, reason) = ("<color=zzz>", "not a colour");
        assert_ne!(
            Diagnostic::at(path, reason, 0),
            Diagnostic::at(path, reason, 20)
        );
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
