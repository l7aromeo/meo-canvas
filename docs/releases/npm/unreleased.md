### Added

- `frame` takes a negative index, counting from the end: `frame: -1` draws the last frame of an animated image and `-2` the one before it. A source with one frame ignores a negative index as it does a positive one.

### Changed

- A chart given a negative value throws `[canvas] a chart cannot draw a negative value (got N)`; the message no longer compares the refusal with the previous major version.
- A `frame` past either end of an animated image is refused with `node N asks for frame F of an image with M frames`, naming the index as written and the frame count, rather than as an image whose bytes are in no format this decodes.

### Fixed

- An image URL whose host does not resolve is reported in `canvas.warnings` as `failure: 'host-not-found'` (do not retry) rather than `'transport'` (retry). A lookup that could not finish, because the resolver was unreachable or said to try again, stays `'transport'`.
- An SVG image given `frame: 1` or later draws, as a still raster does, rather than being refused.
- A `frame` that is not a whole number, or a `frame` or `zIndex` outside -2147483648 to 2147483647, is refused naming the property (`frame is 1.5; it takes a whole number`) rather than a slot the caller never wrote (`slot 41 holds 1.5, which is not an integer`).
