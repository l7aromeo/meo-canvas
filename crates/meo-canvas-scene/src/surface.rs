//! What the surface a scene is drawn on is made of.
//!
//! [`Scene`](crate::Scene) already carries its size and its device scale.
//! These are the rest of the surface's description: which rasteriser it asks
//! for, how deep its pixels are, and which colour space it composites in.
//!
//! # Why they are on the scene and not on the renderer
//!
//! `Surface::new` in `meo-canvas-core` takes size, scale and gpu, and until
//! now two of those came from the scene and one from the renderer for no
//! reason a caller could see. A caller writes all four in one place —
//! `Root({ width, height, scale, gpu })` on either surface — so the scene is
//! where they belong.
//!
//! # Why every one of them is optional
//!
//! `None` means "whatever the renderer decides", which is what makes *the
//! renderer's value is the default* a true sentence rather than a comment. A
//! bare `bool` defaulting to `true` would silently override a renderer someone
//! set to the CPU on purpose, and there would be no way for a scene to say it
//! does not care.

use crate::wire::wire_enum;

wire_enum! {
    /// The pixel layout a surface composites, exports and reads back in.
    ///
    /// Every layout `meo-skia-canvas` offers, named as it names them. The list
    /// is copied rather than delegated because upstream's own `all()` is
    /// `pub(crate)` (`meo-skia-canvas-0.11.0/src/pixels.rs:493`) and cannot be
    /// walked from here — so the guard is a compile-time one instead, and it is
    /// stronger than a conformance test: `to_skia` matches exhaustively over
    /// this enum and `from_skia` matches exhaustively over theirs, so a variant
    /// added on either side fails the build rather than a test.
    #[derive(Default)]
    pub enum ColorType {
        /// 8-bit unsigned normalized RGBA, four bytes a pixel. The usual one.
        #[default]
        Uint8 = 0,
        /// 16-bit float RGBA, eight bytes a pixel.
        F16 = 1,
        /// 32-bit float RGBA, sixteen bytes a pixel.
        F32 = 2,
        /// 8-bit alpha only. Colour reads as zero.
        Alpha8 = 3,
        /// 8-bit greyscale. Alpha reads as opaque.
        Gray8 = 4,
        /// 8-bit single channel, unsigned normalized.
        R8UNorm = 5,
        /// 8-bit red and green, unsigned normalized.
        R8G8UNorm = 6,
        /// 16-bit float alpha only.
        A16Float = 7,
        /// 16-bit unsigned normalized alpha only.
        A16UNorm = 8,
        /// Four bits a channel.
        Argb4444 = 9,
        /// Five, six and five bits, with no alpha.
        Rgb565 = 10,
        /// 8-bit RGB with a padding byte.
        Rgb888x = 11,
        /// 8-bit BGRA, which is what Apple and Windows composite in.
        Bgra8888 = 12,
        /// 8-bit RGBA, read through the sRGB transfer function.
        Srgba8888 = 13,
        /// Whichever 32-bit order this platform composites in.
        ///
        /// A readback of this layout needs no swizzle. Reading the type back
        /// afterwards reports the concrete one, since that is what the pixels
        /// turned out to be.
        N32 = 14,
        /// Ten bits a colour channel and two of alpha.
        Rgba1010102 = 15,
        /// Ten bits a colour channel and two of alpha, blue first.
        Bgra1010102 = 16,
        /// Ten bits a colour channel with two of padding.
        Rgb101010x = 17,
        /// Ten bits a colour channel with two of padding, blue first.
        Bgr101010x = 18,
        /// 16-bit float red and green.
        R16G16Float = 19,
        /// 16-bit unsigned normalized red and green.
        R16G16UNorm = 20,
        /// 16-bit unsigned normalized RGBA.
        R16G16B16A16UNorm = 21,
        /// 16-bit float RGBA, clamped to the unit range.
        F16Norm = 22,
    }
}

wire_enum! {
    /// The colour space a surface composites in.
    ///
    /// Fixed for the whole surface rather than at export, because a colour
    /// outside the space's gamut is clipped as it is drawn and an export
    /// converts out of it. Same copying rule as [`ColorType`].
    #[derive(Default)]
    pub enum ColorSpace {
        /// sRGB primaries and transfer function.
        #[default]
        Srgb = 0,
        /// sRGB primaries, linear transfer function.
        SrgbLinear = 1,
        /// Display P3 primaries, sRGB transfer function.
        DisplayP3 = 2,
        /// Display P3 primaries, linear transfer function.
        DisplayP3Linear = 3,
        /// Rec. 2020 primaries, Rec. 709 transfer function.
        Rec2020 = 4,
        /// Rec. 2020 primaries, linear transfer function.
        Rec2020Linear = 5,
        /// Rec. 2020 primaries, PQ transfer function -- HDR10.
        Rec2020Pq = 6,
        /// Rec. 2020 primaries, HLG transfer function.
        Rec2020Hlg = 7,
    }
}

/// The names v1 accepts that are not variant names.
///
/// `meo-skia-canvas`'s TypeScript accepts fifteen spellings for eight spaces
/// (`meo-skia-canvas/lib/index.d.ts:240`): `'hdr10'` and `'rec2020-pq'` are one
/// space, and so are `'p3'` and `'display-p3'`. A v1 caller has one of those
/// fifteen written down, and seven of them name nothing here.
///
/// **Associated constants rather than variants**, so the wire enum stays
/// honest: [`ALL`](ColorSpace::ALL) is still eight, `to_wire` and `from_wire`
/// stay total, and the generator still emits eight keywords rather than
/// fifteen. An alias is a second name for a space, not a second space, and only
/// a constant says that.
///
/// [`ColorType`] carries the same aliases for the same reason.
impl ColorSpace {
    /// `'bt2020'`, which is [`Rec2020`](ColorSpace::Rec2020).
    pub const BT2020: Self = Self::Rec2020;
    /// `'bt2020-linear'`, which is
    /// [`Rec2020Linear`](ColorSpace::Rec2020Linear).
    pub const BT2020_LINEAR: Self = Self::Rec2020Linear;
    /// `'hdr10'`, which is [`Rec2020Pq`](ColorSpace::Rec2020Pq).
    pub const HDR10: Self = Self::Rec2020Pq;
    /// `'hlg'`, which is [`Rec2020Hlg`](ColorSpace::Rec2020Hlg).
    pub const HLG: Self = Self::Rec2020Hlg;
    /// `'linear'`, which is [`SrgbLinear`](ColorSpace::SrgbLinear).
    pub const LINEAR: Self = Self::SrgbLinear;
    /// `'p3'`, which is [`DisplayP3`](ColorSpace::DisplayP3).
    pub const P3: Self = Self::DisplayP3;
    /// `'p3-linear'`, which is
    /// [`DisplayP3Linear`](ColorSpace::DisplayP3Linear).
    pub const P3_LINEAR: Self = Self::DisplayP3Linear;
}

/// The names v1 accepts that are not variant names.
///
/// The same problem [`ColorSpace`]'s aliases solve, in the enum where a v1
/// caller is most likely to hit it: `'rgba'` is the spelling in
/// `RootProps.colorType`'s own default (`canvas.type.ts:1202`), and it names
/// nothing here -- the layout is [`Uint8`](ColorType::Uint8).
///
/// Only the spellings that differ by more than case. `'ARGB4444'` and
/// `'BGRA8888'` are [`Argb4444`] and [`Bgra8888`] to any reader, and a constant
/// per casing would be sixteen names for the sake of six.
///
/// [`Argb4444`]: ColorType::Argb4444
/// [`Bgra8888`]: ColorType::Bgra8888
impl ColorType {
    /// `'bgra'`, which is [`Bgra8888`](ColorType::Bgra8888).
    pub const BGRA: Self = Self::Bgra8888;
    /// `'rgb'`, which is [`Rgb888x`](ColorType::Rgb888x) -- eight bits a
    /// channel with a padding byte, since there is no three-byte layout.
    pub const RGB: Self = Self::Rgb888x;
    /// `'rgba'`, which is [`Uint8`](ColorType::Uint8). v1's default.
    pub const RGBA: Self = Self::Uint8;
    /// `'RGBAF16'`, which is [`F16`](ColorType::F16).
    pub const RGBAF16: Self = Self::F16;
    /// `'RGBAF16Norm'`, which is [`F16Norm`](ColorType::F16Norm).
    pub const RGBAF16_NORM: Self = Self::F16Norm;
    /// `'RGBAF32'`, which is [`F32`](ColorType::F32).
    pub const RGBAF32: Self = Self::F32;
}

#[cfg(test)]
mod tests {
    use super::{ColorSpace, ColorType};

    #[test]
    fn the_defaults_are_what_v1_defaulted_to() {
        // `RootProps.colorType` defaults to `'rgba'` and `colorSpace` to
        // `'srgb'` (`canvas.type.ts:1202`, `:1211`), which are these.
        assert_eq!(ColorType::default(), ColorType::Uint8);
        assert_eq!(ColorSpace::default(), ColorSpace::Srgb);
    }

    #[test]
    fn each_alias_names_the_space_v1_spells_it() {
        // Asserted one by one rather than derived, because there is nothing to
        // derive it from: which space `'hdr10'` means is a fact about v1's
        // vocabulary, not about either enum. A constant pointing at the wrong
        // variant resolves happily and composites in the wrong space, and on
        // this side that is one `assert_eq!` away from being caught -- which is
        // more than the TypeScript alias table can say for itself.
        assert_eq!(ColorSpace::LINEAR, ColorSpace::SrgbLinear);
        assert_eq!(ColorSpace::P3, ColorSpace::DisplayP3);
        assert_eq!(ColorSpace::P3_LINEAR, ColorSpace::DisplayP3Linear);
        assert_eq!(ColorSpace::BT2020, ColorSpace::Rec2020);
        assert_eq!(ColorSpace::BT2020_LINEAR, ColorSpace::Rec2020Linear);
        assert_eq!(ColorSpace::HDR10, ColorSpace::Rec2020Pq);
        assert_eq!(ColorSpace::HLG, ColorSpace::Rec2020Hlg);
    }

    #[test]
    fn each_layout_alias_names_the_layout_v1_spells_it() {
        assert_eq!(ColorType::RGBA, ColorType::Uint8);
        assert_eq!(ColorType::RGB, ColorType::Rgb888x);
        assert_eq!(ColorType::BGRA, ColorType::Bgra8888);
        assert_eq!(ColorType::RGBAF16, ColorType::F16);
        assert_eq!(ColorType::RGBAF16_NORM, ColorType::F16Norm);
        assert_eq!(ColorType::RGBAF32, ColorType::F32);

        // The one that matters most: v1's default spelling.
        assert_eq!(ColorType::RGBA, ColorType::default());
    }

    #[test]
    fn an_alias_is_a_second_name_and_not_a_second_space() {
        // The property the constants exist to preserve. Were they variants,
        // `ALL` would be fifteen, `from_wire` would have to pick one of two
        // names for a byte, and the generator would emit fifteen keywords for
        // eight spaces.
        assert_eq!(ColorSpace::ALL.len(), 8);
        assert_eq!(ColorType::ALL.len(), 23);
        assert_eq!(
            ColorSpace::HDR10.to_wire(),
            ColorSpace::Rec2020Pq.to_wire()
        );
        assert_eq!(
            ColorSpace::from_wire(ColorSpace::HDR10.to_wire()),
            Some(ColorSpace::Rec2020Pq)
        );
    }

    #[test]
    fn every_variant_has_its_own_wire_byte() {
        for enumeration in [
            ColorType::ALL
                .iter()
                .map(|value| u16::from(value.to_wire()))
                .collect::<Vec<_>>(),
            ColorSpace::ALL
                .iter()
                .map(|value| u16::from(value.to_wire()))
                .collect::<Vec<_>>(),
        ] {
            let mut seen = enumeration.clone();
            seen.sort_unstable();
            seen.dedup();
            assert_eq!(seen.len(), enumeration.len());
        }
    }
}

wire_enum! {
    /// What a render does when an image source cannot be resolved.
    ///
    /// **The split this governs is which source failed, not how badly.** A
    /// [`Path`](crate::node::ImageSource::Path) that cannot be read and
    /// [`Bytes`](crate::node::ImageSource::Bytes) that will not decode fail
    /// loudly whatever this says: the caller is holding the input and can
    /// check it before rendering. Only a
    /// [`Url`](crate::node::ImageSource::Url) consults this, because whether a
    /// fetch will succeed is a fact about the world at render time and no care
    /// upstream establishes it. As the report that prompted this put it, of the
    /// guard a consumer would write for themselves:
    ///
    /// > a URL that is present and well-formed and answers 404 passes through
    /// > it untouched. Every consumer that writes this helper will write it
    /// > with the same blind spot, because the information it would need —
    /// > whether the fetch will succeed — does not exist at the point where the
    /// > node is built.
    ///
    /// That is why the default is not [`Throw`](Self::Throw).
    ///
    /// **Every variant records a warning.** The render result carries one entry
    /// per source that failed, whichever of these is chosen, so turning the
    /// drawing off never turns the knowing off.
    #[derive(Default)]
    pub enum OnImageError {
        /// Draw a neutral mark in the box and let the render finish.
        ///
        /// The box keeps whatever extent it was given: an explicit width or
        /// height is honoured and an `auto` axis contributes zero, which is
        /// what Chrome does with a broken `<img>`. A box that comes out `0x0`
        /// is drawn as nothing, also as Chrome does.
        #[default]
        Placeholder = 0,
        /// Fail the whole render, as every version before this one did.
        ///
        /// The behaviour of `10.0.0-alpha.5` exactly, for a caller whose
        /// sources come from a manifest they control -- there a 404 means their
        /// own deployment is broken and finishing the render hides it.
        Throw = 1,
        /// Draw nothing at all, and still record the warning.
        ///
        /// **The warning is still recorded**, which is the whole difference
        /// between this and not noticing. A caller who finds the mark
        /// distracting keeps the diagnostic; the render result carries the same
        /// entries it would have under [`Placeholder`](Self::Placeholder).
        Ignore = 2,
    }
}
