//! A consumer, not a test.
//!
//! It reaches `meo-canvas` through a dependency rather than from inside the
//! workspace, so it fails if the crate's public surface is missing something a
//! caller needs — which the crate's own tests cannot notice, because they are
//! inside it.
//!
//! It is deliberately the same picture as `examples/bun`, written the same way
//! round. The two surfaces are meant to differ in syntax and not in shape, and
//! a reader comparing these two files is the check on that.

use meo_canvas::{Column, Format, Justify, Renderer, Root, Row, Text, hex, px};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let renderer = Renderer::new();

    let mut canvas = Root::new(520.0, 180.0)
        .background_color(hex("#101014"))
        .children(
            Row::new().gap(px(20.0)).padding(px(24.0)).children(
                Column::new()
                    .gap(px(6.0))
                    .justify_content(Justify::Center)
                    .children([
                        Text::new("Ukasyah Rahmatullah Zada")
                            .font_size(26.0)
                            .bold()
                            .color(hex("#f4f4f6")),
                        Text::new("meo-canvas — <b>declarative</b> scenes, rendered in Rust")
                            .font_size(15.0)
                            .color(hex("#8a8a94")),
                    ]),
            ),
        )
        .render(&renderer)?;

    canvas.to_file("out.png", Format::Png)?;
    println!("wrote out.png");
    Ok(())
}
