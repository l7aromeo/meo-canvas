//! The macro every wire enum in this crate is declared with.
//!
//! A wire enum is one that crosses [`crate::codec`] as a single byte. Three
//! things have to agree for that to work: the variant list, the byte each
//! variant is written as, and the mapping back. Writing those three by hand is
//! writing the same list three times, and the failure mode is silent -- a
//! variant added to the enum and forgotten in the decoder reads back as a
//! different variant, not as an error.
//!
//! [`wire_enum`] emits all three from one list, plus the [`crate::codec::Wire`]
//! implementation and an `ALL` slice that lets a test round-trip every variant
//! of every enum without naming any of them. That slice is why `codec` reaches
//! full arm coverage from a handful of tests rather than one per variant.

/// Declares an enum whose values cross the wire as one byte.
///
/// Emits the enum itself, `to_wire`/`from_wire`, and an `ALL` constant listing
/// every variant in declaration order.
///
/// Discriminants are written explicitly at the call site rather than derived
/// from position, because position changes when a variant is inserted and the
/// byte a variant is written as cannot.
macro_rules! wire_enum {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $(
                $(#[$vmeta:meta])*
                $variant:ident = $value:expr
            ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        $vis enum $name {
            $(
                $(#[$vmeta])*
                $variant
            ),+
        }

        impl $name {
            /// Every variant, in declaration order.
            ///
            /// Exists so a test can exercise the whole enum without listing it a
            /// second time; see the `every_wire_enum_round_trips` tests in
            /// [`crate::codec`].
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            /// The byte this variant is written as.
            #[must_use]
            pub const fn to_wire(self) -> u8 {
                match self {
                    $(Self::$variant => $value),+
                }
            }

            /// The variant a byte names, or `None` if it names none.
            #[must_use]
            pub const fn from_wire(byte: u8) -> Option<Self> {
                match byte {
                    $($value => Some(Self::$variant),)+
                    _ => None,
                }
            }
        }

        impl crate::codec::Wire for $name {
            fn write(&self, out: &mut crate::codec::Writer<'_>) {
                out.u8(self.to_wire());
            }

            fn read(
                input: &mut crate::codec::Reader<'_>,
            ) -> Result<Self, crate::codec::CodecError> {
                let offset = input.offset();
                let tag = input.u8()?;
                Self::from_wire(tag).ok_or(
                    crate::codec::CodecError::UnknownTag { offset, tag },
                )
            }
        }
    };
}

pub(crate) use wire_enum;
