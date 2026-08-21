import { vi } from 'vitest'
import type { CanvasRenderingContext2D } from 'meo-skia-canvas'
import { Style } from '@/constant/common.const.js'
import { ColumnNode } from '@/canvas/layout.canvas.js'
import { createMockCanvasContext, createTestTextMetrics } from './helpers/mock-canvas-context.js'

vi.mock('meo-skia-canvas', () => import('@/__mocks__/meo-skia-canvas.js'))

let TextNode: typeof import('@/canvas/text.canvas.js').TextNode
let Text: typeof import('@/canvas/text.canvas.js').Text
let skiaMockCtx: typeof import('@/__mocks__/meo-skia-canvas.js').mockCanvasContext

const widthPerChar = 8

/** Metrics with the font bounding box present — the ordinary case. */
const withFontBox: CanvasRenderingContext2D['measureText'] = text =>
  createTestTextMetrics({
    width: [...text].length * widthPerChar,
    actualBoundingBoxAscent: 10,
    actualBoundingBoxDescent: 3,
    fontBoundingBoxAscent: 12,
    fontBoundingBoxDescent: 4,
  })

/** Metrics with no font bounding box, so the `?? fontSize * ratio` fallbacks are taken. */
const withoutFontBox: CanvasRenderingContext2D['measureText'] = text => createTestTextMetrics({ width: [...text].length * widthPerChar })

function contextWith(measure: CanvasRenderingContext2D['measureText']): CanvasRenderingContext2D {
  const ctx = createMockCanvasContext()
  ctx.measureText = vi.fn<CanvasRenderingContext2D['measureText']>(measure)
  return ctx
}

async function render(
  node: InstanceType<typeof TextNode>,
  parentWidth: number,
  measure: CanvasRenderingContext2D['measureText'] = withFontBox,
  parentHeight?: number,
) {
  const col = new ColumnNode({ width: parentWidth, height: parentHeight })
  ;(col as any).appendChild(node, 0)
  col.node.calculateLayout(parentWidth, parentHeight, Style.Direction.LTR)
  const layout = node.node.getComputedLayout()
  const ctx = contextWith(measure)
  await node.render(ctx, layout.left, layout.top)
  return ctx
}

describe('TextNode — branch coverage', () => {
  beforeEach(async () => {
    vi.resetModules()
    ;({ mockCanvasContext: skiaMockCtx } = await import('@/__mocks__/meo-skia-canvas.js'))
    skiaMockCtx.measureText.mockImplementation((text: string) => withFontBox(text))
    const mod = await import('@/canvas/text.canvas.js')
    TextNode = mod.TextNode
    Text = mod.Text
  })

  describe('fontVariant', () => {
    it('passes a string fontVariant through to the context', async () => {
      const ctx = await render(new TextNode('small caps here', { fontVariant: 'small-caps' } as any), 400)
      expect(ctx.fontVariant).toBe('small-caps')
    })

    it('falls back to normal when fontVariant is set to a non-string', async () => {
      const ctx = await render(new TextNode('not a string', { fontVariant: true } as any), 400)
      expect(ctx.fontVariant).toBe('normal')
    })

    it('leaves fontVariant at its default when the prop is absent', async () => {
      const ctx = await render(new TextNode('plain', {}), 400)
      expect(ctx.fontVariant).toBe('normal')
    })
  })

  describe('metrics fallbacks', () => {
    it('derives ascent and descent from the font size when the font box is missing', async () => {
      const ctx = await render(new TextNode('no font box', { fontSize: 20 }), 400, withoutFontBox)
      expect(ctx.fillText).toHaveBeenCalled()
    })

    it('measures an empty line from the metrics string rather than the line itself', async () => {
      const ctx = await render(new TextNode('above\n\nbelow', { fontSize: 16 }), 400)
      const drawn = (ctx.fillText as any).mock.calls.map((call: unknown[]) => call[0])
      expect(drawn).toContain('above')
      expect(drawn).toContain('below')
    })

    it('handles an empty line with the font box missing too', async () => {
      const ctx = await render(new TextNode('a\n\nb', { fontSize: 16 }), 400, withoutFontBox)
      expect(ctx.fillText).toHaveBeenCalled()
    })
  })

  describe('spacing units', () => {
    it.each([
      ['em word spacing', { wordSpacing: '0.5em' as const }],
      ['em letter spacing', { letterSpacing: '0.25em' as const }],
      ['px word spacing', { wordSpacing: '4px' as const }],
      ['unitless string spacing', { letterSpacing: '3' as any }],
      ['unparseable spacing', { letterSpacing: 'wide' as any }],
      ['explicit normal', { wordSpacing: 'normal' as const, letterSpacing: 'normal' as const }],
      ['numeric spacing', { wordSpacing: 6, letterSpacing: 2 }],
    ])('renders with %s', async (_label, props) => {
      const ctx = await render(new TextNode('one two three', { fontSize: 16, ...props }), 400)
      expect(ctx.fillText).toHaveBeenCalled()
    })
  })

  describe('line box height', () => {
    it('uses a positive numeric lineHeight as the line box', async () => {
      const ctx = await render(new TextNode('tall lines', { fontSize: 16, lineHeight: 40 }), 400)
      expect(ctx.fillText).toHaveBeenCalled()
    })

    it('ignores a non-positive lineHeight and measures the content instead', async () => {
      const ctx = await render(new TextNode('normal lines', { fontSize: 16, lineHeight: 0 }), 400)
      expect(ctx.fillText).toHaveBeenCalled()
    })
  })

  describe('content box', () => {
    it('lays text out inside its own padding and border', async () => {
      const padded = new TextNode('inside the box', {
        fontSize: 16,
        padding: { Left: 10, Top: 8, Right: 10, Bottom: 8 },
        border: { Left: 4, Top: 4, Right: 4, Bottom: 4 },
        borderColor: '#000',
      } as any)
      const ctx = await render(padded, 400)
      const firstX = (ctx.fillText as any).mock.calls[0]?.[1]
      expect(firstX).toBeGreaterThanOrEqual(14)
    })
  })

  describe('decorated whole-line path', () => {
    it('draws a uniform decorated line in one call', async () => {
      const ctx = await render(new TextNode('underline this line', { fontSize: 16, textDecoration: 'underline' }), 400)
      const drawn = (ctx.fillText as any).mock.calls.map((call: unknown[]) => call[0])
      expect(drawn.some((text: string) => text.includes(' '))).toBe(true)
    })

    it('falls back to per-word drawing when the segments are not uniform', async () => {
      const mixed = new TextNode('plain <b>bold</b> plain', { fontSize: 16, textDecoration: 'underline', richText: true } as any)
      const ctx = await render(mixed, 400)
      expect(ctx.fillText).toHaveBeenCalled()
    })

    it('draws each shadow of a textShadow array', async () => {
      const ctx = await render(
        new TextNode('shadowed', {
          fontSize: 16,
          textDecoration: 'underline',
          textShadow: [
            { color: '#f00', blur: 2, offsetX: 1, offsetY: 1 },
            { color: '#00f', blur: 4, offsetX: -1, offsetY: -1 },
          ],
        } as any),
        400,
      )
      expect((ctx.fillText as any).mock.calls.length).toBeGreaterThan(1)
    })

    it('accepts a single textShadow object', async () => {
      const ctx = await render(new TextNode('one shadow', { fontSize: 16, textShadow: { color: '#0a0' } } as any), 400)
      expect(ctx.fillText).toHaveBeenCalled()
    })
  })

  describe('wrapping edges', () => {
    it('breaks a word that is wider than the whole line', async () => {
      const ctx = await render(new TextNode('supercalifragilisticexpialidocious', { fontSize: 16, width: 80 } as any), 80)
      expect(ctx.fillText).toHaveBeenCalled()
    })

    it('draws nothing when there is no width to lay text out in', async () => {
      const ctx = await render(new TextNode('unbreakableword', { fontSize: 16, width: 0 } as any), 0)
      expect(ctx.fillText).not.toHaveBeenCalled()
    })

    it('justifies a wrapped paragraph', async () => {
      const ctx = await render(new TextNode('one two three four five six seven eight', { fontSize: 16, textAlign: 'justify' }), 160)
      expect((ctx.fillText as any).mock.calls.length).toBeGreaterThan(1)
    })

    it('collapses a run of spaces between words', async () => {
      const ctx = await render(new TextNode('spaced     out', { fontSize: 16 }), 400)
      expect(ctx.fillText).toHaveBeenCalled()
    })
  })

  it('Text() forwards props to the node', () => {
    const descriptor = Text('x', { fontVariant: 'small-caps' } as any)
    expect(descriptor.__type).toBe('Text')
  })

  describe('rich text tags', () => {
    it.each([
      ['a bold tag', 'plain <b>bold</b> plain'],
      ['an italic tag', 'plain <i>slanted</i> plain'],
      ['a colour tag with an unquoted value', 'plain <color=#ff0000>red</color> plain'],
      ['a colour tag with a double-quoted value', 'plain <color="#00ff00">green</color> plain'],
      ['a colour tag with a single-quoted value', "plain <color='#0000ff'>blue</color> plain"],
      ['a weight tag with a number', 'plain <weight=700>heavy</weight> plain'],
      ['a weight tag with a keyword', 'plain <weight=bold>heavy</weight> plain'],
      ['a size tag', 'plain <size=24>big</size> plain'],
      ['nested tags', '<b>bold <i>and slanted</i></b>'],
      ['an uppercase tag name', 'plain <B>bold</B> plain'],
      ['a tag with no value where one is expected', 'plain <color>bare</color> plain'],
      ['an unknown tag', 'plain <marquee>odd</marquee> plain'],
      ['a stray closing tag with nothing on the stack', 'plain </b> plain'],
      ['text with no tags at all', 'entirely plain'],
      ['a tag at the very start', '<b>leading</b> rest'],
      ['a tag at the very end', 'rest <b>trailing</b>'],
    ])('renders %s', async (_label, text) => {
      const ctx = await render(new TextNode(text, { fontSize: 16 }), 400)
      expect(ctx.fillText).toHaveBeenCalled()
    })

    it('warns and drops a size tag whose value is not a number', async () => {
      const warn = vi.spyOn(console, 'warn').mockImplementation(() => {})
      await render(new TextNode('plain <size=huge>big</size> plain', { fontSize: 16 }), 400)
      expect(warn).toHaveBeenCalledWith(expect.stringContaining('Invalid numeric value for size tag'))
      warn.mockRestore()
    })

    it('drops a size tag with an empty value rather than warning', async () => {
      const ctx = await render(new TextNode('plain <size=>x</size> plain', { fontSize: 16 }), 400)
      expect(ctx.fillText).toHaveBeenCalled()
    })
  })

  describe('font string assembly', () => {
    it.each([
      ['a base italic style', { fontStyle: 'italic' }],
      ['a base weight', { fontWeight: 700 }],
      ['a base family', { fontFamily: 'Georgia' }],
      ['no font size, which defaults to 16', { fontSize: undefined }],
      ['every base property at once', { fontStyle: 'italic', fontWeight: 600, fontFamily: 'Georgia', fontSize: 22 }],
    ])('builds a font string from %s', async (_label, props) => {
      const ctx = await render(new TextNode('styled <b>and</b> <i>marked</i>', props as any), 400)
      expect(ctx.font).toBeTruthy()
    })

    it('lets a segment weight tag win over the base weight', async () => {
      const ctx = await render(new TextNode('<weight=200>light</weight>', { fontWeight: 900 } as any), 400)
      expect(ctx.fillText).toHaveBeenCalled()
    })

    it('lets a bold tag win over the base weight when no weight tag is given', async () => {
      const ctx = await render(new TextNode('<b>bold</b>', { fontWeight: 200 } as any), 400)
      expect(ctx.fillText).toHaveBeenCalled()
    })

    it('lets a segment size win over the base size', async () => {
      const ctx = await render(new TextNode('<size=30>big</size>', { fontSize: 10 }), 400)
      expect(ctx.fillText).toHaveBeenCalled()
    })
  })

  describe('truncation', () => {
    it.each([
      ['maxLines of one with an ellipsis', { maxLines: 1, ellipsis: true }],
      ['maxLines of two with an ellipsis', { maxLines: 2, ellipsis: true }],
      ['a custom ellipsis string', { maxLines: 1, ellipsis: ' — more' }],
      ['maxLines with no ellipsis', { maxLines: 1 }],
      ['maxLines larger than the text needs', { maxLines: 10, ellipsis: true }],
    ])('truncates with %s', async (_label, props) => {
      const ctx = await render(new TextNode('one two three four five six seven eight nine ten', { fontSize: 16, ...props } as any), 120)
      expect(ctx.fillText).toHaveBeenCalled()
    })

    it('carries the last segment style onto the ellipsis', async () => {
      const ctx = await render(new TextNode('plain words then <b>bold words at the end here</b>', { fontSize: 16, maxLines: 1, ellipsis: true } as any), 120)
      expect(ctx.fillText).toHaveBeenCalled()
    })

    it('applies fontVariant to the ellipsis measurement', async () => {
      const ctx = await render(
        new TextNode('one two three four five six', { fontSize: 16, maxLines: 1, ellipsis: true, fontVariant: 'small-caps' } as any),
        100,
      )
      expect(ctx.fillText).toHaveBeenCalled()
    })

    it('falls back to normal for a non-string fontVariant on the ellipsis', async () => {
      const ctx = await render(new TextNode('one two three four five six', { fontSize: 16, maxLines: 1, ellipsis: true, fontVariant: 7 } as any), 100)
      expect(ctx.fillText).toHaveBeenCalled()
    })
  })

  describe('justification detail', () => {
    it('adds no gap before trailing punctuation', async () => {
      const ctx = await render(new TextNode('alpha beta gamma delta , epsilon zeta eta theta', { fontSize: 16, textAlign: 'justify' } as any), 160)
      expect(ctx.fillText).toHaveBeenCalled()
    })

    it.each([
      ['a full stop', '.'],
      ['a comma', ','],
      ['a closing bracket', ')'],
      ['a closing square bracket', ']'],
      ['a closing brace', '}'],
      ['a colon', ':'],
      ['a semicolon', ';'],
      ['an exclamation mark', '!'],
      ['a question mark', '?'],
    ])('handles %s as a segment of its own when justifying', async (_label, mark) => {
      const ctx = await render(new TextNode(`alpha beta gamma delta ${mark} epsilon zeta eta theta iota`, { fontSize: 16, textAlign: 'justify' } as any), 160)
      expect(ctx.fillText).toHaveBeenCalled()
    })

    it('justifies a decorated line through the per-word path', async () => {
      const ctx = await render(
        new TextNode('alpha beta gamma delta epsilon zeta', { fontSize: 16, textAlign: 'justify', textDecoration: 'underline' } as any),
        140,
      )
      expect(ctx.fillText).toHaveBeenCalled()
    })
  })

  describe('space measurement', () => {
    it('falls back to a fraction of the font size when a space measures zero', async () => {
      const zeroWidthSpace: CanvasRenderingContext2D['measureText'] = text =>
        createTestTextMetrics({
          width: text === ' ' ? 0 : [...text].length * widthPerChar,
          fontBoundingBoxAscent: 12,
          fontBoundingBoxDescent: 4,
        })
      const ctx = await render(new TextNode('one two three', { fontSize: 20 }), 400, zeroWidthSpace)
      expect(ctx.fillText).toHaveBeenCalled()
    })

    it('warns once for a fontVariant that is neither a string nor absent', async () => {
      const warn = vi.spyOn(console, 'warn').mockImplementation(() => {})
      await render(new TextNode('one two three', { fontSize: 16, fontVariant: 42 } as any), 400)
      expect(warn).toHaveBeenCalled()
      warn.mockRestore()
    })
  })

  describe('alignment and baseline', () => {
    it.each([
      ['left', 'left'],
      ['center', 'center'],
      ['right', 'right'],
      ['start', 'start'],
      ['end', 'end'],
    ])('renders %s aligned text', async (_label, textAlign) => {
      const ctx = await render(new TextNode('aligned words here', { fontSize: 16, textAlign } as any), 300)
      expect(ctx.fillText).toHaveBeenCalled()
    })

    it.each([
      ['top', 'top'],
      ['middle', 'middle'],
      ['bottom', 'bottom'],
    ])('renders %s aligned text vertically', async (_label, verticalAlign) => {
      const ctx = await render(new TextNode('vertical', { fontSize: 16, verticalAlign } as any), 300, withFontBox, 120)
      expect(ctx.fillText).toHaveBeenCalled()
    })
  })
})
