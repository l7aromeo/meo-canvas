//! Something a caller wrote that the renderer could not use.
//!
//! # Why this is not an error and not a warning
//!
//! An error stops the render. A [`crate::ImageWarning`] reports a failed fetch,
//! a fact about the world. A diagnostic is a fact about the caller's own input:
//! it was not usable, and the render continued without it.
//!
//! The render can be correct and still need one. `<color=#ff00>` is CSS's
//! four-digit `#RGBA`, yellow at alpha zero, and a caller who truncated
//! `#ff0000` sees blank text either way.
//!
//! # Where one is raised
//!
//! At the site that dropped the value: one layer down, a dropped value and one
//! never written are the same absence.

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
    /// A path rather than a name, since a nested value has no single property
    /// name: `<color=zzz>` and `segments[2].color` are paths, and `color` is a
    /// field. Every diagnostic raised is a markup tag, so the path is the tag
    /// as written, value included: `<weight=1500>` rather than `<weight>`. A
    /// diagnostic about a scene value would take the property-path spelling,
    /// with [`Diagnostic::offset`] `None`; nothing raises one.
    pub path: String,
    /// What was wrong with it, and what was done instead.
    pub detail: String,
    /// Where in the markup it was, as a byte offset into the string the caller
    /// wrote, or `None` for a diagnostic that did not come from markup.
    ///
    /// The offset of the tag's `<` in the caller's own string, so
    /// `input[offset..]` starts at the tag. There is no end: `unescape` runs
    /// before a tag is scanned and an escape inside a tag changes its length,
    /// so a length from the scanner would be wrong for exactly the tags with
    /// escapes, and nothing would mark them.
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
    /// `new` leaving `None` and `at` setting it, asserted together, say the two
    /// constructors differ.
    #[test]
    fn only_the_positioned_constructor_carries_an_offset() {
        assert_eq!(Diagnostic::new("<color=zzz>", "not a colour").offset, None);
        assert_eq!(
            Diagnostic::at("<color=zzz>", "not a colour", 20).offset,
            Some(20)
        );
    }

    /// Two diagnostics about the same tag in different places differ: without
    /// the offset, `<color=zzz>` written twice reports the same thing twice.
    #[test]
    fn the_offset_is_part_of_the_identity() {
        let (path, reason) = ("<color=zzz>", "not a colour");
        assert_ne!(
            Diagnostic::at(path, reason, 0),
            Diagnostic::at(path, reason, 20)
        );
    }

    /// Two diagnostics about different paths differ: comparing on the reason
    /// alone would collapse two places into one, and the place is what a caller
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
