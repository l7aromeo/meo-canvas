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
