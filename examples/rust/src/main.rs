//! A consumer, not a test.
//!
//! It reaches `meo-canvas` through a dependency rather than from inside the
//! workspace, so it fails if the crate's public surface is missing something a
//! caller needs — which the crate's own tests cannot notice, because they are
//! inside it.
//!
//! The output is something to look at. What the renderer draws correctly is
//! settled by the golden fixtures; this answers the different question of
//! whether a person can use the crate to draw anything at all.

use meo_canvas::{Canvas, Column, EncodeOptions, Format, Renderer, Row, Style, Text, hex, px};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let card = Row::new()
        .style(
            Style::new()
                .gap(px(20.0))
                .padding(all(px(24.0)))
                .background(hex("#101014")),
        )
        .children([Column::new()
            .style(Style::new().gap(px(6.0)))
            .children([
                Text::new("Ukasyah Rahmatullah Zada")
                    .style(Style::new().font_size(26.0).bold().color(hex("#f4f4f6"))),
                Text::new("meo-canvas — declarative scenes, rendered in Rust")
                    .style(Style::new().font_size(15.0).color(hex("#8a8a94"))),
            ])]);

    let renderer = Renderer::new();
    let mut canvas = Canvas::new(520.0, 180.0).page(card).render(&renderer)?;
    std::fs::write("out.png", canvas.to_buffer(Format::Png, &EncodeOptions::default())?)?;
    println!("wrote out.png");
    Ok(())
}
