import { describe, expect, it } from 'vitest'

import { Box, Column, DEFAULT_ELLIPSIS, Grid, Image, NODE_KEYS, Path, RichText, Row, Text, type SceneNode } from './node.js'
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
    // object. v1 spells it this way and a ported tree should not have to move.
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

    // **`Box` copies now, and the reason the old comment gave for not copying
    // was never measured.** It said a spread "costs per node on a path that
    // has to stay cheap" -- while `Row` and `Column` had always spread, on
    // every call, in every example. Measured: 0.03 to 0.08 microseconds per
    // container, 3.1 ms across a hundred thousand of them, against a build of
    // 90 ms and a render of 8 to 22 ms for a tree of six thousand. It is not
    // visible next to the work it precedes.
    //
    // What it buys is that a `Box` is a flex container whatever the scene's
    // default becomes, which is the defect the default move would otherwise
    // have introduced: `gap`, `align_items` and `justify_content` silently
    // stopping.
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
    // `condition && Text('…')` is how a v1 caller writes a conditional, and the
    // `false` it leaves behind has to disappear rather than become a node.
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
    // A `Text` with no text is not a thing worth being able to write, so the
    // content is a parameter rather than a key that could be forgotten.
    //
    // It lands in `markup` rather than in a segment, and that is the whole
    // distinction from `RichText`: the renderer parses this string, so
    // `Text('a <b>b</b>')` is two runs by the time it is drawn. Building a
    // segment here would make the two indistinguishable on the wire and cost
    // every caller the rich text v1 gave them.
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
    // v1 types this `boolean | string` (`canvas.type.ts:1543`) and a ported
    // script writes `true`. Both booleans used to cross TypeScript unchecked
    // and be refused by the arena at the far end, which is a throw naming a
    // slot index rather than the property.
    //
    // `false` matters as much as `true`: it is v1's own applied default
    // (`text.canvas.ts:207`), so the caller most likely to have written it is
    // exactly the caller migrating.
    //
    // The node carries the resolved marker, never the boolean -- the scene
    // holds what will be drawn, and no measurer, line-breaker or painter reads
    // which spelling asked for it.
    expect(Text('x', { maxLines: 1, ellipsis: true }).paragraph).toEqual({ maxLines: 1, ellipsis: DEFAULT_ELLIPSIS })
    expect(Text('x', { maxLines: 1, ellipsis: false }).paragraph).toEqual({ maxLines: 1 })
    expect(Text('x', { maxLines: 1, ellipsis: '—' }).paragraph).toEqual({ maxLines: 1, ellipsis: '—' })
    // An empty marker and no marker draw the same picture, which is where v1's
    // truthiness guard landed and where a caller who wrote `''` still lands.
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
    // literal U+2026; three full stops sit 7px apart across 26px. v1 draws the
    // same character for `ellipsis: true` (`text.canvas.ts:1244`).
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
  // **The assertion is that the two lists agree, not that either skips four
  // values.** A fix to segments alone satisfies "segments skip four" and lets
  // children drift again later; only comparing the two columns catches that.
  //
  // Measured against React 19.2.8 before the repair: `null` and `''` render
  // nothing there, `false` and `undefined` already did here, and `0` renders
  // as the text `0` — which is why `0` is an error rather than a skip.
  const ignorable = [false, undefined, null, ''] as const

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

  // **`0` is the row that keeps this honest.** React renders it as text, so
  // skipping it would be a different decision from the one #44 measured, and a
  // caller writing `items.length && …` would silently lose a visible zero.
  // **The rows that keep the set honest.** React renders these as text, so
  // skipping them would be a different decision from the one measured — and a
  // caller writing `items.length && …` meaning "when there are items" would
  // lose a visible zero rather than seeing nothing.
  it.each([0, NaN])('neither list drops %p', value => {
    const keep = value as unknown as undefined
    expect(Box({ children: [Box(), keep] }).children).toHaveLength(2)
    expect(RichText([{ text: 'a' }, keep]).segments).toHaveLength(2)
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
  // **Refused at the writer because the type system refuses it almost
  // nowhere.** Excess property checking fires on a fresh object literal and on
  // nothing else. Measured across nine spellings, two were caught at compile
  // time and seven were not — including `rows.map(r => ({ text, fontSize }))`,
  // which is the case `RichText` exists for. All nine discarded the styling
  // silently at runtime.
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
