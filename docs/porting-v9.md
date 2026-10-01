# Porting a v9 component

This document is the authority on carrying a component written for v9 -- the
predecessor, frozen at `9.0.4` on the `v9` branch -- to this renderer. It lists
where the same text means something different here, and the method that keeps
a port faithful. `MIGRATING.md` covers the API changes a caller meets; this
covers the ones that change a picture without an error.

Each hazard is listed with what it does when you get it wrong, because that
decides how much of a port has to be re-checked: a type error costs nothing; a
value silently wrong by a factor of the font size costs the whole render.

## Six hazards

1. **A bare `Box` runs the other way, and its shrink is a trap pointing the
   wrong direction.** v9's direction is Yoga's default, `column`; this renderer
   follows CSS and uses `row`, so every container writes its axis out. Yoga
   defaults `flex-shrink` to `0`, but v9's constructors put CSS's value back and
   all four declare `flexShrink: 1` -- so a v9 node that says nothing about
   shrinking means `1`, and taffy already means `1`. **The faithful port writes
   no `flex-shrink` at all.** _Writing `0` is a divergence dressed as a
   reproduction; pinned into a wrapper every node passes through, it moves the
   whole geometry._

2. **`lineHeight` is a different quantity.** v9's is the line box in
   **pixels**; this renderer's is a **multiple of the em size**.
   `lineHeight: 24` at 18px is 24 pixels there and 432 here. A component
   written in pixels may still hold a bare ratio, and those are the only values
   that carry over unchanged. _Wrong by a factor of the font size, and it looks
   like a layout defect rather than a unit mistake._

3. **`ellipsis` keeps v9's type, and `false` is the value to notice.** Both
   surfaces take `boolean | string`: `true` draws U+2026, measured in Chrome;
   `false`, `''` and omitting it all truncate without a marker. `false` is v9's
   own applied default, so the caller most likely to have written it is the one
   migrating. _Not a hazard; listed so a porter does not convert it._

4. **Edge groups are gone.** v9 spells `padding: { Horizontal: 2, Bottom: 2 }`;
   this renderer has only `top`, `right`, `bottom` and `left`. **TypeScript
   catches it; the runtime does not**: a node given
   `padding: { Horizontal: 16 }` renders with no padding and nothing is thrown.
   _Keep the port in TypeScript and the whole class is a compile error; leave it
   and the class is invisible._

5. **`<b>` inside a plain `Text` is markup there and literal text here**, which
   has `RichText` for the purpose. _Visible immediately: the tags draw._

6. **Capitalisation throughout**: `Style.PositionType.Absolute` becomes
   `'absolute'`, `position: { Top }` becomes `{ top }`. _As in 4: a compile
   error from TypeScript, silently dropped at runtime._

## The method

A v9 component's numbers are the geometry Chrome laid out for the template it
replaced. **The numbers are already the answer:** carry them rather than
re-deriving them, and when something is off, measure both renders and say by
how much. A component rebuilt from part of its source and an impression of the
rest is not a port and cannot be corrected into one.

**An asset that is not reachable gets a hatched plate at exactly the box the
real image would fill** -- never a guess at the layout around it, and never
nothing. _A missing asset must not be readable as a layout defect._
