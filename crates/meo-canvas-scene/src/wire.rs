//! The macro every wire enum is declared with: one list emits the enum, the
//! byte each variant is written as, the mapping back, the
//! [`crate::codec::Wire`] impl and an `ALL` slice. By hand, a variant forgotten
//! in the decoder would read back as a different variant, not an error.

/// Declares an enum whose values cross the wire as one byte, with
/// `to_wire`/`from_wire` and an `ALL` constant in declaration order.
/// Discriminants are written at the call site rather than derived from
/// position, which moves when a variant is inserted.
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
