import { Canvas, type CanvasRenderingContext2D } from 'meo-skia-canvas'
import { BoxNode } from '@/canvas/layout.canvas.js'
import { drawWithGradientMask } from '@/canvas/mask.canvas.js'
import type { BoxProps } from '@/canvas/canvas.type.js'

/** What each node saw on the context at the moment it drew, in the order the nodes drew. */
let trace: Array<[string, boolean]> = []

beforeEach(() => {
  trace = []
})

/**
 * A node that reports rather than draws.
 *
 * `_renderContent` is where a component's own drawing happens — `Text`, `Image` and `Path` all
 * override it — so a value read here is the value those would have drawn with.
 */
class Probe extends BoxNode {
  protected override async _renderContent(ctx: CanvasRenderingContext2D) {
    trace.push([this.name as string, ctx.dither])
  }
}

/** A probe that reports and then fails, for the case where the context has to be put back anyway. */
class FailingProbe extends BoxNode {
  protected override async _renderContent(ctx: CanvasRenderingContext2D) {
    trace.push([this.name as string, ctx.dither])
    throw new Error('drawing failed')
  }
}

// Labelled by `name`: `key` is rewritten to carry the ancestor chain, so it does not identify a
// node on its own.
const probe = (name: string, props: Partial<BoxProps> = {}) => new Probe({ name, width: 10, height: 10, ...props })

/**
 * Lays a tree out and draws it on a real context.
 *
 * Real rather than mocked: `dither` is renderer state with a renderer default, and a mock would be
 * asserting this file's idea of both.
 */
async function draw(root: BoxNode) {
  root.processInitialChildren()
  root.node.calculateLayout(undefined, undefined, root.props.direction)

  const ctx = new Canvas(100, 100).getContext('2d')
  await root.render(ctx, 0, 0)
  return ctx
}

describe('dither', () => {
  it('leaves the context at the renderer default when nothing asks for it', async () => {
    const ctx = await draw(new BoxNode({ name: 'root', width: 100, height: 100, children: [probe('a'), probe('b')] }))

    expect(trace).toEqual([
      ['a', false],
      ['b', false],
    ])
    expect(ctx.dither).toBe(false)
  })

  it('covers the whole page from the root', async () => {
    await draw(new BoxNode({ name: 'root', width: 100, height: 100, dither: true, children: [probe('a'), probe('b')] }))

    expect(trace).toEqual([
      ['a', true],
      ['b', true],
    ])
  })

  it('lets a node overrule the page for its own subtree', async () => {
    await draw(
      new BoxNode({
        name: 'root',
        width: 100,
        height: 100,
        dither: true,
        children: [probe('off', { dither: false, children: [probe('under-off')] }), probe('under-root')],
      }),
    )

    // `under-off` says nothing and takes its parent's answer rather than the root's, while
    // `under-root` says nothing and takes the root's: nearest ancestor wins, either way.
    expect(trace).toEqual([
      ['off', false],
      ['under-off', false],
      ['under-root', true],
    ])
  })

  it('does not carry one nodes answer across to its siblings', async () => {
    await draw(
      new BoxNode({
        name: 'root',
        width: 100,
        height: 100,
        dither: true,
        children: [probe('before'), probe('off', { dither: false }), probe('after')],
      }),
    )

    // `after` is drawn immediately following `off` and reads the root's answer rather than the one
    // its sibling just set. This is the whole reason the state is put back rather than only set.
    expect(trace).toEqual([
      ['before', true],
      ['off', false],
      ['after', true],
    ])
  })

  it('puts back what it found rather than what the renderer defaults to', async () => {
    await draw(new BoxNode({ name: 'root', width: 100, height: 100, children: [probe('on', { dither: true }), probe('after')] }))

    // Nothing above them asked for anything, so `after` falls to the renderer's default — which it
    // reaches by restoration, not because the default happens to be what was there.
    expect(trace).toEqual([
      ['on', true],
      ['after', false],
    ])
  })

  it('lets a node turn it back on inside a subtree that turned it off', async () => {
    await draw(
      new BoxNode({
        name: 'root',
        width: 100,
        height: 100,
        dither: true,
        children: [probe('off', { dither: false, children: [probe('on-again', { dither: true }), probe('still-off')] })],
      }),
    )

    expect(trace).toEqual([
      ['off', false],
      ['on-again', true],
      ['still-off', false],
    ])
  })

  it('puts the context back when a node fails partway through drawing', async () => {
    const failing = new FailingProbe({ name: 'boom', width: 10, height: 10, dither: false })
    const root = new BoxNode({ name: 'root', width: 100, height: 100, children: [failing] })

    root.processInitialChildren()
    root.node.calculateLayout(undefined, undefined, root.props.direction)

    const ctx = new Canvas(100, 100).getContext('2d')
    // Something the failing node has to put back, rather than the default it would land on anyway.
    ctx.dither = true

    await expect(root.render(ctx, 0, 0)).rejects.toThrow('drawing failed')

    // Both halves, because the value afterwards cannot tell a setting that was put back from one
    // that was never applied: the node drew with its own answer, and the context kept the one it
    // had for whatever draws next.
    expect(trace).toEqual([['boom', false]])
    expect(ctx.dither).toBe(true)
  })

  it('carries onto the offscreen a gradient mask composites through', async () => {
    const ctx = new Canvas(100, 100).getContext('2d')
    ctx.dither = true

    let offscreenDither: boolean | undefined
    const drawn = await drawWithGradientMask(
      ctx,
      { type: 'linear', direction: 'to-bottom', colors: ['#000000ff', '#00000000'] },
      { x: 0, y: 0, width: 50, height: 50 },
      async target => {
        offscreenDither = target.dither
      },
      '[test]',
    )

    expect(drawn).toBe(true)
    // The masked node draws on the offscreen instead of the page, so without this it would band
    // where an unmasked node beside it does not.
    expect(offscreenDither).toBe(true)
  })
})
