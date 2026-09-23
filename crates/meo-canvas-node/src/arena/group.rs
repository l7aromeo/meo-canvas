//! Property tables, and the mask saying which properties a record carries. Each
//! group's table, declared with [`arena_group`], is the one definition of
//! property order and mask bits, for the decoder and the TypeScript generator
//! alike; [`BITS_PER_SLOT`] says why 53.

use super::{ArenaError, Reader};

/// Bits a mask slot carries: 53, not 64, since a double is exact on integers
/// only to 2^53 and the 54th bit would be lost without a trace. The whole
/// format's property budget derives from this.
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
    /// The most slots any group declares: two, naming 106 properties. Every
    /// table fits one today; the second lets a group grow without the
    /// reader changing shape.
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

    /// How many properties the record carries. The decoder walks bits instead;
    /// this is for the tests that check a mask against the values after it.
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

/// Whether a table's indices ascend, checked at compile time by every
/// [`arena_group`] table: out of order, the decoder reads the right count of
/// slots into the wrong fields, which no length check catches.
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

/// Declares one group's property table: its count, mask width, a compile-time
/// check that indices ascend and fit, and a reader onto the group's `Default`.
/// Each property carries the name a caller writes (`borderColor` for
/// `border_color_all`) and an explicit index, since both are published.
macro_rules! arena_group {
    (
        $(#[$meta:meta])*
        $vis:vis mod $name:ident for $target:ty {
            $( $index:literal => $field:ident as $caller:literal : $ty:ty ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        $vis mod $name {
            use super::{Mask, ascending, BITS_PER_SLOT};
            use crate::arena::value::ArenaValue;
            use crate::arena::{ArenaError, Reader};

            /// Every property's index, in the order they are written.
            pub(crate) const INDICES: &[u32] = &[$($index),+];

            /// Every property's name, in the same order: the round-trip artefact's
            /// keys. Test-only, like [`probe`], so the shipped binary carries no table
            /// of strings.
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

            /// A group with exactly one property set to its probe value, which the
            /// decoder reads from a stream of ones ([`crate::arena::probe_reader`]),
            /// so no second table of literals can drift. `None` for an index this
            /// group does not declare.
            #[cfg(test)]
            pub(crate) fn probe(index: u32) -> Option<$target> {
                let mut value = <$target>::default();
                let mut found = false;
                $(
                    if index == $index {
                        let base = <$target>::default();
                        for fills in crate::arena::probe_fills() {
                            let (slots, values) = crate::arena::probe_slots();
                            let mut input =
                                Reader::new_for_probe(&slots, &values, fills);
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

            /// A group with every property set to its probe value: one record
            /// exercising every write path, catching slot widths or order that are
            /// right one property at a time and wrong together.
            #[cfg(test)]
            pub(crate) fn probe_all() -> $target {
                let mut value = <$target>::default();
                $(
                    if let Some(one) = probe($index) {
                        value.$field = one.$field;
                    }
                )+
                value
            }

            /// One property's value as JSON, by index: only the table knows which
            /// field an index names, so that knowledge stays here.
            #[cfg(test)]
            pub(crate) fn field_json(
                value: &$target,
                index: u32,
            ) -> Option<String> {
                use crate::arena::cases::ToJson;
                $(
                    if index == $index {
                        return Some(value.$field.to_json());
                    }
                )+
                None
            }

            /// Reads the properties this record carries onto a default.
            pub(crate) fn read(
                mask: &Mask,
                input: &mut Reader<'_>,
            ) -> Result<$target, ArenaError> {
                let mut value = <$target>::default();
                $(
                    if mask.has($index) {
                        // Where a property's decode begins, so where its name is
                        // known: the caller's spelling from the table, since Rust's
                        // is not always it (`border_color_all` is `borderColor`).
                        input.set_property($caller);
                        value.$field = ArenaValue::read(input)?;
                    }
                )+
                Ok(value)
            }
        }
    };
}

pub(crate) use arena_group;
