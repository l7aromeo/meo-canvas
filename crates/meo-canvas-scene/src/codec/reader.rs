//! Reading primitives back out of a byte slice.
//!
//! Every method is fallible and every failure names the offset it happened at,
//! because the reader is the half that meets a buffer it did not write. An
//! error that says only "truncated" leaves a caller inspecting a fixture with
//! nothing to inspect.

use super::{CodecError, Wire};

/// A cursor over an encoded scene.
#[derive(Debug)]
pub(crate) struct Reader<'bytes> {
    bytes: &'bytes [u8],
    offset: usize,
}

impl<'bytes> Reader<'bytes> {
    /// Starts at the beginning of a buffer.
    pub(crate) const fn new(bytes: &'bytes [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    /// The offset the next read starts at.
    pub(crate) const fn offset(&self) -> usize {
        self.offset
    }

    /// How many bytes are left.
    pub(crate) const fn remaining(&self) -> usize {
        self.bytes.len() - self.offset
    }

    /// Takes `count` bytes verbatim.
    pub(crate) fn raw(
        &mut self,
        count: usize,
    ) -> Result<&'bytes [u8], CodecError> {
        let available = self.remaining();
        if count > available {
            return Err(CodecError::Truncated {
                offset: self.offset,
                needed: count,
                available,
            });
        }
        let start = self.offset;
        self.offset += count;
        Ok(&self.bytes[start..self.offset])
    }

    /// Takes a fixed-width value.
    ///
    /// The array conversion cannot fail: [`Reader::raw`] returns exactly `N`
    /// bytes or an error, and `N` is the array's own length.
    fn array<const N: usize>(&mut self) -> Result<[u8; N], CodecError> {
        let slice = self.raw(N)?;
        let mut buffer = [0_u8; N];
        buffer.copy_from_slice(slice);
        Ok(buffer)
    }

    /// Takes one byte.
    pub(crate) fn u8(&mut self) -> Result<u8, CodecError> {
        Ok(self.array::<1>()?[0])
    }

    /// Takes a `bool`, refusing any byte other than `0` or `1`.
    pub(crate) fn bool(&mut self) -> Result<bool, CodecError> {
        let offset = self.offset;
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            tag => Err(CodecError::UnknownTag { offset, tag }),
        }
    }

    /// Takes a little-endian `u16`.
    pub(crate) fn u16(&mut self) -> Result<u16, CodecError> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    /// Takes a little-endian `u32`.
    pub(crate) fn u32(&mut self) -> Result<u32, CodecError> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    /// Takes a little-endian `i16`.
    pub(crate) fn i16(&mut self) -> Result<i16, CodecError> {
        Ok(i16::from_le_bytes(self.array()?))
    }

    /// Takes a little-endian `i32`.
    pub(crate) fn i32(&mut self) -> Result<i32, CodecError> {
        Ok(i32::from_le_bytes(self.array()?))
    }

    /// Takes a little-endian `f32`.
    pub(crate) fn f32(&mut self) -> Result<f32, CodecError> {
        Ok(f32::from_le_bytes(self.array()?))
    }

    /// Reads a `u32` without advancing.
    ///
    /// [`super::decode`] uses it to check a node count against
    /// [`super::MAX_NODES`] before [`Reader::list`] reserves for it.
    pub(crate) fn peek_u32(&mut self) -> Result<u32, CodecError> {
        let start = self.offset;
        let value = self.u32();
        self.offset = start;
        value
    }

    /// Takes a length-prefixed byte slice.
    pub(crate) fn bytes(&mut self) -> Result<Vec<u8>, CodecError> {
        let length = self.u32()? as usize;
        Ok(self.raw(length)?.to_vec())
    }

    /// Takes a length-prefixed UTF-8 string.
    pub(crate) fn str(&mut self) -> Result<String, CodecError> {
        let length = self.u32()? as usize;
        let offset = self.offset;
        let slice = self.raw(length)?;
        core::str::from_utf8(slice)
            .map(str::to_owned)
            .map_err(|_| CodecError::InvalidUtf8 { offset })
    }

    /// Takes a count followed by that many values.
    ///
    /// The count is checked against the bytes left before anything is
    /// reserved: every element costs at least one byte, so a count above the
    /// remaining length is a corrupt prefix rather than a large list, and
    /// reserving for it would honour a number the buffer cannot back.
    pub(crate) fn list<T: Wire>(&mut self) -> Result<Vec<T>, CodecError> {
        let offset = self.offset;
        let count = self.u32()? as usize;
        let available = self.remaining();
        if count > available {
            return Err(CodecError::Truncated {
                offset,
                needed: count,
                available,
            });
        }
        // **The count is bounded and the reservation is not the count.**
        // `count <= available` is right about whether the prefix is corrupt,
        // and says nothing about the memory it asks for: a `Node` is 1048
        // bytes in memory and 184 on the wire, so a count this buffer can back
        // still reserved a thousand times the buffer. Measured before this
        // line existed: one megabyte of input, 1.02 GB reserved, then refused.
        //
        // So the reservation is bounded by what the remaining bytes can
        // actually contain, at the smallest this type encodes to. A count
        // larger than that is still allowed to be read -- it will run out of
        // bytes and fail honestly -- it is simply not reserved for up front.
        let capacity = count.min(available / T::MIN_ENCODED.max(1));
        let mut values = Vec::with_capacity(capacity);
        for _ in 0..count {
            values.push(T::read(self)?);
        }
        Ok(values)
    }

    /// Takes a presence byte, and the value when there is one.
    pub(crate) fn opt<T: Wire>(&mut self) -> Result<Option<T>, CodecError> {
        let offset = self.offset;
        match self.u8()? {
            0 => Ok(None),
            1 => Ok(Some(T::read(self)?)),
            tag => Err(CodecError::UnknownTag { offset, tag }),
        }
    }
}
