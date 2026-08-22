import { describe, expect, it } from 'vitest'

import { Box, Column, Grid, Image, NODE_KEYS, Path, RichText, Row, Text, type SceneNode } from './node.js'
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
  it('are read rather than copied', () => {
    // No spread and no per-node merge: both cost per node on a path that has to
    // stay cheap, and the defaults already exist in Rust. The factories that
    // name no direction of their own hand the caller's object straight through.
    const style: Style = { gap: 16 }

    expect(Box({ style }).style).toBe(style)
    expect(Text('x', { style }).style).toBe(style)
    expect(Image({ src: 'a.png', style }).style).toBe(style)
    expect(Path({ d: 'M0 0', style }).style).toBe(style)
  })

  it('are copied once by the factories that name a direction, and the caller wins', () => {
    // `Row` and `Column` mean a direction, so they write one. Spreading the
    // caller's style after the default is what keeps an explicit value.
    expect(Row().style).toEqual({ flexDirection: 'row' })
    expect(Column().style).toEqual({ flexDirection: 'column' })
    expect(Grid().style).toEqual({ display: 'grid' })

    expect(Row({ style: { flexDirection: 'column' } }).style).toEqual({
      flexDirection: 'column',
    })
    expect(Grid({ style: { display: 'flex' } }).style).toEqual({ display: 'flex' })
  })

  it('keep the caller’s other properties when a direction is added', () => {
    expect(Row({ style: { gap: 8 } }).style).toEqual({
      flexDirection: 'row',
      gap: 8,
    })
  })
})

describe('containers', () => {
  it('take their children in order', () => {
    const first = Text('a')
    const second = Text('b')

    expect(Row({ children: [first, second] }).children).toEqual([first, second])
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
  it('takes its content as the first argument', () => {
    // A `Text` with no text is not a thing worth being able to write, so the
    // content is a parameter rather than a key that could be forgotten.
    expect(Text('Ukasyah').segments).toEqual([{ text: 'Ukasyah', style: undefined }])
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
