import { TextNode } from '@/canvas/text.canvas.js'
import { ColumnNode } from '@/canvas/layout.canvas.js'
import { FontLibrary } from 'meo-skia-canvas'
import { Style } from '@/constant/common.const.js'
import type { TextProps } from '@/canvas/canvas.type.js'
import { INTEGRATION_FONT_FAMILY, integrationRootBase } from './helpers/integration-font.js'

const FONT_SIZE = 20
const PHRASE = 'Sphinx of quartz'

/**
 * Chrome's width for the same string, font and size, from a shrink-wrapped inline-block.
 *
 * Yoga reports a whole number where Chrome reports a fraction, so each comparison allows the
 * rounding — but not more, which is what makes a spacing rule that is wrong by a unit visible.
 */
const CHROME = {
  plain: 145.13,
  letterSpacing: 177.13,
  negativeLetterSpacing: 129.13,
  wordSpacing: 161.13,
  both: 193.13,
  noBreakSpaces: 155.03,
  widestExplicitLine: 80.27,
} as const

const ROUNDING = 1.5

beforeAll(() => {
  FontLibrary.use({ [INTEGRATION_FONT_FAMILY]: [integrationRootBase.fonts![0].paths![0]] })
})

/** The width the node asks for, laid out in a parent too wide to constrain it. */
function intrinsicWidth(text: string, props: Partial<TextProps> = {}) {
  const node = new TextNode(text, { fontSize: FONT_SIZE, fontFamily: INTEGRATION_FONT_FAMILY, ...props })
  const parent = new ColumnNode({ width: 2000, alignItems: Style.Align.FlexStart })
  ;(parent as unknown as { appendChild(child: unknown, index: number): void }).appendChild(node, 0)
  parent.node.calculateLayout(2000, undefined, Style.Direction.LTR)
  return node.node.getComputedLayout().width
}

/** The height the node asks for, in the same unconstrained parent. */
function intrinsicHeight(text: string) {
  const node = new TextNode(text, { fontSize: FONT_SIZE, fontFamily: INTEGRATION_FONT_FAMILY })
  const parent = new ColumnNode({ width: 2000, alignItems: Style.Align.FlexStart })
  ;(parent as unknown as { appendChild(child: unknown, index: number): void }).appendChild(node, 0)
  parent.node.calculateLayout(2000, undefined, Style.Direction.LTR)
  return node.node.getComputedLayout().height
}

const expectWidth = (actual: number, expected: number) =>
  expect(Math.abs(actual - expected), `${actual} is more than ${ROUNDING} from ${expected}`).toBeLessThanOrEqual(ROUNDING)

describe('letterSpacing', () => {
  it('adds one spacing per character, as CSS does', () => {
    // The renderer applies spacing *between* characters, so a run comes back one unit short; text is
    // measured a word at a time and the spaces on their own, so the shortfall used to grow with the
    // word count. It was also added a second time by hand, on a premise that had stopped holding.
    expectWidth(intrinsicWidth(PHRASE, { letterSpacing: 2 }), CHROME.letterSpacing)
  })

  it('takes a negative spacing the same way', () => {
    expectWidth(intrinsicWidth(PHRASE, { letterSpacing: -1 }), CHROME.negativeLetterSpacing)
  })

  it('leaves the width alone at zero', () => {
    expectWidth(intrinsicWidth(PHRASE), CHROME.plain)
  })

  it('combines with wordSpacing', () => {
    expectWidth(intrinsicWidth(PHRASE, { letterSpacing: 2, wordSpacing: 8 }), CHROME.both)
  })
})

describe('wordSpacing', () => {
  it('adds one spacing per gap between words', () => {
    expectWidth(intrinsicWidth(PHRASE, { wordSpacing: 8 }), CHROME.wordSpacing)
  })
})

describe('whitespace', () => {
  it.each([
    ['a run of spaces', 'Sphinx    of     quartz'],
    ['leading and trailing space', '   Sphinx of quartz   '],
    ['a tab', 'Sphinx\tof quartz'],
  ])('collapses %s', (_label, text) => {
    expectWidth(intrinsicWidth(text), CHROME.plain)
  })

  it('keeps the no-break spaces, which CSS does not collapse', () => {
    // `\s` in JavaScript matches U+00A0, so treating it as collapsible loses exactly what a caller
    // reached for it to keep.
    expectWidth(intrinsicWidth('Sphinx   of quartz'), CHROME.noBreakSpaces)
  })
})

describe('an explicit newline', () => {
  it('asks for the widest line rather than every line end to end', () => {
    // `Sphinx` and `of quartz` are drawn on separate lines, so the box needs the wider of the two.
    // Summing them asked for nearly twice the room and left the rest of the box empty.
    expectWidth(intrinsicWidth('Sphinx\nof quartz'), CHROME.widestExplicitLine)
  })

  it('still lays the text out on two lines', () => {
    const node = new TextNode('Sphinx\nof quartz', { fontSize: FONT_SIZE, fontFamily: INTEGRATION_FONT_FAMILY })
    const parent = new ColumnNode({ width: 2000, alignItems: Style.Align.FlexStart })
    ;(parent as unknown as { appendChild(child: unknown, index: number): void }).appendChild(node, 0)
    parent.node.calculateLayout(2000, undefined, Style.Direction.LTR)

    // Narrowing the box must not have cost a line: the same text without the newline is one line
    // tall, and this is two.
    expect(node.node.getComputedLayout().height).toBeGreaterThan(intrinsicHeight('Sphinx of quartz') * 1.5)
  })
})
