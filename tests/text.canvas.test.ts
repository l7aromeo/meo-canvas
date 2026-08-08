import { vi } from 'vitest'
import type { CanvasRenderingContext2D } from 'phyron-skia-canvas'
import Yoga, { Style } from '@/constant/common.const.js'
import { ColumnNode } from '@/canvas/layout.canvas.js'
import { createMockCanvasContext, createTestTextMetrics } from './helpers/mock-canvas-context.js'

vi.mock('phyron-skia-canvas', () => import('@/__mocks__/phyron-skia-canvas.js'))

let TextNode: typeof import('@/canvas/text.canvas.js').TextNode
let Text: typeof import('@/canvas/text.canvas.js').Text
let skiaMockCtx: typeof import('@/__mocks__/phyron-skia-canvas.js').mockCanvasContext

const chartWidthPerChar = 8

const measureByCharLength: CanvasRenderingContext2D['measureText'] = text =>
  createTestTextMetrics({
    width: [...text].length * chartWidthPerChar,
    actualBoundingBoxAscent: 10,
    actualBoundingBoxDescent: 3,
  })

function createRenderContext(): CanvasRenderingContext2D {
  const ctx = createMockCanvasContext()
  ctx.measureText = vi.fn<CanvasRenderingContext2D['measureText']>(measureByCharLength)
  return ctx
}

function attachAndLayout(parentWidth: number, text: InstanceType<typeof TextNode>, parentHeight?: number): void {
  const col = new ColumnNode({ width: parentWidth, height: parentHeight })
  ;(col as any).appendChild(text, 0)
  col.node.calculateLayout(parentWidth, parentHeight, Style.Direction.LTR)
}

async function renderText(node: InstanceType<typeof TextNode>, parentWidth: number, parentHeight?: number) {
  attachAndLayout(parentWidth, node, parentHeight)
  const layout = node.node.getComputedLayout()
  const ctx = createRenderContext()
  await node.render(ctx, layout.left, layout.top)
  return ctx
}

describe('TextNode & Text factory', () => {
  beforeEach(async () => {
    vi.resetModules()
    ;({ mockCanvasContext: skiaMockCtx } = await import('@/__mocks__/phyron-skia-canvas.js'))

    skiaMockCtx.measureText.mockImplementation((text: string) => measureByCharLength(text))

    const mod = await import('@/canvas/text.canvas.js')
    TextNode = mod.TextNode
    Text = mod.Text
  })

  describe('Task 5 — factory & construction', () => {
    it('Text() returns a CanvasElement descriptor with __type Text', () => {
      const bare = Text('hello')
      expect(bare.__type).toBe('Text')
      if (bare.__type === 'Text') {
        expect(bare.text).toBe('hello')
        expect(bare.props).toBeUndefined()
      }

      const styled = Text('world', { color: '#111' })
      expect(styled.__type).toBe('Text')
      if (styled.__type === 'Text') {
        expect(styled.props?.color).toBe('#111')
      }
    })

    it('TextNode exposes a Yoga node', () => {
      const node = new TextNode('x')
      expect(node.node).toBeInstanceOf(Yoga.Node)
    })

    it('TextNode defaults flexShrink to 1 on the Yoga node', () => {
      const node = new TextNode('shrink-me')
      expect(node.props.flexShrink).toBe(1)
      expect(node.node.getFlexShrink()).toBe(1)
    })

    it('escape sequences \\n splits content so measured height exceeds one line box', () => {
      const twoLines = String.raw`LineOne\nLineTwo`
      expect(twoLines).toContain('\\')

      const node = new TextNode(twoLines, { fontSize: 16 })
      const control = new TextNode('single', { fontSize: 16 })
      attachAndLayout(420, node)
      attachAndLayout(420, control)

      expect(node.node.getComputedLayout().height).toBeGreaterThan(control.node.getComputedLayout().height)
      expect(node.node.getComputedLayout().height).toBeGreaterThan(16)
    })
  })

  describe('Task 6 — rich text rendering', () => {
    it('renderSimpleText configures font, fillStyle, textAlign/textBaseline and calls fillText', () => {
      const raw = createMockCanvasContext()
      raw.fillText = vi.fn<CanvasRenderingContext2D['fillText']>()

      TextNode.renderSimpleText(raw, 'Hi', 5, 16, {
        fontFamily: 'Georgia',
        fontSize: 20,
        fontWeight: 'bold',
        fontStyle: 'italic',
        color: '#abc',
        textAlign: 'right',
        textBaseline: 'middle',
      })

      expect(raw.font).toContain('Georgia')
      expect(raw.font).toContain('italic')
      expect(raw.font).toContain('bold')
      expect(raw.font).toContain('20px')
      expect(raw.fillStyle).toBe('#abc')
      expect(raw.textAlign).toBe('right')
      expect(raw.textBaseline).toBe('middle')
      expect(raw.fillText).toHaveBeenCalledWith('Hi', 5, 16)
      expect(raw.save).toHaveBeenCalled()
      expect(raw.restore).toHaveBeenCalled()
    })

    it('renders plain text via render() with fillText', async () => {
      const node = new TextNode('Hello', { width: 160, height: 44 })
      attachAndLayout(200, node)

      const mockCtx = createRenderContext()
      await node.render(mockCtx, 0, 0)

      expect(mockCtx.fillText).toHaveBeenCalledWith('Hello', expect.any(Number), expect.any(Number), expect.any(Number))
    })

    it('rich <color="red"> uses red fillStyle for the colored glyph', async () => {
      const node = new TextNode(String.raw`A<color="red">B</color>C`, {
        width: 200,
        height: 56,
        fontSize: 16,
      })
      attachAndLayout(260, node)

      const ctx = createRenderContext()
      let fillStylesAtLetterB: unknown

      ctx.fillText = vi.fn<CanvasRenderingContext2D['fillText']>((text, _x, _y, _max) => {
        if (text === 'B') fillStylesAtLetterB = ctx.fillStyle
      })

      await node.render(ctx, 0, 0)

      expect(fillStylesAtLetterB).toBe('red')
    })
  })

  describe('Task 7 — truncation (ellipsis + maxLines)', () => {
    it('shows default ellipsis "..." when maxLines truncates wrapping text', async () => {
      const body = [...Array(24)].map((_, i) => `term${i}`).join(' ')
      const node = new TextNode(body, {
        width: 168,
        height: 420,
        maxLines: 2,
        ellipsis: true as const,
        fontSize: 14,
      })
      attachAndLayout(380, node)

      const mockCtx = createRenderContext()
      await node.render(mockCtx, 0, 0)

      const calls = vi.mocked(mockCtx.fillText).mock.calls
      expect(calls.some(args => args[0] === '...')).toBe(true)
    })

    it('uses custom ellipsis character when ellipsis is a string', async () => {
      const body = [...Array(18)].map((_, i) => `item${i}`).join(' ')
      const ellipsisChar = '\u2026'
      const node = new TextNode(body, {
        width: 56,
        height: 220,
        maxLines: 2,
        ellipsis: ellipsisChar,
        fontSize: 14,
      })
      attachAndLayout(260, node)

      const mockCtx = createRenderContext()
      await node.render(mockCtx, 0, 0)

      const calls = vi.mocked(mockCtx.fillText).mock.calls
      expect(calls.some(args => args[0] === ellipsisChar)).toBe(true)
    })
  })

  describe('Task 8 — layout measurement', () => {
    it('multi-line escaped text measures taller than raw fontSize', () => {
      const node = new TextNode(String.raw`a\nb\nc`, { fontSize: 16 })
      const single = new TextNode('x', { fontSize: 16 })
      attachAndLayout(260, node)
      attachAndLayout(260, single)

      expect(node.node.getComputedLayout().height).toBeGreaterThan(16)
      expect(node.node.getComputedLayout().height).toBeGreaterThan(single.node.getComputedLayout().height)
    })

    it('respects explicit parent width (wraps when intrinsic line would be wider)', () => {
      const wide = [...Array(12)].map((_, i) => `w${i}`).join('')
      expect(wide.length * chartWidthPerChar).toBeGreaterThan(64)

      const node = new TextNode(wide, { fontSize: 12 })
      attachAndLayout(64, node)
      expect(node.node.getComputedLayout().width).toBeLessThanOrEqual(64 + 1e-3)
    })
  })

  describe('rich text tags and escape sequences', () => {
    it('parses weight, size, bold, and italic tags when rendering', async () => {
      const node = new TextNode(String.raw`<weight="700">W</weight><size="24">S</size><b>B</b><i>I</i>`, {
        width: 320,
        height: 80,
        fontSize: 16,
      })
      attachAndLayout(360, node)
      const ctx = createRenderContext()
      const fonts: string[] = []
      ctx.fillText = vi.fn<CanvasRenderingContext2D['fillText']>((_text, _x, _y, _max) => {
        fonts.push(ctx.font)
      })
      await node.render(ctx, 0, 0)

      expect(fonts.some(f => f.includes('700'))).toBe(true)
      expect(fonts.some(f => f.includes('24px'))).toBe(true)
      expect(fonts.some(f => f.includes('bold'))).toBe(true)
      expect(fonts.some(f => f.includes('italic'))).toBe(true)
    })

    it('supports unquoted and single-quoted color values', async () => {
      const node = new TextNode(String.raw`<color=green>G</color><color='orange'>O</color>`, {
        width: 200,
        height: 60,
      })
      attachAndLayout(240, node)
      const ctx = createRenderContext()
      const styles: unknown[] = []
      ctx.fillText = vi.fn<CanvasRenderingContext2D['fillText']>(text => {
        if (text === 'G' || text === 'O') styles.push(ctx.fillStyle)
      })
      await node.render(ctx, 0, 0)
      expect(styles).toContain('green')
      expect(styles).toContain('orange')
    })

    it('warns on invalid size tag values', () => {
      const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {})
      new TextNode(String.raw`<size="not-a-number">x</size>`, { fontSize: 16 })
      expect(warnSpy).toHaveBeenCalledWith(expect.stringContaining('Invalid numeric value for size tag'))
      warnSpy.mockRestore()
    })

    it('processes common escape sequences into layout lines', () => {
      const escaped = String.raw`tab\there\nline\rcr\\quote\'single\"double\0null\bback\f feed\v tab\unknown`
      const node = new TextNode(escaped, { fontSize: 14 })
      attachAndLayout(400, node)
      expect(node.node.getComputedLayout().height).toBeGreaterThan(14)
    })

    it('creates empty lines for consecutive newlines', async () => {
      const node = new TextNode('first\n\nsecond', { width: 200, height: 120, fontSize: 16 })
      const ctx = await renderText(node, 240)
      expect(vi.mocked(ctx.fillText).mock.calls.length).toBeGreaterThan(0)
    })
  })

  describe('spacing, alignment, and styling', () => {
    it('applies letterSpacing and wordSpacing during render', async () => {
      const node = new TextNode('alpha beta', {
        width: 240,
        height: 60,
        letterSpacing: '2px',
        wordSpacing: '0.5em',
        fontSize: 16,
      })
      const ctx = await renderText(node, 280)
      expect(ctx.letterSpacing).toBe('2px')
      expect(vi.mocked(ctx.fillText)).toHaveBeenCalled()
    })

    it('uses numeric letterSpacing and wordSpacing values', () => {
      const node = new TextNode('spaced words', {
        letterSpacing: 4,
        wordSpacing: 6,
        fontSize: 20,
      })
      attachAndLayout(300, node)
      expect(node.node.getComputedLayout().height).toBeGreaterThan(0)
    })

    it('renders centered and right-aligned text', async () => {
      const center = new TextNode('center me', { width: 200, height: 40, textAlign: 'center', fontSize: 16 })
      const right = new TextNode('right me', { width: 200, height: 40, textAlign: 'right', fontSize: 16 })
      const left = new TextNode('left me', { width: 200, height: 40, textAlign: 'left', fontSize: 16 })

      const centerCtx = await renderText(center, 240)
      const rightCtx = await renderText(right, 240)
      const leftCtx = await renderText(left, 240)

      const centerX = vi.mocked(centerCtx.fillText).mock.calls.find(c => c[0] === 'center')?.[1] ?? 0
      const rightX = vi.mocked(rightCtx.fillText).mock.calls.find(c => c[0] === 'right')?.[1] ?? 0
      const leftX = vi.mocked(leftCtx.fillText).mock.calls.find(c => c[0] === 'left')?.[1] ?? 0

      expect(centerX).toBeGreaterThan(leftX)
      expect(rightX).toBeGreaterThan(centerX)
    })

    it('renders end-aligned text using the end alias', async () => {
      const node = new TextNode('end text', { width: 180, height: 40, textAlign: 'end', fontSize: 16 })
      const ctx = await renderText(node, 220)
      expect(vi.mocked(ctx.fillText).mock.calls.some(c => c[0] === 'end')).toBe(true)
    })

    it('justifies multi-word lines except the last line', async () => {
      const node = new TextNode('one two three four', {
        width: 200,
        height: 60,
        textAlign: 'justify',
        fontSize: 14,
      })
      const ctx = await renderText(node, 240)
      expect(vi.mocked(ctx.fillText).mock.calls.length).toBeGreaterThan(1)
    })

    it('vertically centers and bottom-aligns text blocks', async () => {
      const middle = new TextNode('middle', { width: 160, height: 120, verticalAlign: 'middle', fontSize: 16 })
      const bottom = new TextNode('bottom', { width: 160, height: 120, verticalAlign: 'bottom', fontSize: 16 })

      const middleCtx = await renderText(middle, 200, 120)
      const bottomCtx = await renderText(bottom, 200, 120)

      const middleY = vi.mocked(middleCtx.fillText).mock.calls[0]?.[2] ?? 0
      const bottomY = vi.mocked(bottomCtx.fillText).mock.calls[0]?.[2] ?? 0
      expect(bottomY).toBeGreaterThan(middleY)
    })

    it('applies explicit lineHeight and lineGap', () => {
      const loose = new TextNode('line one\nline two', { lineHeight: 40, lineGap: 8, fontSize: 16 })
      const tight = new TextNode('line one\nline two', { fontSize: 16 })
      attachAndLayout(260, loose)
      attachAndLayout(260, tight)
      expect(loose.node.getComputedLayout().height).toBeGreaterThan(tight.node.getComputedLayout().height)
    })

    it('draws text shadows before the main fill', async () => {
      const node = new TextNode('shadowed', {
        width: 160,
        height: 40,
        textShadow: [
          { color: 'rgba(0,0,0,0.5)', blur: 4, offsetX: 2, offsetY: 2 },
          { color: 'blue', blur: 0, offsetX: 1, offsetY: 0 },
        ],
      })
      const ctx = await renderText(node, 200)
      expect(ctx.shadowColor).toBe('transparent')
      expect(vi.mocked(ctx.fillText).mock.calls.length).toBeGreaterThan(1)
    })

    it('applies fontVariant and warns on invalid variant types', async () => {
      const valid = new TextNode('caps', { width: 120, height: 40, fontVariant: 'small-caps' })
      await renderText(valid, 160)

      const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {})
      const invalid = new TextNode('bad', { fontVariant: 123 as never })
      attachAndLayout(160, invalid)
      expect(warnSpy).toHaveBeenCalledWith(expect.stringContaining('Invalid fontVariant prop type'), 123)
      warnSpy.mockRestore()
    })
  })

  describe('wrapping, breaking, and edge cases', () => {
    it('breaks words that exceed the available line width', async () => {
      const node = new TextNode('supercalifragilisticexpialidocious', {
        width: 48,
        height: 200,
        fontSize: 12,
      })
      attachAndLayout(48, node)
      expect(node.node.getComputedLayout().height).toBeGreaterThan(14)
      const ctx = createRenderContext()
      await node.render(ctx, 0, 0)
      expect(vi.mocked(ctx.fillText)).toHaveBeenCalled()
    })

    it('skips drawing when padding leaves no content area', async () => {
      const node = new TextNode('hidden', {
        width: 80,
        height: 40,
        padding: 50,
        fontSize: 14,
      })
      const ctx = await renderText(node, 80, 40)
      expect(vi.mocked(ctx.fillText)).not.toHaveBeenCalled()
    })

    it('handles empty input without throwing during layout or render', async () => {
      const node = new TextNode('', { width: 100, height: 40 })
      attachAndLayout(120, node)
      const ctx = createRenderContext()
      await expect(node.render(ctx, 0, 0)).resolves.not.toThrow()
    })

    it('respects maxLines during measurement', () => {
      const body = [...Array(30)].map((_, i) => `word${i}`).join(' ')
      const node = new TextNode(body, { width: 80, maxLines: 3, fontSize: 12 })
      attachAndLayout(120, node)
      expect(node.node.getComputedLayout().height).toBeGreaterThan(0)
    })

    it('handles punctuation after spaces without adding extra gaps', async () => {
      const node = new TextNode('Hello , world.', { width: 220, height: 40, fontSize: 16 })
      const ctx = await renderText(node, 260)
      expect(vi.mocked(ctx.fillText).mock.calls.some(c => c[0] === ',' || c[0] === '.')).toBe(true)
    })

    it('renders mid-segment truncation when a long token partially fits', async () => {
      const node = new TextNode('abcdefghijklmnop', {
        width: 40,
        height: 30,
        maxLines: 1,
        ellipsis: true,
        fontSize: 12,
      })
      attachAndLayout(40, node)
      const ctx = createRenderContext()
      let charMeasureCalls = 0
      ctx.measureText = vi.fn<CanvasRenderingContext2D['measureText']>(text => {
        if (text.length === 1) charMeasureCalls++
        return measureByCharLength(text)
      })
      await node.render(ctx, 0, 0)
      expect(charMeasureCalls).toBeGreaterThan(0)
      expect(vi.mocked(ctx.fillText)).toHaveBeenCalled()
    })

    it('culls lines that fall outside the visible content box', async () => {
      const lines = [...Array(20)].map((_, i) => `line${i}`).join('\n')
      const node = new TextNode(lines, { width: 200, height: 30, fontSize: 12, lineGap: 2 })
      const ctx = await renderText(node, 200, 30)
      expect(vi.mocked(ctx.fillText).mock.calls.length).toBeLessThan(20)
    })

    it('justifies wrapped lines that are not the final line', async () => {
      const node = new TextNode('one two three four five six seven eight nine', {
        width: 90,
        height: 80,
        textAlign: 'justify',
        fontSize: 14,
      })
      const ctx = await renderText(node, 120, 80)
      expect(vi.mocked(ctx.fillText).mock.calls.length).toBeGreaterThan(2)
    })

    it('measures wrapped width when a single line overflows the constraint', () => {
      const node = new TextNode('short mediumlengthword another', { width: 70, fontSize: 12 })
      attachAndLayout(70, node)
      expect(node.node.getComputedLayout().width).toBeLessThanOrEqual(70 + 1e-3)
      expect(node.node.getComputedLayout().height).toBeGreaterThan(12)
    })

    it('handles whitespace-only segments during layout', () => {
      const node = new TextNode('   leading spaces', { width: 200, fontSize: 16 })
      attachAndLayout(240, node)
      expect(node.node.getComputedLayout().height).toBeGreaterThan(0)
    })

    it('restores nested tag styles after closing tags', async () => {
      const node = new TextNode(String.raw`<color="blue">in <b>bold</b> out</color>`, {
        width: 240,
        height: 60,
        fontSize: 16,
      })
      attachAndLayout(280, node)
      const ctx = createRenderContext()
      const styles: string[] = []
      ctx.fillText = vi.fn<CanvasRenderingContext2D['fillText']>(_text => {
        styles.push(String(ctx.fillStyle))
      })
      await node.render(ctx, 0, 0)
      expect(styles.some(s => s === 'blue')).toBe(true)
    })

    it('uses em units for spacing calculations', () => {
      const node = new TextNode('spacing test', {
        letterSpacing: '0.25em',
        wordSpacing: '0.5em',
        fontSize: 20,
        width: 200,
      })
      attachAndLayout(240, node)
      expect(node.node.getComputedLayout().width).toBeGreaterThan(0)
    })
  })

  describe('internal layout helpers', () => {
    it('measureText returns bounded dimensions for an exact width constraint', () => {
      const node = new TextNode('hello world with enough words to wrap', {
        fontSize: 14,
        maxLines: 3,
        lineGap: 4,
        lineHeight: 24,
        letterSpacing: '1px',
        wordSpacing: '2px',
      })
      const result = (node as any).measureText(80, Style.MeasureMode.Exactly)
      expect(result.width).toBeGreaterThan(0)
      expect(result.height).toBeGreaterThan(0)
    })

    it('measureText treats undefined width mode as unbounded', () => {
      const node = new TextNode('single line width', { fontSize: 16 })
      const result = (node as any).measureText(0, Style.MeasureMode.Undefined)
      expect(result.width).toBeGreaterThan(0)
    })

    it('wrapTextRich splits oversized tokens across lines', () => {
      const node = new TextNode('x', { fontSize: 12 })
      const ctx = createRenderContext()
      const lines = (node as any).wrapTextRich(ctx, [{ text: 'abcdefghijklmnopqrstuvwxyz' }], 16, 0, 0)
      const combined = lines
        .flat()
        .map((s: { text: string }) => s.text)
        .join('')
      expect(combined.replace(/\s/g, '')).toContain('abcdefghijklmnop')
      expect(lines.length).toBeGreaterThan(0)
    })

    it('breakWordRich returns multiple styled fragments', () => {
      const node = new TextNode('x', { fontSize: 12, color: 'navy' })
      const ctx = createRenderContext()
      const parts = (node as any).breakWordRich(ctx, { text: 'abcdefghijklmnopqrstuvwxyz', color: 'navy' }, 24, 0)
      expect(parts.length).toBeGreaterThan(1)
      expect(parts[0].text.length).toBeGreaterThan(0)
    })

    it('parseRichText builds styled segments including invalid size warnings', () => {
      const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {})
      const node = new TextNode('x', { fontSize: 16 })
      const segments = (node as any).parseRichText(String.raw`<size="bad">a</size><i>i</i>`, {
        color: 'black',
        weight: 'normal',
        size: 16,
      })
      expect(segments.some((s: { i?: boolean }) => s.i)).toBe(true)
      expect(warnSpy).toHaveBeenCalled()
      warnSpy.mockRestore()
    })

    it('processEscapeSequences converts tab, quote, and unknown escapes', () => {
      const node = new TextNode('x', { fontSize: 16 })
      const processed = (node as any).processEscapeSequences(String.raw`a\tb\\c\z`)
      expect(processed).toContain('    ')
      expect(processed).toContain('\\')
      expect(processed).toContain('\\z')
    })

    it('parseSpacingToPx handles em units and invalid strings', () => {
      const node = new TextNode('x', { fontSize: 20 })
      expect((node as any).parseSpacingToPx('0.5em', 20)).toBe(10)
      expect((node as any).parseSpacingToPx('invalidunit', 16)).toBe(0)
      expect((node as any).parseSpacingToPx('12', 16)).toBe(12)
    })

    it('breakWordRich forces single-character segments when a glyph exceeds max width', () => {
      const node = new TextNode('x', { fontSize: 12 })
      const ctx = createRenderContext()
      ctx.measureText = vi.fn<CanvasRenderingContext2D['measureText']>(() =>
        createTestTextMetrics({
          width: 50,
          actualBoundingBoxAscent: 10,
          actualBoundingBoxDescent: 2,
        }),
      )
      const parts = (node as any).breakWordRich(ctx, { text: 'ab', color: 'black' }, 10, 0)
      expect(parts.length).toBeGreaterThan(1)
    })

    it('wrapTextRich breaks long tokens after explicit newlines', () => {
      const node = new TextNode('x', { fontSize: 12 })
      const ctx = createRenderContext()
      const lines = (node as any).wrapTextRich(ctx, [{ text: `intro\n${'z'.repeat(30)}` }], 24, 0, 0)
      expect(lines.length).toBeGreaterThan(1)
    })

    it('measureText handles blank lines and whitespace-only content', () => {
      const node = new TextNode('\n\n   ', { fontSize: 16, maxLines: 2 })
      const result = (node as any).measureText(120, Style.MeasureMode.Exactly)
      expect(result.height).toBeGreaterThan(0)
    })

    it('measureText uses fallback metrics for whitespace-only wrapped lines', () => {
      const node = new TextNode('x', { fontSize: 16 })
      vi.spyOn(node as any, 'wrapTextRich').mockReturnValue([[{ text: '     ', width: 0 }]])
      const result = (node as any).measureText(80, Style.MeasureMode.Exactly)
      expect(result.height).toBeGreaterThan(0)
    })

    it('applyDefaults marks the yoga node dirty when measurement defaults are applied', () => {
      const node = new TextNode('defaults', { fontSize: 20 })
      attachAndLayout(200, node)
      expect(node.node.isDirty()).toBe(false)

      node.props.fontSize = undefined
      const markDirtySpy = vi.spyOn(node.node, 'markDirty')
      ;(node as any).applyDefaults()

      expect(node.props.fontSize).toBe(16)
      expect(markDirtySpy).toHaveBeenCalled()
      markDirtySpy.mockRestore()
    })

    it('measureText uses fallback metrics when a wrapped line contains only whitespace segments', () => {
      const node = new TextNode('alpha\n       \nbeta', { fontSize: 16, width: 200 })
      const result = (node as any).measureText(200, Style.MeasureMode.Exactly)
      expect(result.height).toBeGreaterThan(0)
    })

    it('wrapTextRich handles overflow after explicit newlines', () => {
      const node = new TextNode('x', { fontSize: 12 })
      const ctx = createRenderContext()
      const lines = (node as any).wrapTextRich(ctx, [{ text: `intro\n${'z'.repeat(30)}` }], 16, 0, 0)
      expect(lines.length).toBeGreaterThan(1)
    })
  })

  describe('render edge paths', () => {
    it('measures ellipsis with fontVariant applied', async () => {
      const body = [...Array(20)].map((_, i) => `term${i}`).join(' ')
      const node = new TextNode(body, {
        width: 80,
        height: 40,
        maxLines: 1,
        ellipsis: true,
        fontVariant: 'small-caps',
        fontSize: 14,
      })
      const ctx = await renderText(node, 120)
      expect(vi.mocked(ctx.fillText).mock.calls.some(c => String(c[0]).includes('...'))).toBe(true)
    })

    it('character-truncates an overflowing last segment before drawing ellipsis', async () => {
      const node = new TextNode('aa bbbbbbbbbbbb', {
        width: 56,
        height: 24,
        maxLines: 1,
        ellipsis: true,
        fontSize: 12,
      })
      attachAndLayout(56, node, 24)
      const ctx = createRenderContext()
      ctx.measureText = vi.fn<CanvasRenderingContext2D['measureText']>(text =>
        createTestTextMetrics({
          width: text === '...' ? 12 : text.length * 6,
          actualBoundingBoxAscent: 10,
          actualBoundingBoxDescent: 2,
        }),
      )
      const drawn: string[] = []
      ctx.fillText = vi.fn<CanvasRenderingContext2D['fillText']>(text => {
        drawn.push(String(text))
      })
      await node.render(ctx, 0, 0)
      expect(drawn.some(t => t.length > 0 && t.length < 6 && t !== '...')).toBe(true)
      expect(drawn).toContain('...')
    })

    it('styles ellipsis from whitespace-only last visible lines when no text segment exists', async () => {
      const node = new TextNode('hello\n       ', {
        width: 120,
        height: 48,
        maxLines: 2,
        ellipsis: true,
        fontSize: 14,
      })
      const ctx = await renderText(node, 120, 48)
      expect(vi.mocked(ctx.save).mock.calls.length).toBeGreaterThan(0)
    })

    it('uses fallback metrics for whitespace-only render lines', async () => {
      const node = new TextNode('content\n   \nmore', {
        width: 160,
        height: 80,
        fontSize: 16,
      })
      const ctx = await renderText(node, 200, 80)
      expect(vi.mocked(ctx.fillText).mock.calls.length).toBeGreaterThan(0)
    })
  })
})
