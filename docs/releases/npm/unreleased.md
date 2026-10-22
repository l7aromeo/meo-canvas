### Added

- `frame` takes a negative index, counting from the end: `frame: -1` draws the last frame of an animated image and `-2` the one before it. A source with one frame ignores a negative index as it does a positive one.

### Changed

- A chart given a negative value throws `[canvas] a chart cannot draw a negative value (got N)`; the message no longer compares the refusal with the previous major version.
- The page root's background colour fills the whole canvas beneath every effect the root carries, as the root element's does in Chrome: a root with `opacity` below 1, a `mask` or a `borderRadius` draws its colour opaque and out to the canvas's corners, with its content, gradient and image over it. Its `filter` applies to the whole canvas; its border and shadows stay on its box.
- A `frame` past either end of an animated image is refused with `node N asks for frame F of an image with M frames`, naming the index as written and the frame count, rather than as an image whose bytes are in no format this decodes.

### Fixed

- A text item that wraps is as wide as the space it is offered, as in Chrome, rather than as wide as its widest line, wherever it is not stretched: in a column under `alignItems: 'flex-start'` or `'center'`, and in a container sized to its content, such as an absolutely positioned box with only a `maxWidth`. In a 200-wide column with 50 of horizontal margin it was 137 wide where Chrome gives 150. Right- and centre-aligned lines move with the box, and an item cut to one line by `maxLines` is the full 150 rather than 134. Stretched items, rows and blocks were already right.
- An image URL whose host does not resolve is reported in `canvas.warnings` as `failure: 'host-not-found'` (do not retry) rather than `'transport'` (retry). A lookup that could not finish, because the resolver was unreachable or said to try again, stays `'transport'`.
- An SVG image given `frame: 1` or later draws, as a still raster does, rather than being refused.
- The page root's `margin` and `position` place it as Chrome places `<html>`, rather than being ignored: an unsized root fills the canvas less its margins, a percentage resolving against the canvas width on either axis, `'auto'` counting as no margin and a negative margin growing it; a `position` offset moves it without resizing it; and a page as tall as its content counts the root's vertical margins. Its background still covers the whole canvas, its gradient and image tiling from the root's box. A stated `width` and `height` are still the canvas size.
- An `Image` grid item under the default alignment draws at its intrinsic size, as Chrome does, rather than stretched to fill its cell. `alignItems: 'stretch'` still stretches it, its width following its ratio.
- A `transform`'s `translateX` and `translateY` are applied after its `rotate` and `scale`, as CSS applies `translate` after `rotate` and `scale`, rather than being rotated and scaled with the box. A box translated 20px and scaled 2 moves 20px, not 40px, and a 20px box at the page's top-left corner, translated 20px and turned 90° about that corner, covers x 0 to 19 rather than leaving the canvas. A transform with only a translation, or none, draws as before.
- A `frame` that is not a whole number, or a `frame` or `zIndex` outside -2147483648 to 2147483647, is refused naming the property (`frame is 1.5; it takes a whole number`) rather than a slot the caller never wrote (`slot 41 holds 1.5, which is not an integer`).
- A box that states no `direction` takes its parent's, as it does in Chrome (`l7aromeo/meo-canvas#165`): under a `direction: 'rtl'` parent a grid runs its tracks from the right, a flex row starts at the right, a block's child under a box stating no direction sits at the right, and `textAlign: 'start'` and `'end'` align to the right and left edges. An explicit `direction` still overrides the inherited one.
