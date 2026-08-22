//! Property tables, and the mask that says which properties a record carries.
//!
//! A node's four style groups are each described by a table declared with
//! [`arena_group`]. The table is the single definition of a group's property
//! order and of the bit each property occupies; the decoder reads it and the
//! TypeScript writer is generated against the same numbers, so an added
//! property is one table entry rather than two lists that must agree.
//!
//! # Why a mask holds 53 bits and not 64
//!
//! A double represents integers exactly only to 2^53. A 64-bit mask written
//! into one `f64` slot loses every bit above the 53rd **silently** -- no
//! rounding error a reader could notice, just properties that vanish. So a
//! slot carries 53 bits, two slots name 106 properties, and a group that grows
//! past its slots takes another.
//!
//! [`arena_group`] emits a compile-time assertion for exactly that, so a table
//! outgrowing its mask is a build failure rather than a field that stops
//! arriving.

use super::{ArenaError, Reader};

/// Bits a mask slot carries.
///
/// 53, not 64: a double is exact on integers only to 2^53, so the 54th bit of
/// a mask written into an `f64` is lost without a trace. This is the constant
/// the whole format's property budget derives from.
pub(crate) const BITS_PER_SLOT: u32 = 53;

/// Which properties of one group a record carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Mask {
    /// One `u64` per slot, each holding at most [`BITS_PER_SLOT`] bits.
    slots: [u64; Self::MAX_SLOTS],
    used: usize,
}

impl Mask {
    /// The largest value a mask slot may hold: 53 bits all set.
    const LARGEST_SLOT: f64 = ((1_u64 << BITS_PER_SLOT) - 1) as f64;
    /// The most slots any group declares.
    ///
    /// Two, which names 106 properties. The largest table today is
    /// [`layout`](crate::arena::group::layout) at well under 53, so one slot
    /// serves every group; the second exists so a group can grow into it
    /// without the reader changing shape.
    pub(crate) const MAX_SLOTS: usize = 2;

    /// Reads `slots` mask slots.
    pub(crate) fn read(
        input: &mut Reader<'_>,
        slots: usize,
    ) -> Result<Self, ArenaError> {
        debug_assert!(
            slots <= Self::MAX_SLOTS,
            "a group asked for {slots} mask slots, more than {} exist",
            Self::MAX_SLOTS
        );
        let mut mask = Self {
            slots: [0; Self::MAX_SLOTS],
            used: slots,
        };
        for slot in mask.slots.iter_mut().take(slots) {
            let offset = input.offset();
            let bits = input.bounded_integer(Self::LARGEST_SLOT)?;
            // The bound is what turns "the writer packed 64 bits into a double"
            // from silent loss into an error naming the slot.
            if bits < 0.0 {
                return Err(ArenaError::OutOfRange {
                    slot: offset,
                    found: bits,
                });
            }
            *slot = bits as u64;
        }
        Ok(mask)
    }

    /// Whether the property at `index` is present.
    pub(crate) const fn has(&self, index: u32) -> bool {
        let slot = (index / BITS_PER_SLOT) as usize;
        if slot >= self.used {
            return false;
        }
        self.slots[slot] & (1 << (index % BITS_PER_SLOT)) != 0
    }

    /// How many properties the record carries.
    ///
    /// Not read by the decoder, which walks the bits it needs. It exists for
    /// the tests that check a mask against the values that follow it, which is
    /// the invariant a writer can break silently.
    #[cfg(test)]
    pub(crate) const fn count(&self) -> u32 {
        let mut total = 0;
        let mut slot = 0;
        while slot < self.used {
            total += self.slots[slot].count_ones();
            slot += 1;
        }
        total
    }
}

/// Whether a table's indices ascend, which the reader depends on.
///
/// Properties are written in index order, so a table declared out of order
/// would have the decoder read the right count of slots into the wrong fields
/// — a corruption no length check catches. Checked at compile time by every
/// [`arena_group`] table.
pub(crate) const fn ascending(indices: &[u32]) -> bool {
    let mut i = 1;
    while i < indices.len() {
        if indices[i - 1] >= indices[i] {
            return false;
        }
        i += 1;
    }
    true
}

/// Declares one group's property table.
///
/// Emits the property count, the mask width the count needs, a compile-time
/// check that the indices ascend and fit that mask, and the reader that
/// applies the present properties onto the group's `Default`.
///
/// Indices are written explicitly at each property, as `wire_enum!` writes
/// discriminants in the scene crate and for the same reason: a position-derived
/// index renumbers every later property when one is inserted, and the number is
/// a published part of the format.
macro_rules! arena_group {
    (
        $(#[$meta:meta])*
        $vis:vis mod $name:ident for $target:ty {
            $( $index:literal => $field:ident : $ty:ty ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        $vis mod $name {
            use super::{Mask, ascending, BITS_PER_SLOT};
            use crate::arena::value::ArenaValue;
            use crate::arena::{ArenaError, Reader};

            /// Every property's index, in the order they are written.
            pub(crate) const INDICES: &[u32] = &[$($index),+];

            /// Every property's name, in the same order.
            ///
            /// The table describing itself. The round-trip artefact is keyed by
            /// these, so a property added to the table appears in the artefact
            /// on the next regeneration rather than being quietly untested.
            ///
            /// Test-only, like [`probe`]: nothing the addon does at runtime
            /// needs a property's name, and a table of strings compiled into
            /// the shipped binary would be paid for by every caller of it.
            #[cfg(test)]
            pub(crate) const NAMES: &[&str] = &[$(stringify!($field)),+];

            /// How many properties this group declares.
            pub(crate) const COUNT: usize = INDICES.len();

            /// How many mask slots the count needs.
            pub(crate) const SLOTS: usize =
                (COUNT as u32).div_ceil(BITS_PER_SLOT) as usize;

            // The table is the format. An out-of-order index would read the
            // right number of slots into the wrong fields, and a table wider
            // than its mask would lose its last properties silently -- which is
            // the whole reason the mask is 53 bits and not 64.
            const _: () = {
                assert!(ascending(INDICES), "property indices must ascend");
                assert!(
                    COUNT <= SLOTS * BITS_PER_SLOT as usize,
                    "the table outgrew its mask slots"
                );
                assert!(
                    SLOTS <= Mask::MAX_SLOTS,
                    "the table needs more mask slots than a record carries"
                );
            };

            /// A group with exactly one property set to its probe value.
            ///
            /// The probe value is whatever [`crate::arena::value::ArenaValue`]
            /// reads out of a slot stream of ones -- see
            /// [`crate::arena::probe_reader`]. Using the decoder to produce it
            /// rather than a second table of literals is what stops the probe
            /// from drifting: there is no separate definition of "the value for
            /// a `Length`" that could disagree with how a `Length` is read.
            ///
            /// Returns `None` for an index this group does not declare.
            #[cfg(test)]
            pub(crate) fn probe(index: u32) -> Option<$target> {
                let mut value = <$target>::default();
                let mut found = false;
                $(
                    if index == $index {
                        let base = <$target>::default();
                        for fill in crate::arena::probe_fills() {
                            let (slots, values) =
                                crate::arena::probe_slots(fill);
                            let mut input =
                                Reader::new_for_probe(&slots, &values);
                            if let Ok(read) = ArenaValue::read(&mut input) {
                                value.$field = read;
                                found = true;
                                // The first fill that says something the
                                // default does not. A value equal to the
                                // default leaves an encoder nothing to write.
                                if value != base {
                                    break;
                                }
                            }
                        }
                    }
                )+
                found.then_some(value)
            }

            /// Reads the properties this record carries onto a default.
            pub(crate) fn read(
                mask: &Mask,
                input: &mut Reader<'_>,
            ) -> Result<$target, ArenaError> {
                let mut value = <$target>::default();
                $(
                    if mask.has($index) {
                        value.$field = ArenaValue::read(input)?;
                    }
                )+
                Ok(value)
            }
        }
    };
}

pub(crate) use arena_group;
