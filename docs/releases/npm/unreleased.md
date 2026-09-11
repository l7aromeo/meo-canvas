**Some of the entries below are workarounds rather than fixes**, and each says so
where it sits — no count here, because a count in prose is a claim nothing
checks and this file grows. The defect is in the layout engine this renderer sits on, and
what ships here is compensation around it — so the issue this repository filed
stays open after the release, by design rather than by oversight, and the entry
cites it anyway.

Two things follow for a caller. A compensation is removed the day the engine
stops needing it, and each is held to that by a test that fails on the day
rather than by anyone remembering — so the behaviour can move again, in the
direction of the browser. And a compensation is narrower than a fix: where one
is known to make some other shape worse, the entry names that shape rather than
leaving it to be met.

### Fixed

- A percentage height under a parent sized by `width` and `aspectRatio` came out
  at zero. The ratio settles the parent's height, but the rule deciding whether a
  percentage had anything to resolve against never consulted it, so the height
  was thrown away before layout saw it. `minHeight` and `maxHeight` percentages
  were dropped the same way and are fixed with it — a `minHeight: '200%'` child
  kept its own height rather than being floored.

  It settles against whatever gives the parent its width: a stated `width`, a
  percentage of another box, or shrink-to-fit content. A declared `height` still
  wins outright, and a parent with no ratio still resolves nothing, which is what
  a browser does.

  **This is a regression introduced in 10.0.0-alpha.6.** It renders correctly in
  alpha.5, so a caller who stopped there can move forward rather than back.
  Measured against Chrome across eleven cases before anything changed: three
  were nominated as controls and had to come out unchanged, and five came out
  unchanged in the end. Only the three are evidence — a control counts because
  it was named in advance. (#91)

- An `Image` with no `width` or `height` was resized by its surroundings. Two
  ways, and both are fixed: positioned with `position: 'absolute'` and opposing
  insets — `left` with `right`, or `top` with `bottom` — it was stretched to
  span them; placed in a container shorter than itself it was shrunk to fit.

  The rule now is the browser's. **An image with no size of its own keeps its own
  dimensions.** Insets position it rather than size it: the start inset is
  honoured and the end one gives way, so `left: 0; right: 0` puts a 60-wide
  picture at the left edge at 60 wide. A declared `width` **and** `height` still
  win: `inset: 0` with both at `'100%'` fills the box, as before. (#92)

- An `Image` with a `width` and no `height` came back at its own height in a
  block container. Given a 60x40 picture and `width: 200`, it drew 200 x 40
  where a browser draws 200 x 133 — the intrinsic height arriving untouched
  rather than the height the picture's ratio implies. A container of 30, 40, 90
  or no stated height all produced 40, so it was not the container's height
  either.

  The ratio now settles the free axis in block flow, as it already did for an
  absolutely positioned image. **Flex is unchanged and was never wrong**: an
  item with an `auto` cross size stretches to its line and the ratio does not
  stop it, so 200 x 40 is what a browser gives there too — **where there is a
  line to stretch to.** With nothing to stretch to the ratio settles the cross
  size after all, and the same 200-wide item comes back 200 x 133.33. Those rows were the
  control — a repair that moved them would have broken something Chrome agrees
  with us about. (#94)

- An `Image` whose `src` is an SVG document ignored `objectFit`. Under `'fill'`,
  `'contain'` and `'cover'` the document drew at its own size in the top-left
  corner of the box instead of being placed in it, and under `'scale-down'` it
  did the same wherever the box was smaller than the picture. `'none'` was
  always correct. A raster source was placed correctly throughout, so the same
  picture behaved differently depending on which format it arrived in.

  **One thing to expect rather than report:** `'fill'` on an SVG is not the
  rectangle it gives a bitmap, and that is correct. `objectFit` sizes the
  element, and an SVG document then lays itself out inside that box under its
  own `preserveAspectRatio` — `xMidYMid meet` unless the document says
  otherwise — so it fits uniformly and centres instead of stretching. On a box
  that does not share the picture's aspect, `'fill'` on a document lands where
  `'contain'` does. A bitmap carries no such rule and is stretched. The two
  formats agree under every other rule.

  Pinned by thirty measurements of Chrome across both source kinds at three box
  shapes, none of which shares the picture's aspect — a box that does collapses
  `'fill'`, `'contain'` and `'cover'` onto one rectangle and cannot tell a fixed
  renderer from a broken one. (#95)

- A `Box` with `aspectRatio` and no declared width ignored the ratio. It came
  back at its content's height and at a width of the height times the ratio, so
  a 30x10 child under `aspectRatio: 0.85` gave a 9x10 box where a browser gives
  29.98 x 35.28. The ratio now derives the height from the width the content
  asks for, on both surfaces, wherever the width is not stated.

  **Two things that did not change, because they were already right.** A ratio
  box whose content is taller than the ratio implies keeps the content's height
  and takes its width from it — 300 tall and 255 wide, not 35 tall — and one
  carrying a `minHeight` larger than the derived value does the same. Those
  agreed with the browser before this and are untouched by it, so a box that
  comes out 300 x 255 is correct rather than a regression. Same for a ratio box
  nested inside another: the pair resolves in order, the inner ends taller than
  the outer, and that is what a browser does.

  **What the pass is worth, since it is a workaround and a reader will ask.**
  A grown row container comes out 424 x 424, which is a browser's answer, where
  the layout engine underneath on its own gives 424 x 0 — a box with no height,
  and nothing inside it drawn.

  **This is a workaround rather than a fix.** The layout engine underneath
  resolves a ratio box's axes in the wrong order, which is not something this
  renderer can correct there, so it lays the page out twice and puts the ratio
  back between the two passes. The issue stays open until the engine is fixed, and a
  test pinned to what the engine does today fails the day it is — which is what
  stops the compensation outliving its reason.

  **A second case in the same area is the entry below**, and it is fixed in this
  release too: at a declared width a ratio-derived height capped the box rather
  than flooring it. (#97)

- A `Box` with `aspectRatio` and a width it got from its parent capped its
  height at the ratio instead of letting taller content push past it. A
  100-wide box with `aspectRatio: 0.85` holding 300 of content came back 118
  tall where a browser gives 300.

  Content now exceeds a ratio-derived height wherever the width is not itself a
  consequence of the ratio. The distinction is what a browser does rather than a
  simplification: where removing the ratio moves the width — a shrink-to-fit box
  — the old behaviour was already correct and is untouched, which is why a
  `300 x 255` box stays `300 x 255` rather than collapsing to its content.

  **One thing to expect rather than report: a scroll container still caps.**
  Measured on the same box, Chrome gives 117.64 under `overflow: 'hidden'`,
  `'scroll'` and `'auto'`, and 300 under `visible`. A scrolling box has no
  automatic minimum to restore, so that is agreement rather than a leftover.

  **This is a workaround rather than a fix**, and it shares the ratio-free solve
  the entry above already takes rather than repeating it — though when it fires
  it lays the page out once more of its own. CSS has two minimums on that axis where the engine
  underneath has one slot for them, which is the thing this cannot repair at
  source. The issue stays open until it can be. (#104)

- `textAlign: 'start'` and `'end'` did not flip under `direction: 'rtl'`.
  `'start'` was the left edge under both directions, so a right-to-left
  paragraph drew against the wrong margin, and a justified line's last row went
  with it. `'left'` and `'right'` were correct throughout and are unchanged —
  they are the two that are meant not to flip.

  Only the two direction-relative values moved. The four left-to-right rows of
  the comparison table were green before this and after it, which is what says
  the repair is about the direction rather than about alignment. (#109)

- A flex item with `flexGrow: 1` and an `aspectRatio` came out with no width in
  a column. Growing settles the item's height and the ratio should turn that
  into a width; instead the item was 0 wide where a browser gives 248 x 248, so
  anything inside it had nowhere to draw.

  The ratio now supplies the cross size wherever the item did not already get
  one from something else. None of the things that look like they should matter
  do: all five `alignItems` values behave the same, so do a `flexGrow` of 2 and
  two competing siblings, and so does the ratio's own value — measured across
  twenty-seven shapes.

  **An `Image` in that shape was already correct** and is unchanged; the shape
  that diverged is a plain `Box`. Do not read that as _a picture keeps its own
  size in flex_ — the entry above measures the opposite, an auto cross size
  taking 200 x 133.33 from the ratio rather than the picture's own 40. The image
  case was measured during this investigation and is recorded beside the probe
  rather than as a row in a comparison table, which is the weaker of the two
  places for it. The issue cited below is named for the
  image because that is what the original report named, and the investigation
  found the image was never the failing shape — so the title and this paragraph
  disagree, and this paragraph is the later reading.

  **A `maxWidth` or `maxHeight` on the cross axis clamps that axis and leaves
  the other one alone**, which is what a browser does: a grown item at
  `aspectRatio: 1` under `maxWidth: 100` is 100 x 248 rather than 100 x 100,
  and the same item under `maxWidth: 1` is 1 x 248 rather than 1 x 1. The
  engine underneath carries the maximum through the ratio into the other axis;
  this puts it back where it was written. A maximum on the _main_ axis still
  clamps that axis and lets the ratio settle the other, unchanged. (#129)

  **One case is still wrong and is worth knowing about.** An item stretched
  across its line and then asked for a main size from that stretch comes out
  424 x 248 where a browser gives 424 x 424 — which overflows its own 248-tall
  line. That one is deliberately not compensated: producing an overflowing box
  is a layout change nobody asked for, and the proposal to make it the engine's
  own answer is an open pull request rather than a shipped one.

  **It is a workaround and it is marked as one**, with a probe that fails the
  day the layout engine underneath stops needing it. (#123)

- A negative margin on a flex item with `flexGrow` was dropped from the
  container's height, and the item was laid out at the wrong size with it. A
  903-wide column holding a 500-tall child at `marginTop: -24` came out 500 tall
  where a browser gives 476, and the child was grown to 524 rather than kept at
  its own 500.

  **Both halves scale with the margin and neither is the one you notice first.**
  Where several growing children carry negative margins, every one of them was
  dropped rather than one — two 200-tall children at `-24` and `-10` gave 400
  against a browser's 366. Percentage margins were dropped in exactly the same
  way, `marginTop: '-10%'` of a 903-wide container giving 500 against 409.7. A
  margin too small to survive rounding into a rendered pixel was never
  observable either way, before or after.

  A static item was correct, a positive margin was correct, a row direction was
  correct, and a container with a stated height was correct — a container that
  is stretched by its parent never resolves its own height and never met this at
  all.

- A grid item with `overflow: 'hidden'` or `'scroll'` made an ancestor's height
  ignore a negative margin. A strip at `marginTop: -32` above a grid holding a
  100-tall clipping item gave an ancestor of 100 where a browser gives 68.

  **`'scroll'` is affected exactly as `'hidden'` is**, which is not what the
  names suggest: the two are grouped by the minimum size they give an item
  rather than by whether they clip. `'visible'` was correct, wrapping the item
  was correct, and a flex container in place of the grid was correct.

  Both are compensations and both are marked as such, each with a probe that
  fails the day the layout engine underneath stops needing it. (#107)
