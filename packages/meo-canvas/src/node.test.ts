import { describe, expect, it } from 'vitest'

import { Box, Column, containerPropsOf, DEFAULT_ELLIPSIS, Grid, Image, NODE_KEYS, Path, RichText, Row, Text, type SceneNode } from './node.js'
import type { Style } from './style.js'

/** One of every factory, for the checks that must hold across all of them. */
const everyKind = (): readonly SceneNode[] => [
  Box(),
  Row(),
  Column(),
  Grid(),
  Text('x'),
  RichText([{ text: 'x', style: undefined }]),
  Image({ src: 'a.png' }),
  Path({ d: 'M0 0' }),
]

describe('the node shape', () => {
  it('carries every key on every node, in one order', () => {
    // The monomorphic claim, asserted rather than trusted. A node that
    // sometimes carries `src` gives V8 a second hidden class for one shape, and
    // every property read in the encoder deoptimises — a cost invisible until
    // something is profiled, which is why it is checked here instead.
    for (const node of everyKind()) {
      expect(Object.keys(node)).toEqual(NODE_KEYS)
    }
  })

  it('leaves an inapplicable field as undefined rather than absent', () => {
    const box = Box()

    expect('src' in box).toBe(true)
    expect(box.src).toBeUndefined()
    expect('segments' in box).toBe(true)
    expect(box.segments).toBeUndefined()
    expect('d' in box).toBe(true)
    expect(box.d).toBeUndefined()
  })

  it('names the kind each factory draws', () => {
    expect(Box().kind).toBe('box')
    expect(Row().kind).toBe('box')
    expect(Column().kind).toBe('box')
    expect(Grid().kind).toBe('box')
    expect(Text('x').kind).toBe('text')
    expect(Image({ src: 'a.png' }).kind).toBe('image')
    expect(Path({ d: 'M0 0' }).kind).toBe('path')
  })
})

describe('styles', () => {
  it('are written flat, in the props', () => {
    // The property a caller writes is the property, not a key inside a `style`
    // object.
    expect(Box({ gap: 16 }).style).toEqual({ display: 'flex', gap: 16 })
    expect(Text('x', { fontSize: 24 }).style).toEqual({ fontSize: 24 })
  })

  it('are read rather than copied, except by the containers that name a display', () => {
    // `Text`, `Image` and `Path` store the caller's own object: they name
    // nothing of their own, so there is nothing to spread.
    const props = { gap: 16 }

    expect(Text('x', props).style).toBe(props)

    const image = { src: 'a.png', gap: 16 }
    const path = { d: 'M0 0', gap: 16 }

    expect(Image(image).style).toBe(image)
    expect(Path(path).style).toBe(path)

    // `Box` names `display: flex`, so it copies the props: 0.03 to 0.08
    // microseconds per container, invisible next to a build or a render. It keeps a
    // `Box` a flex container whatever the scene's default, so `gap` and
    // `justifyContent` never silently stop.
    expect(Box(props).style).not.toBe(props)
    expect(Box(props).style).toEqual({ display: 'flex', gap: 16 })
  })

  it('carry the props that are not style properties, which nothing reads', () => {
    // The consequence of carrying the props object rather than a filtered
    // copy. `children` and `name` are not style property names, and the
    // encoder looks up only the names in its own table, so the extra keys cost
    // a read that never happens.
    const child = Text('x')

    expect(Box({ children: [child], name: 'card' }).style).toEqual({
      display: 'flex',
      children: [child],
      name: 'card',
    })
  })

  it('are copied once by the factories that name a direction, and the caller wins', () => {
    // `Row` and `Column` mean a direction, so they write one. Spreading the
    // caller's props after the default is what keeps an explicit value.
    expect(Row().style).toEqual({ display: 'flex', flexDirection: 'row' })
    expect(Column().style).toEqual({ display: 'flex', flexDirection: 'column' })
    expect(Grid().style).toEqual({ display: 'grid' })

    expect(Row({ flexDirection: 'column' }).style).toEqual({ display: 'flex', flexDirection: 'column' })
    expect(Grid({ display: 'flex' }).style).toEqual({ display: 'flex' })

    // A caller who wants a block container says so, and wins over the factory.
    expect(Box({ display: 'block' }).style).toEqual({ display: 'block' })
  })

  it('keep the caller’s other properties when a direction is added', () => {
    expect(Row({ gap: 8 }).style).toEqual({ display: 'flex', flexDirection: 'row', gap: 8 })
  })

  it('take a style object spread into the props', () => {
    // A caller who keeps a shared style in a variable spreads it, as CSS-in-JS
    // callers do. Nothing in the surface stops that, and this says so: a flat
    // props object is not a reason to give up a shared base.
    const theme: Style = { backgroundColor: '#101014', padding: 24 }

    expect(Box({ ...theme, gap: 16 }).style).toEqual({
      display: 'flex',
      backgroundColor: '#101014',
      padding: 24,
      gap: 16,
    })
  })
})

describe('containers', () => {
  it('take their children in order', () => {
    const first = Text('a')
    const second = Text('b')

    expect(Row({ children: [first, second] }).children).toEqual([first, second])
  })

  it('take a single child without an array around it', () => {
    const only = Text('a')

    expect(Box({ children: only }).children).toEqual([only])
  })

  it('drop a conditional that did not render', () => {
    // `condition && Text('…')` leaves a `false` behind, which has to disappear
    // rather than become a node.
    const shown = Text('a')
    const hidden = false

    expect(Box({ children: [shown, hidden, undefined] }).children).toEqual([shown])
    expect(Box({ children: false }).children).toEqual([])
  })

  it('hand a clean array through without copying it', () => {
    // The filter runs only when there is something to filter out, so the common
    // case allocates nothing — the same reason the style is not copied.
    const children = [Text('a'), Text('b')]

    expect(Box({ children }).children).toBe(children)
  })

  it('leave children undefined when none are given', () => {
    expect(Box().children).toBeUndefined()
  })

  it('carry a name through for diagnostics', () => {
    expect(Box({ name: 'card' }).name).toBe('card')
    expect(Box().name).toBeUndefined()
  })
})

describe('text', () => {
  it('takes its content as the first argument, as markup', () => {
    // The content is a parameter, not a key that could be forgotten, and lands in
    // `markup`, which the renderer parses: that is the distinction from `RichText`,
    // and building a segment here would lose it on the wire.
    expect(Text('Ukasyah').markup).toBe('Ukasyah')
    expect(Text('Ukasyah').segments).toBeUndefined()
  })

  it('leaves markup unset for runs the caller built', () => {
    // The other half of the discriminant. `RichText` is the only way to write
    // a literal `<`, which it can only be if nothing parses it.
    const node = RichText([{ text: 'a <b> b', style: undefined }])
    expect(node.markup).toBeUndefined()
    expect(node.segments).toEqual([{ text: 'a <b> b', style: undefined }])
  })

  it('carries paragraph properties apart from style', () => {
    // `maxLines` and `ellipsis` describe the block, not the glyphs, and nothing
    // inherits them — which is why the scene keeps them in their own struct and
    // so does this.
    const node = Text('x', { maxLines: 2, ellipsis: '...', fontSize: 12 })
    expect(node.paragraph).toEqual({ maxLines: 2, ellipsis: '...' })
    expect(Text('x').paragraph).toBeUndefined()
  })

  it('resolves every spelling of `ellipsis` to the marker or to nothing', () => {
    // `ellipsis` takes `true` and `false` as well as a string, and the node carries
    // the resolved marker, never the boolean: the scene holds what will be drawn,
    // and no measurer, line-breaker or painter reads which spelling asked for it.
    expect(Text('x', { maxLines: 1, ellipsis: true }).paragraph).toEqual({ maxLines: 1, ellipsis: DEFAULT_ELLIPSIS })
    expect(Text('x', { maxLines: 1, ellipsis: false }).paragraph).toEqual({ maxLines: 1 })
    expect(Text('x', { maxLines: 1, ellipsis: '—' }).paragraph).toEqual({ maxLines: 1, ellipsis: '—' })
    // An empty marker and no marker draw the same picture, so `''` is no marker.
    expect(Text('x', { maxLines: 1, ellipsis: '' }).paragraph).toEqual({ maxLines: 1 })
  })

  it('has no paragraph when the only thing written resolves to nothing', () => {
    // `ellipsis: false` is a value the caller wrote and resolves to no marker,
    // so the test on the way in cannot be the test on the way out: an early
    // return keyed on `undefined` would leave an empty object here where every
    // other path produces an absent one.
    expect(Text('x', { ellipsis: false }).paragraph).toBeUndefined()
    expect(Text('x', { ellipsis: '' }).paragraph).toBeUndefined()
  })

  it('spells the default marker as the character CSS uses', () => {
    // Measured in Chrome rather than picked: `text-overflow: ellipsis` in
    // Helvetica at 40px draws three dots 10px apart across 31px, which is a
    // literal U+2026; three full stops sit 7px apart across 26px.
    expect(DEFAULT_ELLIPSIS).toBe('\u2026')
    expect(DEFAULT_ELLIPSIS).not.toBe('...')
  })

  it('carries one segment per run when the runs differ', () => {
    const segments = [
      { text: 'plain ', style: undefined },
      { text: 'bold', style: { fontWeight: 'bold' } as const },
    ]

    expect(RichText(segments).segments).toBe(segments)
  })

  it('has no children, whatever it holds', () => {
    expect(Text('x').children).toBeUndefined()
    expect(RichText([{ text: 'x', style: undefined }]).children).toBeUndefined()
  })
})

describe('images', () => {
  it('read a bare string as a local path', () => {
    expect(Image({ src: 'avatar.png' }).src).toEqual({ path: 'avatar.png' })
  })

  it('carry an explicit source as it was written', () => {
    const url = { url: 'https://example.invalid/a.png' }
    const bytes = { bytes: new Uint8Array([1, 2]) }

    expect(Image({ src: url }).src).toBe(url)
    expect(Image({ src: bytes }).src).toBe(bytes)
  })
})

describe('paths', () => {
  it('carry their data', () => {
    expect(Path({ d: 'M2 8 L6 12 L14 3' }).d).toBe('M2 8 L6 12 L14 3')
  })
})

describe('the values a list ignores', () => {
  // The assertion is that the two lists agree, not that either skips four values:
  // a fix to one alone would pass and let the other drift. React 19.2.8 renders
  // nothing for `null` and `''`, and renders `0` as the text `0`, so `0` is kept.
  const ignorable = [false, true, undefined, null] as const

  it.each(ignorable)('a container drops %p from its children', value => {
    const kept = Box({ children: [Box(), value] }).children
    expect(kept).toHaveLength(1)
  })

  it.each(ignorable)('a paragraph drops %p from its segments', value => {
    const runs = RichText([{ text: 'a' }, value]).segments
    expect(runs).toHaveLength(1)
  })

  it('the two lists ignore exactly the same values', () => {
    // The columns, built rather than asserted one by one, so a value added to
    // one predicate and not the other fails here rather than in neither.
    const children = ignorable.filter(value => Box({ children: [Box(), value] }).children?.length === 1)
    const segments = ignorable.filter(value => RichText([{ text: 'a' }, value]).segments?.length === 1)
    expect(segments).toEqual(children)
    expect(children).toHaveLength(ignorable.length)
  })

  // The rows that keep the set honest: React renders these as text, so skipping
  // them would differ from what `l7aromeo/meo-canvas#44` measured, and a caller
  // writing `items.length && …` would lose a visible zero.
  it.each([0, NaN, 42, -1, 3.5, Infinity, 'hi', ' ', ''])('neither list drops %p', value => {
    // No cast: these are legal children now, which is the change.
    expect(Box({ children: [Box(), value] }).children).toHaveLength(2)
    expect(RichText([{ text: 'a' }, value]).segments).toHaveLength(2)
  })

  it('a string, number or bigint child becomes a text node', () => {
    const kids = Box({ children: [0, 'hi', 12n] }).children
    expect(kids).toHaveLength(3)
    expect(kids?.map(child => child.kind)).toEqual(['text', 'text', 'text'])
    // `String()`, as React uses — so a bigint loses its `n`, and a non-finite
    // number renders its spelling rather than being dropped by a finiteness
    // guard nobody measured.
    expect(kids?.map(child => child.segments?.[0]?.text)).toEqual(['0', 'hi', '12'])
  })

  it('a text child is literal, not markup', () => {
    // `Text`'s content is parsed as markup and a text child's is not: built on
    // `Text`, a caller's `<b>` would draw bold (ink 565) rather than as tags (820).
    const child = Box({ children: '<b>x</b>' }).children?.[0]
    expect(child?.markup).toBeUndefined()
    expect(child?.segments?.[0]?.text).toBe('<b>x</b>')
  })

  // **React's line, and the one thing that must not become permissive.**
  it('a plain object still throws', () => {
    expect(() => Box({ children: [{} as never] })).toThrow('it takes a node, a string or a number')
  })

  // The middle row of a three-valued control. React skips these *with a
  // console warning*; the only warning channel here is typed `ImageWarning`,
  // so they are skipped silently and the divergence is in the diagnostic.
  it.each([() => {}, Symbol('s')])('skips %p rather than refusing it', value => {
    expect(Box({ children: [Box(), value as unknown as undefined] }).children).toHaveLength(1)
  })

  // The rows a filter that ate too much would fail. Without these, a predicate
  // that returned true for everything passes every test above.
  it('keeps the entries that are real', () => {
    expect(Box({ children: [Box(), Box()] }).children).toHaveLength(2)
    expect(RichText([{ text: 'a' }, { text: 'b' }]).segments).toHaveLength(2)
  })

  it("leaves '' alone when it is content rather than a child", () => {
    // **The same literal, one argument apart, meaning two different things.**
    // `''` in a child list is an ignorable entry; `''` as a paragraph's text is
    // a legitimate empty string. A skip written too low in the stack swallows
    // both.
    expect(Text('').markup).toBe('')
    expect(RichText([{ text: '' }]).segments).toHaveLength(1)
  })
})

describe('a segment carrying a key it has no room for', () => {
  // Refused at the writer because excess-property checking fires only on a fresh
  // literal: of nine spellings two were caught at compile time, and `rows.map(r =>
  // ({ text, fontSize }))`, the case `RichText` exists for, was not.
  it('refuses a flat style key and names the segment', () => {
    expect(() => RichText([{ text: 'hi', fontSize: 30 } as never])).toThrow('segments[0] has no property "fontSize"')
  })

  it('refuses it however the object was built', () => {
    // The route the type gate cannot see, and the one that matters.
    const rows = [{ label: 'hi', size: 30 }]
    expect(() => RichText(rows.map(row => ({ text: row.label, fontSize: row.size })) as never)).toThrow('has no property "fontSize"')
  })

  it('names the index, so one bad run in twenty is findable', () => {
    expect(() => RichText([{ text: 'a' }, { text: 'b' }, { text: 'c', color: '#f00' } as never])).toThrow('segments[2]')
  })

  // **The suggestion is offered only where it is certainly right.** A key the
  // generated tables carry is a style property; one they do not carry might be
  // a typo for anything, and a confidently wrong suggestion sends the caller to
  // write a second broken call.
  it('suggests the nested spelling for a real style key', () => {
    expect(() => RichText([{ text: 'hi', fontSize: 30 } as never])).toThrow('did you mean style: { fontSize }?')
  })

  it('suggests nothing for a key that is not a style property', () => {
    expect(() => RichText([{ text: 'hi', zzz: 1 } as never])).toThrow(/has no property "zzz"/)
    expect(() => RichText([{ text: 'hi', zzz: 1 } as never])).not.toThrow(/did you mean/)
  })

  // The rows a check that refused everything would fail.
  it('accepts the spellings that are correct', () => {
    expect(() => RichText([{ text: 'hi' }])).not.toThrow()
    expect(() => RichText([{ text: 'hi', style: { fontSize: 30 } }])).not.toThrow()
    expect(() => RichText([])).not.toThrow()
  })

  it('checks only the segments it kept', () => {
    // An ignorable entry is dropped before the key check, so a `null` beside a
    // good segment must not be inspected for keys it could never have.
    expect(() => RichText([{ text: 'hi' }, null])).not.toThrow()
  })
})

describe('a first argument that is not a props object', () => {
  // `checkProps` checks the value before its keys: `Object.keys('hello')` is `"0"`
  // to `"4"`, and `42`, `true`, `[]`, `''` and a function have none, so without it
  // `Box(42)` would build a default box.

  const notProps = {
    'an empty string': '',
    'a string with keys': 'hello',
    'a number': 42,
    'a boolean': true,
    'a list': [],
    'a function': () => {},
    null: null,
  }

  describe.each(['Box', 'Row', 'Column', 'Grid', 'Image', 'Path'] as const)('%s', name => {
    const factory = { Box, Row, Column, Grid, Image, Path }[name]
    it.each(Object.entries(notProps))('refuses %s', (_label, value) => {
      expect(() => factory(value as never)).toThrow(`${name} takes a props object`)
    })
  })

  // **The two spellings the old code answered differently, asserted together.**
  // One had keys and one did not, and that was the whole difference between a
  // nonsense message and silence. Whatever they produce now, they produce the
  // same thing.
  it('answers a value with keys and a value without the same way', () => {
    const withKeys = (): unknown => Box('hello' as never)
    const without = (): unknown => Box(42 as never)
    expect(withKeys).toThrow('Box takes a props object')
    expect(without).toThrow('Box takes a props object')
  })

  it('no longer reports an index as an unknown property', () => {
    expect(() => Box('hello' as never)).not.toThrow('has no property "0"')
  })

  // **A list is named as a list rather than as the object it technically is.**
  // The mistake it comes from is specific -- a caller reaching for CSS's
  // four-value shorthand -- and "an object" gives them nothing to correct.
  it('names a list as a list', () => {
    expect(() => Box([] as never)).toThrow('it was given a list')
  })

  it('names the two factories whose first argument is not props', () => {
    expect(() => Text({ children: 'Hg' } as never)).toThrow('Text takes its text first and its props second')
    expect(() => RichText({ children: 'Hg' } as never)).toThrow('RichText takes its segments first and its props second')
    // The internal method names these used to arrive as.
    expect(() => Text({} as never)).not.toThrow('side value')
    expect(() => RichText({} as never)).not.toThrow('segments.every')
  })

  // **The rows a check that refused too much would fail.** Five of these
  // factories default their props to `{}`, so a guard written against
  // `arguments.length` or against `undefined` refuses the documented spelling
  // and passes every row above.
  it('still takes an omitted props argument', () => {
    expect(() => Box()).not.toThrow()
    expect(() => Row()).not.toThrow()
    expect(() => Text('Hg')).not.toThrow()
    expect(() => RichText(['Hg'])).not.toThrow()
  })

  it('still takes what each factory takes', () => {
    expect(() => Box({})).not.toThrow()
    expect(() => Box({ width: 4, height: 4 })).not.toThrow()
    expect(() => Text('Hg', {})).not.toThrow()
    expect(() => Text('', { fontSize: 12 })).not.toThrow()
    expect(() => RichText([{ text: 'Hg' }])).not.toThrow()
    expect(() => Image({ src: 'a.png' })).not.toThrow()
    expect(() => Path({ d: 'M0 0 L4 4' })).not.toThrow()
  })
})

describe('a container props key it does not have', () => {
  // An unknown key is refused on every factory. The allowlist is `STYLE_KEYS` plus
  // the factory's two structural keys, each with an exhaustiveness proof, so a key
  // added to `Style` or `ContainerProps` and not listed is a compile error.

  it.each(['Box', 'Row', 'Column', 'Grid'] as const)('%s refuses a key it has no room for', name => {
    const factory = { Box, Row, Column, Grid }[name]
    expect(() => factory({ nonsenseKey: 1 } as never)).toThrow(`${name} has no property "nonsenseKey"`)
  })

  it('refuses the v9 spelling that was silently ignored', () => {
    // `templateColumns` is v9's name for `gridTemplateColumns`; ignored, the grid
    // would fall back to auto-placement and lay out a tree the caller did not describe.
    expect(() => Grid({ display: 'grid', templateColumns: [40, 60] } as never)).toThrow('Grid has no property "templateColumns"')
  })

  it('refuses it however the props object was built', () => {
    // Excess property checking sees a fresh literal and nothing else, so this
    // route compiles clean and is the one the check exists for.
    const props = { display: 'grid', templateColumns: [40, 60] }
    expect(() => Grid(props as never)).toThrow('has no property "templateColumns"')
  })

  // The rows a check that refused too much would fail. Without them a predicate
  // that threw on everything passes every row above.
  it('accepts the keys a container has', () => {
    expect(() => Box()).not.toThrow()
    expect(() => Box({})).not.toThrow()
    expect(() => Box({ width: 10, height: 10, children: [], name: 'x' })).not.toThrow()
    expect(() => Grid({ display: 'grid', gridTemplateColumns: [40, 60] })).not.toThrow()
  })

  it('accepts a style key that lives in a payload rather than a style group', () => {
    // `objectFit`, `objectPosition` and `frame` are declared on `Style` and are
    // absent from the generated property tables. A list derived from those
    // tables would refuse them, which is why the list is proved against
    // `keyof Style` instead of built from the tables.
    expect(() => Column({ objectFit: 'cover' })).not.toThrow()
    expect(() => Column({ objectPosition: [0, 0] })).not.toThrow()
    expect(() => Column({ frame: 1 })).not.toThrow()
  })
})

describe('a wider props object narrowed to a container', () => {
  // `Root` holds surface options (`fonts`, `gpu`, `pages`, `scale`) beside the
  // page's own style, and builds each page from the container half of that object.

  it('keeps the container keys and drops the rest', () => {
    const wide = {
      width: 100,
      backgroundColor: '#f00',
      name: 'page',
      gpu: true,
      fonts: [],
      pages: 3,
      scale: 2,
    }
    expect(containerPropsOf(wide)).toEqual({
      width: 100,
      backgroundColor: '#f00',
      name: 'page',
    })
  })

  it('carries a style key that is not in the generated tables', () => {
    // `objectFit` is on `Style` and absent from the arena property tables, so a
    // filter built on those tables would drop it here rather than refuse it
    // loudly — a silent narrowing, which is worse than the spread it replaced.
    expect(containerPropsOf({ objectFit: 'cover', gpu: true })).toEqual({ objectFit: 'cover' })
  })

  it('does not invent keys the caller did not pass', () => {
    expect(containerPropsOf({})).toEqual({})
  })
})

describe('a text props key it does not have', () => {
  // `TextProps` is three sources and the paragraph options have a proof of their
  // own, so dropping `ellipsis` fails twice: at `PARAGRAPH_KEYS` and at `TEXT_KEYS`.
  it.each([
    ['Text', () => Text('hi', { nonsense: 1 } as never)],
    ['RichText', () => RichText([{ text: 'a' }], { nonsense: 1 } as never)],
  ])('%s refuses it', (name, build) => {
    expect(build).toThrow(`${name} has no property "nonsense"`)
  })

  it('accepts every source of its keys', () => {
    expect(() => Text('hi')).not.toThrow()
    expect(() => Text('hi', { maxLines: 2, ellipsis: '…' })).not.toThrow() // ParagraphProps
    expect(() => Text('hi', { fontSize: 20 })).not.toThrow() // Style
    expect(() => Text('hi', { name: 'x' })).not.toThrow() // its own
  })
})

describe('a path props key it does not have', () => {
  it('refuses it', () => {
    expect(() => Path({ d: 'M0 0', nonsense: 1 } as never)).toThrow('Path has no property "nonsense"')
  })

  // `PathProps` adds eleven of its own beside `Style`, so this is the list most
  // likely to be written short — and the proof is what makes that a compile
  // error rather than a silently narrower surface.
  it('accepts the eleven it does have', () => {
    expect(() =>
      Path({
        d: 'M0 0',
        viewBox: [0, 0, 1, 1],
        preserveAspectRatio: 'none',
        fill: '#f00',
        stroke: '#00f',
        lineWidth: 2,
        fillRule: 'evenodd',
        lineCap: 'round',
        lineJoin: 'bevel',
        lineDash: [2, 2],
        lineDashOffset: 1,
        name: 'p',
      }),
    ).not.toThrow()
  })
})

describe('an image props key it does not have', () => {
  it('refuses it', () => {
    expect(() => Image({ src: 'x', nonsense: 1 } as never)).toThrow('Image has no property "nonsense"')
  })

  it('accepts the keys it does have', () => {
    expect(() => Image({ src: 'x', name: 'i' })).not.toThrow()
  })

  // `objectFit`, `objectPosition` and `frame` are the three keys `Style`
  // declares that the generated property tables do not carry, and they are an
  // image's own business — so this is the factory where a table-derived
  // allowlist would have refused valid code most visibly.
  it('accepts the three style keys the generated tables omit', () => {
    expect(() => Image({ src: 'x', objectFit: 'cover', objectPosition: [0, 0], frame: 2 })).not.toThrow()
  })
})
