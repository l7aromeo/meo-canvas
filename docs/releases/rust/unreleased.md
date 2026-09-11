All of these but one are in `meo-canvas-core`, so they reach the Rust surface
and the Node addon alike. The exception is a dependency requirement, and it is
the one with a condition attached — see its entry. No count is given here
because a count in prose is a claim nothing checks.

### Fixed

- A percentage height under a parent sized by `size.0` and `aspect_ratio` came
  out at zero. The ratio settles the parent's height, but the rule deciding
  whether a percentage had anything to resolve against never consulted it, so
  `Dimension::Percent` was replaced with `Dimension::Auto` before taffy saw it.
  `min_size` and `max_size` percentages were dropped on the same condition and
  come back with it.

  The ratio settles the block axis whatever gives the inline axis its width — a
  `Dimension::Points`, a percentage, or shrink-to-fit content. A declared height
  is unaffected: it never reached the rule. A parent with no ratio still settles
  nothing, and a ratio that is not a positive finite number is discarded here
  exactly as it is on the way to taffy, so the two cannot disagree about what
  counts as one.

  **Present since 0.1.0-alpha.1**, which is this crate's first release, so there
  is no earlier version to fall back to. The change that caused it predates that
  tag; on the npm lineage, which ships from the same core on its own schedule,
  the same defect arrived in 10.0.0-alpha.6. Measured against Chrome across
  eleven cases before anything changed, five of which had to come out unchanged
  and did. (#91)

- A `NodeKind::Image` with `Dimension::Auto` on both axes was resized by its
  surroundings. Out of flow with opposing insets on an axis it was stretched to
  span them, because taffy's `Style` cannot say that a box is replaced and an
  over-constrained axis is sized from its insets — correct for a box, wrong for
  a picture. In flow, it was clamped to the available space and came back
  shorter than its own art.

  An image with no stated extent now keeps its intrinsic one. The end inset is
  dropped on an over-constrained axis and the start inset positions it, which is
  what Chrome does — every measured row sits at the start corner. A stated
  `Dimension::Points` or `Percent` on **both** axes still wins. (#92)

- A `NodeKind::Image` with a definite inline size and `Dimension::Auto` on the
  block axis came back at its intrinsic height in block flow. A 60x40 picture at
  `Dimension::Points(200.0)` laid out 200 x 40 where a browser gives 200 x 133,
  and containers of 30, 40, 90 and `Auto` all produced 40 — the intrinsic height
  arriving untouched rather than a height taken from the parent.

  The ratio now settles the free axis in block flow, as it already did out of
  flow. **`Display::Flex` is unchanged and was correct before**: a flex item with
  an `auto` cross size stretches to its line and the ratio does not override
  that, which is what a browser does, so those rows are the control on this
  change rather than evidence about it. (#94)

- A `NodeKind::Image` whose source is an SVG document ignored `ObjectFit` under
  every rule that scales. `Fill`, `Contain` and `Cover` drew the document at its
  intrinsic size at the content box's origin, and `ScaleDown` did the same
  wherever the box was smaller than the picture; `None` was correct throughout. A
  raster source was placed correctly, so one picture behaved two ways depending
  on the format it arrived in.

  The cause was in the rasteriser rather than in this crate:
  `Svg::rasterize` allocated a surface at the size asked for and then drew a root
  carrying absolute lengths at its intrinsic size, so a `viewBox` did not rescue
  it. **The fix is `meo-skia-canvas` 0.16.1, and the workspace now requires that
  version rather than `0.16`.**

  **The condition, because it decides when this reaches you.** The requirement is
  what a consumer resolves against; a lockfile in this repository is not read by
  anything that depends on the published crate. `meo-canvas-core` 0.1.0-alpha.2
  requires `^0.16`, which admits the broken 0.16.0 — so a caller depending on the
  published `meo-canvas` reaches the fix when **this** version of the core is on
  crates.io, not when the change merged. A caller building from a checkout has it
  already.

  **And one behaviour to expect rather than report:** `ObjectFit::Fill` on a
  document is not the rectangle it gives a bitmap. `object-fit` sizes the
  replaced element, and the document then lays itself out inside that box under
  its own `preserveAspectRatio`, defaulting to `xMidYMid meet`, so it meets
  uniformly instead of stretching. On a box whose aspect differs from the
  picture's, `Fill` on a document lands where `Contain` does; a bitmap has no
  such rule and stretches. Every other rule agrees across the two kinds. (#95)

- A `NodeKind::Box` with `aspect_ratio` and `Dimension::Auto` on both axes
  ignored the ratio, coming back at its content's height and at a width of
  `round(height x ratio)`: a 30x10 child under `0.85` solved to `9 x 10` where
  Chrome gives `29.98 x 35.28`. Every ratio box in a solved tree satisfied that
  expression, which is the browser's rule only where the height was settled by
  something other than the ratio.

  The height is now derived from the width the content asks for. This is in
  `meo-canvas-core`, so it reaches the Rust surface and the Node addon alike,
  and unlike the `meo-skia-canvas` fix above it needs no condition attached: the
  compensation is code in a crate this release publishes rather than a
  requirement a consumer resolves, so a caller of the published facade has it as
  soon as they have this version.

  **What did not change**, and both were already correct: a ratio box whose
  content exceeds the derived height keeps the content's height and transfers it
  back to the width — `300 x 255` rather than `35` tall — and so does one whose
  `min_size` height is larger. A ratio box nested inside another is also
  untouched, because a pair like that resolves in one ordered sweep that already
  matches Chrome.

  **It is a workaround and it is marked as one.** `[WORKAROUND]` in
  `crates/meo-canvas-core/src/layout.rs` names the upstream defect, and
  `crates/meo-canvas-core/tests/taffy_ratio_direction.rs` fails the day it is
  fixed upstream so the compensation cannot outlive its reason.

  **What this costs, and it is new in this release.** The compensation decides
  on a solve taken with the ratio cleared, reading `width / ratio >= height` off
  it, and that inequality has no clause about where the cross size came from. An
  item under `flex_grow` carrying `aspect_ratio: 1` and a binding `min_size`
  width of 300 solves to `300 x 248` against Chrome's `300 x 300` — which taffy
  alone produces, so on that shape the compensation is worse than no
  compensation. One measured case; a binding maximum on the same axis does not
  show it, so the boundary between the two has not been found.

  Stated rather than left to be met, and it does not outweigh the rest: the same
  pass takes a grown row container from `424 x 0` to Chrome's `424 x 424`. (#126)

  **A second case in the same area is the entry below**, and it is fixed in this
  release too: at a definite inline size the derived height capped the box
  rather than flooring it. (#97)

- A `NodeKind::Box` with `aspect_ratio` and an inline size settled by something
  other than the ratio capped its block size at the ratio's value instead of
  letting taller content push past it. A 100-wide box holding 300 of content
  solved to 118 tall against Chrome's 300.

  CSS has two minimums on that axis and taffy has one slot for them: an
  automatic minimum taken from the content does not transfer back into the
  inline axis, an author's `min_size` does, and writing the content-derived
  floor into `min_size.height` takes the same box to `255 x 300` — the right
  quantity in the wrong field. The compensation clears the ratio instead and
  shares the ratio-free solve the entry above already performs.

  Two conditions decide it and both were measured. The inline size must not be
  an outcome of the ratio, read as whether removing the ratio moves the width: a
  100-wide block box reports 100 either way, its shrink-to-fit sibling reports
  255 with and 30 without, and a predicate missing this condition takes
  `ratio-shrink-taller-content` from `300 x 255` to 30. And the box must not
  establish a scroll container — Chrome on the same box gives 117.64 under
  `Overflow::Hidden`, `Scroll` and `Auto`, and 300 under `Visible` — because a
  scrolling box has no automatic minimum to restore.

  **It is a workaround and it is marked as one**, in the same way as the entry
  above: `[WORKAROUND]` in `crates/meo-canvas-core/src/layout.rs`, with the
  probe in `crates/meo-canvas-core/tests/taffy_ratio_direction.rs` that fails
  the day taffy stops needing it. (#104)

- `TextAlign::Start` and `TextAlign::End` did not resolve against the node's
  direction. `Start` folded into the same arm as `Left`, so it was the left edge
  under `Direction::Rtl` as well — which is the enum's own documentation failing
  rather than a missing feature, since `Start` is defined as flipping and `Left`
  as not. `Left` and `Right` were correct throughout and are unchanged.

  Both readers of the alignment go through one resolution now, the placement and
  the justify decision alike, because a justified line reads the same value and
  would otherwise disagree with the line it sits in. The four left-to-right rows
  of the comparison table were green before and after, which is what makes this
  evidence about the direction rather than about alignment.

  The addon reaches it with no JavaScript change: `direction` and `textAlign`
  are neighbouring arena slots, so a JavaScript caller could already express the
  pair and both land in the same core. (#109)

- A flex item with `flex_grow` and an `aspect_ratio` came out with no inline
  size in a column. Growing settles the block axis and the ratio should transfer
  that into the inline one; the item solved to 0 wide where Chrome gives
  248 x 248.

  taffy applies a ratio's transferred size only where the item already has a
  cross contribution of its own, and that one condition has four causes that
  disagree about the number: an empty item contributes 0, padding alone
  contributes the padding, a border alone contributes 0, and a 30-pixel child
  contributes 30 — against the ratio's 248 in every case. So the compensation
  reads the outcome rather than the construction: an item whose cross size is
  not the ratio's derivation, whatever produced it.

  The item's own `Display` is deliberately not part of that test. It changes the
  answer in taffy — a block item with content gets the derivation, a flex item
  with the same content does not — and changes nothing in Chrome, where all four
  combinations of display and content are 248 x 248, so reading it would write a
  taffy artefact into this renderer.

  **A `NodeKind::Image` in that shape was already correct** and is untouched: a
  replaced element carries its own dimensions and never needed the transfer.

  **Two cases remain divergent, both on purpose.** An item wanting a main size
  derived from a stretched cross size is left alone, because Chrome's answer
  overflows its line and `DioxusLabs/taffy#1182` will produce it upstream — this
  renderer should not get there first. And a `max_size` binding the cross axis
  takes the other axis with it, so a maximum-bound item solves to 100 x 100
  against Chrome's 100 x 248; that is (#129).

  **It is a workaround and it is marked as one.** `[WORKAROUND]` in
  `crates/meo-canvas-core/src/layout.rs` names `DioxusLabs/taffy#804`, and
  `crates/meo-canvas-core/tests/taffy_flex_ratio.rs` fails the day taffy stops
  needing it. (#123)
