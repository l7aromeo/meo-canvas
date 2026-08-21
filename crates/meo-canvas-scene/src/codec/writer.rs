//! Appending primitives to a byte buffer.
//!
//! Every method is infallible: a `Vec<u8>` grows, so writing has no error case
//! and the encoder needs no `Result` threaded through it. That asymmetry with
//! [`super::reader`] is the point -- reading is where a buffer can lie.

use super::Wire;

/// A cursor that appends to a caller's buffer.
///
/// Borrows rather than owns, so [`super::encode_into`] can append a scene to a
/// buffer that already holds something.
#[derive(Debug)]
pub(crate) struct Writer<'buffer> {
    out: &'buffer mut Vec<u8>,
}

impl<'buffer> Writer<'buffer> {
    /// Wraps a buffer.
    pub(crate) const fn new(out: &'buffer mut Vec<u8>) -> Self {
        Self { out }
    }

    /// Appends bytes verbatim, with no length prefix.
    pub(crate) fn raw(&mut self, bytes: &[u8]) {
        self.out.extend_from_slice(bytes);
    }

    /// Appends one byte.
    pub(crate) fn u8(&mut self, value: u8) {
        self.out.push(value);
    }

    /// Appends a `bool` as `0` or `1`.
    pub(crate) fn bool(&mut self, value: bool) {
        self.u8(u8::from(value));
    }

    /// Appends a little-endian `u16`.
    pub(crate) fn u16(&mut self, value: u16) {
        self.raw(&value.to_le_bytes());
    }

    /// Appends a little-endian `u32`.
    pub(crate) fn u32(&mut self, value: u32) {
        self.raw(&value.to_le_bytes());
    }

    /// Appends a little-endian `i16`.
    pub(crate) fn i16(&mut self, value: i16) {
        self.raw(&value.to_le_bytes());
    }

    /// Appends a little-endian `i32`.
    pub(crate) fn i32(&mut self, value: i32) {
        self.raw(&value.to_le_bytes());
    }

    /// Appends a little-endian `f32`.
    pub(crate) fn f32(&mut self, value: f32) {
        self.raw(&value.to_le_bytes());
    }

    /// Appends a length-prefixed byte slice.
    ///
    /// The cast cannot lose data for any buffer this crate produces: a slice
    /// longer than `u32::MAX` is four gigabytes, and [`super::MAX_NODES`]
    /// bounds a scene far below that.
    pub(crate) fn bytes(&mut self, value: &[u8]) {
        self.u32(value.len() as u32);
        self.raw(value);
    }

    /// Appends a length-prefixed UTF-8 string.
    pub(crate) fn str(&mut self, value: &str) {
        self.bytes(value.as_bytes());
    }

    /// Appends a count followed by that many values.
    pub(crate) fn list<T: Wire>(&mut self, values: &[T]) {
        self.u32(values.len() as u32);
        for value in values {
            value.write(self);
        }
    }

    /// Appends a presence byte, followed by the value when there is one.
    pub(crate) fn opt<T: Wire>(&mut self, value: Option<&T>) {
        match value {
            Some(value) => {
                self.u8(1);
                value.write(self);
            }
            None => self.u8(0),
        }
    }
}
