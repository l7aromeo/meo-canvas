import type { TextProps, TextSegment, CanvasElement } from '@/canvas/canvas.type.js'
import { Canvas, type CanvasRenderingContext2D, type FontVariantSetting } from 'meo-skia-canvas'
import { BoxNode } from '@/canvas/layout.canvas.js'
import { Style, MeasureMode } from '@/constant/common.const.js'

/**
 * Node for rendering text content with rich text styling support
 * Supports color and weight variations through HTML-like tags
 */
export class TextNode extends BoxNode {
  private readonly segments: TextSegment[] = []
  private lines: TextSegment[][] = []
  private static measurementContext: CanvasRenderingContext2D | null = null
  private readonly metricsString = 'Ag|``'
  private lineHeights: number[] = []
  private lineAscents: number[] = []
  private lineContentHeights: number[] = []

  declare props: TextProps & { lineGap: number }

  constructor(text: number | string = '', props: TextProps = {}) {
    const initialProps = {
      name: 'TextNode',
      flexShrink: 1,
      lineGap: 0,
      ...props,
      children: undefined,
    }
    super(initialProps)
    this.props = initialProps
    // Process escape sequences before parsing rich text
    const processedText = this.processEscapeSequences(String(text ?? ''))
    this.segments = this.parseRichText(processedText, {
      color: this.props.color,
      weight: this.props.fontWeight,
      size: this.props.fontSize,
      b: this.props.fontWeight === 'bold',
      i: this.props.fontStyle === 'italic',
    })
    this.node.setMeasureFunc(this.measureText.bind(this))
    this.applyDefaults()
  }

  /**
   * Renders a simple, single-line text string without complex layout calculations.
   * A lightweight, static utility for drawing text where layout is handled externally.
   * @param ctx The canvas rendering context.
   * @param text The string to render.
   * @param x The x-coordinate for rendering.
   * @param y The y-coordinate for rendering.
   * @param props Basic text styling properties.
   */
  public static renderSimpleText(
    ctx: CanvasRenderingContext2D,
    text: string,
    x: number,
    y: number,
    props: {
      fontFamily?: string
      fontSize?: number
      fontWeight?: TextProps['fontWeight']
      fontStyle?: TextProps['fontStyle']
      color?: string
      textAlign?: CanvasRenderingContext2D['textAlign']
      textBaseline?: CanvasRenderingContext2D['textBaseline']
    } = {},
  ) {
    ctx.save()

    const {
      fontFamily = 'sans-serif',
      fontSize = 12,
      fontWeight = 'normal',
      fontStyle = 'normal',
      color = '#333',
      textAlign = 'left',
      textBaseline = 'alphabetic',
    } = props

    ctx.font = `${fontStyle} ${fontWeight} ${fontSize}px ${fontFamily}`
    ctx.fillStyle = color
    ctx.textAlign = textAlign
    ctx.textBaseline = textBaseline

    ctx.fillText(text, x, y)

    ctx.restore()
  }

  protected override applyDefaults(): void {
    const textDefaults: Required<
      Pick<TextProps, 'fontSize' | 'fontFamily' | 'fontWeight' | 'fontStyle' | 'color' | 'textAlign' | 'verticalAlign' | 'ellipsis' | 'lineGap'>
    > & {
      lineHeight: undefined | number
      maxLines: undefined | number
      letterSpacing: undefined | number
      wordSpacing: undefined | number
      fontVariant: undefined | FontVariantSetting
    } = {
      fontSize: 16,
      fontFamily: 'sans-serif',
      fontWeight: 'normal',
      fontStyle: 'normal',
      color: 'black',
      textAlign: 'left',
      verticalAlign: 'top',
      fontVariant: undefined,
      lineHeight: undefined,
      lineGap: 0,
      maxLines: undefined,
      ellipsis: false,
      letterSpacing: undefined,
      wordSpacing: undefined,
    }

    let defaultsApplied = false
    for (const key of Object.keys(textDefaults) as (keyof typeof textDefaults)[]) {
      if (this.props[key] === undefined && textDefaults[key] !== undefined) {
        ;(this.props as unknown as Record<string, unknown>)[key] = textDefaults[key]
        defaultsApplied = true
      }
    }

    if (defaultsApplied && !this.node.isDirty()) {
      const affectsMeasurement = [
        'fontSize',
        'fontFamily',
        'fontWeight',
        'fontStyle',
        'lineHeight',
        'maxLines',
        'lineGap',
        'letterSpacing',
        'wordSpacing',
      ].some(measureKey => this.props[measureKey as keyof typeof textDefaults] === textDefaults[measureKey as keyof typeof textDefaults])
      if (affectsMeasurement) {
        this.node.markDirty()
      }
    }
  }

  /**
   * Processes Unix-like escape sequences in text strings.
   * Converts escaped characters into their actual representations.
   *
   * Supported escape sequences:
   * - \n - Newline (line feed)
   * - \t - Tab (converted to 4 spaces)
   * - \r - Carriage return (treated as newline)
   * - \\ - Literal backslash
   * - \' - Single quote
   * - \" - Double quote
   * - \0 - Null character (removed)
   * - \b - Backspace (removed)
   * - \f - Form feed (treated as newline)
   * - \v - Vertical tab (treated as newline)
   * @param input Raw text string potentially containing escape sequences
   * @returns Processed string with escape sequences converted
   */
  private processEscapeSequences(input: string): string {
    return input.replace(/\\(.)/g, (match, char) => {
      switch (char) {
        case 'n':
          return '\n' // Newline
        case 't':
          return '    ' // Tab as 4 spaces
        case 'r':
          return '\n' // Carriage return treated as newline
        case '\\':
          return '\\' // Literal backslash
        case "'":
          return "'" // Single quote
        case '"':
          return '"' // Double quote
        case '0':
          return '' // Null character (remove)
        case 'b':
          return '' // Backspace (remove)
        case 'f':
          return '\n' // Form feed as newline
        case 'v':
          return '\n' // Vertical tab as newline
        default:
          // Unknown escape sequence - keep original
          return match
      }
    })
  }

  /**
   * Parses input text with HTML-style markup into styled text segments.
   *
   * Supported tags:
   * - <color="value"> - Sets text color (hex code or CSS color name)
   * - <weight="value"> - Sets font weight (100-900 or keywords like "bold")
   * - <size="value"> - Sets font size in pixels
   * - <b> - Makes text bold (shorthand for weight="bold")
   * - <i> - Makes text italic
   *
   * Tag values can use double quotes, single quotes, or no quotes:
   * <color="red">, <color='red'>, <color=red>
   *
   * Tags can be nested and must be properly closed with </tag>
   * @param input Text string containing markup tags
   * @param baseStyle Default style properties to apply to all segments
   * @returns Array of styled text segments with consistent style properties
   */
  private parseRichText(input: string, baseStyle: Partial<TextSegment>): TextSegment[] {
    // Match opening/closing tags with optional quoted/unquoted values
    // Capture groups: (1) closing slash, (2) tag name, (3) double quoted value, (4) single quoted value, (5) unquoted value
    const tagRegex = /<(\/?)(\w+)(?:=(?:"([^"]*)"|'([^']*)'|([^\s>]+)))?>/g
    const stack: Partial<TextSegment>[] = []
    const segments: TextSegment[] = []
    let lastIndex = 0
    let currentStyle: Partial<TextSegment> = { ...baseStyle }

    // Helper to create a styled segment ensuring all style properties are included
    const applyStyle = (text: string) => {
      if (!text) return
      segments.push({
        text,
        color: currentStyle.color,
        weight: currentStyle.weight,
        size: currentStyle.size,
        b: currentStyle.b,
        i: currentStyle.i,
      })
    }

    let match: RegExpExecArray | null
    while ((match = tagRegex.exec(input))) {
      const [, closingSlash, tagNameStr, quotedVal1, quotedVal2, unquotedVal] = match
      const tagName = tagNameStr.toLowerCase()
      const value = quotedVal1 || quotedVal2 || unquotedVal

      // Process text content before the current tag
      applyStyle(input.slice(lastIndex, match.index))
      lastIndex = tagRegex.lastIndex

      if (!closingSlash) {
        // Opening tag: Save current style state and apply new style
        stack.push({ ...currentStyle })

        switch (tagName) {
          case 'color':
            // Support any valid CSS color value
            currentStyle.color = value as TextSegment['color']
            break

          case 'weight':
            // Support numeric weights (100-900) or keywords
            currentStyle.weight = value as TextSegment['weight']
            break

          case 'size':
            // Parse pixel size as number, revert to default if invalid
            currentStyle.size = value ? Number(value) : undefined
            if (isNaN(currentStyle.size as number)) {
              console.warn(`[TextNode ${this.key || ''}] Invalid numeric value for size tag: ${value}`)
              currentStyle.size = undefined
            }
            break

          case 'b':
            // Simple bold flag
            currentStyle.b = true
            break

          case 'i':
            // Simple italic flag
            currentStyle.i = true
            break
        }
      } else {
        // Closing tag: Restore previous style state
        currentStyle = stack.pop() || { ...baseStyle }
      }
    }

    // Process remaining text after last tag
    applyStyle(input.slice(lastIndex))

    // Don't filter out empty segments - they might represent empty lines
    return segments
  }

  private formatSpacing(value: TextProps['letterSpacing'] | TextProps['wordSpacing']) {
    if (typeof value === 'number') return `${value}px`
    return value || 'normal'
  }

  private parseSpacingToPx(spacingValue: number | string | undefined, fontSize: number): number {
    if (spacingValue === undefined || spacingValue === 'normal') {
      return 0
    }
    if (typeof spacingValue === 'number') {
      return spacingValue // Treat raw number as px
    }
    if (typeof spacingValue === 'string') {
      const trimmed = spacingValue.trim()
      if (trimmed.endsWith('px')) {
        return parseFloat(trimmed) || 0
      }
      if (trimmed.endsWith('em')) {
        // Convert em based on the current font size
        return (parseFloat(trimmed) || 0) * fontSize
      }
      // Attempt to parse as a raw number (pixels) if no unit
      const parsed = parseFloat(trimmed)
      if (!isNaN(parsed)) {
        return parsed
      }
    }
    return 0 // Default fallback
  }

  /**
   * Adds manual letter spacing compensation to a measured text width.
   * Needed because skia-canvas ctx.measureText() does not include letterSpacing in the returned width,
   * even though letterSpacing IS applied during rendering (fillText/strokeText).
   */
  private addLetterSpacingExtra(text: string, measuredWidth: number, letterSpacingPx: number): number {
    if (letterSpacingPx === 0 || text.length === 0) return measuredWidth
    const charCount = [...text].length
    return measuredWidth + (charCount > 1 ? (charCount - 1) * letterSpacingPx : 0)
  }

  /**
   * Generates a CSS font string by combining base TextProps with optional TextSegment styling.
   * Follows browser font string format: "font-style font-weight font-size font-family"
   *
   * Priority for style properties:
   * - Weight: segment <weight> tag > segment <b> tag > base fontWeight prop
   * - Style: segment <i> > base fontStyle
   * - Size: segment size > base fontSize
   * - Family: base fontFamily
   * @param segmentStyle Optional TextSegment styling to override base props
   * @returns Formatted CSS font string for canvas context
   */
  private getFontString(segmentStyle?: Partial<TextSegment>): string {
    const baseStyle = this.props
    let effectiveWeight: TextSegment['weight'] | number | undefined

    // Determine italic style - segment <i> tag overrides base style
    const effectiveStyle = segmentStyle?.i ? 'italic' : baseStyle.fontStyle || 'normal'

    // Determine font weight with priority:
    // 1. Segment explicit weight (<weight> tag)
    // 2. Segment bold flag (<b> tag)
    // 3. Base font weight prop
    if (segmentStyle?.weight) {
      effectiveWeight = segmentStyle.weight
    } else if (segmentStyle?.b) {
      effectiveWeight = 'bold'
    } else {
      effectiveWeight = baseStyle.fontWeight || 'normal'
    }

    // Use segment size if specified, otherwise base size with 16px default
    const effectiveSize = segmentStyle?.size ? segmentStyle.size : baseStyle.fontSize || 16

    // Combine properties into CSS font string format
    const style = {
      fontStyle: effectiveStyle,
      fontWeight: effectiveWeight,
      fontSize: effectiveSize,
      fontFamily: baseStyle.fontFamily || 'sans-serif',
    }

    return `${style.fontStyle} ${style.fontWeight} ${style.fontSize}px ${style.fontFamily}`
  }

  /**
   * Gets lines to process respecting maxLines constraint
   */
  private getLinesToMeasureOrRender(): TextSegment[][] {
    const maxLines = this.props.maxLines
    if (maxLines !== undefined && maxLines > 0 && this.lines.length > maxLines) {
      return this.lines.slice(0, maxLines)
    }
    return this.lines
  }

  /**
   * Measures text dimensions and calculates layout metrics for the YogaLayout engine.
   * Handles text wrapping, line height calculations, and dynamic leading.
   *
   * Line heights are determined by:
   * 1. Using props.lineHeight as fixed pixel value if provided
   * 2. Otherwise calculating dynamic height based on largest font size per line
   * 3. Adding leading space above/below text content
   * 4. Including specified line gaps between lines
   * @param widthConstraint Maximum allowed width in pixels for text layout
   * @param widthMode YogaLayout mode determining how width constraint is applied
   * @returns Calculated minimum dimensions required to render text content
   * - width: Total width needed for text layout
   * - height: Total height including line heights and gaps
   */
  private measureText(widthConstraint: number, widthMode: MeasureMode): { width: number; height: number } {
    // Create measurement canvas if not exists
    if (!TextNode.measurementContext) {
      TextNode.measurementContext = new Canvas(1, 1).getContext('2d')
    }
    const baseFontSize = this.props.fontSize || 16
    const ctx = TextNode.measurementContext!
    ctx.save()

    // Setup text measurement context
    ctx.letterSpacing = this.formatSpacing(this.props.letterSpacing)
    ctx.wordSpacing = 'normal' // Handled manually via parsedWordSpacingPx
    const parsedWordSpacingPx = this.parseSpacingToPx(this.props.wordSpacing, baseFontSize)
    const parsedLetterSpacingPx = this.parseSpacingToPx(this.props.letterSpacing, baseFontSize)

    // Pre-measure each text segment width with its specific styling
    for (const segment of this.segments) {
      ctx.font = this.getFontString(segment)
      this._applyFontVariant(ctx, 'measureText (segment width)')
      segment.width = this.addLetterSpacingExtra(segment.text, ctx.measureText(segment.text).width, parsedLetterSpacingPx)
    }

    // Calculate available layout width
    const availableWidthForContent = widthMode === Style.MeasureMode.Undefined ? Infinity : Math.max(0, widthConstraint)
    const epsilon = 0.001 // Float precision compensation

    // Wrap text into lines based on available width
    this.lines = this.wrapTextRich(ctx, this.segments, availableWidthForContent + epsilon, parsedWordSpacingPx, parsedLetterSpacingPx)

    // Initialize line metrics arrays
    this.lineHeights = [] // Final heights including leading
    this.lineAscents = [] // Text ascent heights
    this.lineContentHeights = [] // Raw content heights (ascent + descent)

    let totalTextHeight = 0
    const linesToMeasure = this.getLinesToMeasureOrRender()
    const numLines = linesToMeasure.length
    const defaultLineHeightMultiplier = 1.2 // Base leading multiplier

    // Calculate metrics for each line
    for (const line of linesToMeasure) {
      let maxAscent = 0
      let maxDescent = 0
      let maxFontSizeOnLine = 0

      // Handle empty line metrics
      if (line.length === 0) {
        ctx.font = this.getFontString()
        this._applyFontVariant(ctx, 'measureText (empty line)')
        const metrics = ctx.measureText(this.metricsString)
        maxAscent = metrics.actualBoundingBoxAscent ?? baseFontSize * 0.8
        maxDescent = metrics.actualBoundingBoxDescent ?? baseFontSize * 0.2
        maxFontSizeOnLine = baseFontSize
      } else {
        // Calculate max metrics across all segments in line
        for (const segment of line) {
          if (/^\s+$/.test(segment.text)) continue

          const segmentSize = segment.size || baseFontSize
          maxFontSizeOnLine = Math.max(maxFontSizeOnLine, segmentSize)

          ctx.font = this.getFontString(segment)
          this._applyFontVariant(ctx, 'measureText (segment height)')

          const metrics = ctx.measureText(this.metricsString)
          const ascent = metrics.actualBoundingBoxAscent ?? segmentSize * 0.8
          const descent = metrics.actualBoundingBoxDescent ?? segmentSize * 0.2

          maxAscent = Math.max(maxAscent, ascent)
          maxDescent = Math.max(maxDescent, descent)
        }
      }

      // Fallback metrics for lines with only whitespace
      if (maxAscent === 0 && maxDescent === 0 && line.length > 0) {
        ctx.font = this.getFontString()
        this._applyFontVariant(ctx, 'measureText (fallback)')
        const metrics = ctx.measureText(this.metricsString)
        maxAscent = metrics.actualBoundingBoxAscent ?? baseFontSize * 0.8
        maxDescent = metrics.actualBoundingBoxDescent ?? baseFontSize * 0.2
        maxFontSizeOnLine = maxFontSizeOnLine || baseFontSize
      }

      maxFontSizeOnLine = maxFontSizeOnLine || baseFontSize

      // Calculate total content height for line
      const actualContentHeight = maxAscent + maxDescent

      // Determine final line box height with leading
      const targetLineBoxHeight =
        typeof this.props.lineHeight === 'number' && this.props.lineHeight > 0 ? this.props.lineHeight : maxFontSizeOnLine * defaultLineHeightMultiplier

      // Use larger of target height or content height to prevent clipping
      const finalLineHeight = Math.max(actualContentHeight, targetLineBoxHeight)

      // Store line metrics for rendering
      this.lineHeights.push(finalLineHeight)
      this.lineAscents.push(maxAscent)
      this.lineContentHeights.push(actualContentHeight)

      totalTextHeight += finalLineHeight
    }

    // Add line gap spacing to total height
    const lineGapValue = this.props.lineGap
    const totalGapHeight = Math.max(0, (numLines - 1) * lineGapValue)
    const calculatedContentHeight = totalTextHeight + totalGapHeight

    // Calculate width required for text content
    const spaceWidth = this.measureSpaceWidth(ctx)
    let singleLineWidth = 0
    let firstWordInSingleLine = true
    for (const segment of this.segments) {
      const words = segment.text.split(/(\s+)/).filter(Boolean)
      for (const word of words) {
        if (/^\s+$/.test(word)) continue
        ctx.font = this.getFontString(segment)
        this._applyFontVariant(ctx, 'measureText (single line width)')
        const wordWidth = this.addLetterSpacingExtra(word, ctx.measureText(word).width, parsedLetterSpacingPx)
        if (!firstWordInSingleLine) {
          singleLineWidth += spaceWidth + parsedWordSpacingPx
        }
        singleLineWidth += wordWidth
        firstWordInSingleLine = false
      }
    }

    // Determine final content width based on wrapping
    let requiredContentWidth: number
    if (singleLineWidth <= availableWidthForContent) {
      requiredContentWidth = singleLineWidth
      if (linesToMeasure.length > 1 && this.props.maxLines !== 1 && !this.segments.some(s => s.text.includes('\n'))) {
        console.warn(
          `[TextNode ${this.key || ''}] Rich text should fit (${singleLineWidth.toFixed(2)} <= ${availableWidthForContent.toFixed(2)}) but wrapTextRich produced ${linesToMeasure.length} lines. Width calculation might be slightly off due to complex spacing/kerning.`,
        )
        let maxWrappedLineWidth = 0
        for (const line of linesToMeasure) {
          let currentLineWidth = 0
          let firstWordOnWrappedLine = true
          for (const segment of line) {
            const segmentWidth = segment.width ?? 0
            const isSpaceSegment = /^\s+$/.test(segment.text)
            if (!isSpaceSegment) {
              if (!firstWordOnWrappedLine) {
                currentLineWidth += spaceWidth + parsedWordSpacingPx
              }
              currentLineWidth += segmentWidth
              firstWordOnWrappedLine = false
            }
          }
          maxWrappedLineWidth = Math.max(maxWrappedLineWidth, currentLineWidth)
        }
        requiredContentWidth = Math.max(singleLineWidth, maxWrappedLineWidth)
      }
    } else {
      let maxWrappedLineWidth = 0
      for (const line of linesToMeasure) {
        let currentLineWidth = 0
        let firstWordOnWrappedLine = true
        for (const segment of line) {
          const segmentWidth = segment.width ?? 0
          const isSpaceSegment = /^\s+$/.test(segment.text)
          if (!isSpaceSegment) {
            if (!firstWordOnWrappedLine) {
              currentLineWidth += spaceWidth + parsedWordSpacingPx
            }
            currentLineWidth += segmentWidth
            firstWordOnWrappedLine = false
          }
        }
        maxWrappedLineWidth = Math.max(maxWrappedLineWidth, currentLineWidth)
      }
      requiredContentWidth = maxWrappedLineWidth
    }

    // Constrain width if needed
    let finalContentWidth = requiredContentWidth
    if (availableWidthForContent !== Infinity) {
      finalContentWidth = Math.min(requiredContentWidth, availableWidthForContent)
    }

    ctx.restore()
    return {
      width: Math.max(0, finalContentWidth),
      height: Math.max(0, calculatedContentHeight),
    }
  }

  /**
   * Wraps text segments into multiple lines while respecting width constraints and preserving styling.
   * Handles rich text attributes (color, weight, size, bold, italic) and proper word wrapping.
   * Also respects explicit newline characters (\n) for forced line breaks.
   * @param ctx Canvas rendering context used for text measurements
   * @param segments Array of text segments with styling information
   * @param maxWidth Maximum allowed width for each line in pixels
   * @param parsedWordSpacingPx Additional spacing to add between words in pixels
   * @returns Array of lines, where each line contains styled text segments
   */
  private wrapTextRich(
    ctx: CanvasRenderingContext2D,
    segments: TextSegment[],
    maxWidth: number,
    parsedWordSpacingPx: number,
    parsedLetterSpacingPx: number = 0,
  ): TextSegment[][] {
    const lines: TextSegment[][] = []

    if (segments.length === 0 || maxWidth <= 0) return lines

    let currentLineSegments: TextSegment[] = []
    let currentLineWidth = 0
    const spaceWidth = this.measureSpaceWidth(ctx)

    // Helper to finalize current line and start new one
    const finalizeLine = (forceEmpty = false) => {
      // Remove trailing whitespace segments unless we're forcing an empty line
      if (!forceEmpty) {
        while (currentLineSegments.length > 0 && /^\s+$/.test(currentLineSegments[currentLineSegments.length - 1].text)) {
          currentLineSegments.pop()
        }
      }
      // Always push the line, even if empty (for \n\n cases)
      lines.push(currentLineSegments)
      currentLineSegments = []
      currentLineWidth = 0
    }

    for (const segment of segments) {
      // Preserve all style attributes for consistency
      const segmentStyle = {
        color: segment.color,
        weight: segment.weight,
        size: segment.size,
        b: segment.b,
        i: segment.i,
      }

      // Check if segment contains newline characters
      if (segment.text.includes('\n')) {
        // Split by newlines and process each part
        const parts = segment.text.split('\n')

        for (let i = 0; i < parts.length; i++) {
          const part = parts[i]
          const isLastPart = i === parts.length - 1

          if (part.length > 0) {
            // Process this part normally
            const wordsAndSpaces = part.split(/(\s+)/).filter(Boolean)

            for (const wordOrSpace of wordsAndSpaces) {
              const isSpace = /^\s+$/.test(wordOrSpace)
              let wordSegment: TextSegment
              let wordWidth: number

              if (isSpace) {
                wordSegment = { text: wordOrSpace, ...segmentStyle, width: 0 }
                wordWidth = 0
              } else {
                ctx.font = this.getFontString(segmentStyle)
                if (this.props.fontVariant) ctx.fontVariant = this.props.fontVariant
                wordWidth = this.addLetterSpacingExtra(wordOrSpace, ctx.measureText(wordOrSpace).width, parsedLetterSpacingPx)
                wordSegment = { text: wordOrSpace, ...segmentStyle, width: wordWidth }
              }

              const needsSpace = currentLineSegments.length > 0 && !/^\s+$/.test(currentLineSegments[currentLineSegments.length - 1].text)
              const spaceToAdd = needsSpace ? spaceWidth + parsedWordSpacingPx : 0

              if (currentLineWidth + spaceToAdd + wordWidth <= maxWidth || currentLineSegments.length === 0) {
                if (needsSpace) {
                  currentLineSegments.push({ text: ' ', ...segmentStyle, width: 0 })
                  currentLineWidth += spaceToAdd
                }
                currentLineSegments.push(wordSegment)
                currentLineWidth += wordWidth
              } else {
                if (currentLineSegments.length > 0) {
                  finalizeLine()
                }

                if (!isSpace) {
                  if (wordWidth > maxWidth && maxWidth > 0) {
                    const brokenParts = this.breakWordRich(ctx, wordSegment, maxWidth, parsedLetterSpacingPx)

                    if (brokenParts.length > 0) {
                      for (let k = 0; k < brokenParts.length - 1; k++) {
                        lines.push([brokenParts[k]])
                      }
                      currentLineSegments = [brokenParts[brokenParts.length - 1]]
                      currentLineWidth = brokenParts[brokenParts.length - 1].width ?? 0
                    } else {
                      currentLineSegments = [wordSegment]
                      currentLineWidth = wordWidth
                    }
                  } else {
                    currentLineSegments = [wordSegment]
                    currentLineWidth = wordWidth
                  }
                }
              }
            }
          }

          // Force line break after each part except the last
          // If part is empty, this creates an empty line (like \n\n)
          if (!isLastPart) {
            finalizeLine(part.length === 0)
          }
        }
      } else {
        // No newlines - process normally
        const wordsAndSpaces = segment.text.split(/(\s+)/).filter(Boolean)

        for (const wordOrSpace of wordsAndSpaces) {
          const isSpace = /^\s+$/.test(wordOrSpace)
          let wordSegment: TextSegment
          let wordWidth: number

          if (isSpace) {
            wordSegment = { text: wordOrSpace, ...segmentStyle, width: 0 }
            wordWidth = 0
          } else {
            ctx.font = this.getFontString(segmentStyle)
            if (this.props.fontVariant) ctx.fontVariant = this.props.fontVariant
            wordWidth = this.addLetterSpacingExtra(wordOrSpace, ctx.measureText(wordOrSpace).width, parsedLetterSpacingPx)
            wordSegment = { text: wordOrSpace, ...segmentStyle, width: wordWidth }
          }

          const needsSpace = currentLineSegments.length > 0 && !/^\s+$/.test(currentLineSegments[currentLineSegments.length - 1].text)
          const spaceToAdd = needsSpace ? spaceWidth + parsedWordSpacingPx : 0

          if (currentLineWidth + spaceToAdd + wordWidth <= maxWidth || currentLineSegments.length === 0) {
            if (needsSpace) {
              currentLineSegments.push({ text: ' ', ...segmentStyle, width: 0 })
              currentLineWidth += spaceToAdd
            }
            currentLineSegments.push(wordSegment)
            currentLineWidth += wordWidth
          } else {
            if (currentLineSegments.length > 0) {
              finalizeLine()
            }

            if (!isSpace) {
              if (wordWidth > maxWidth && maxWidth > 0) {
                const brokenParts = this.breakWordRich(ctx, wordSegment, maxWidth, parsedLetterSpacingPx)

                if (brokenParts.length > 0) {
                  for (let k = 0; k < brokenParts.length - 1; k++) {
                    lines.push([brokenParts[k]])
                  }
                  currentLineSegments = [brokenParts[brokenParts.length - 1]]
                  currentLineWidth = brokenParts[brokenParts.length - 1].width ?? 0
                } else {
                  currentLineSegments = [wordSegment]
                  currentLineWidth = wordWidth
                }
              } else {
                currentLineSegments = [wordSegment]
                currentLineWidth = wordWidth
              }
            }
          }
        }
      }
    }

    finalizeLine()
    return lines
  }

  /**
   * Breaks a word segment into multiple segments that each fit within the specified width constraint.
   * Maintains all styling properties (color, weight, size, bold, italic) across broken segments.
   * @param ctx Canvas rendering context used for text measurements
   * @param segmentToBreak Original text segment to split
   * @param maxWidth Maximum width allowed for each resulting segment
   * @returns Array of TextSegments, each fitting maxWidth, or original segment if no breaking needed
   */
  private breakWordRich(ctx: CanvasRenderingContext2D, segmentToBreak: TextSegment, maxWidth: number, parsedLetterSpacingPx: number = 0): TextSegment[] {
    const word = segmentToBreak.text

    // Copy all style properties to maintain consistent styling across broken segments
    const style = {
      color: segmentToBreak.color,
      weight: segmentToBreak.weight,
      size: segmentToBreak.size,
      b: segmentToBreak.b,
      i: segmentToBreak.i,
    }

    if (maxWidth <= 0) return [segmentToBreak]

    const brokenSegments: TextSegment[] = []
    let currentPartText = ''

    // Configure context with segment style for accurate measurements
    ctx.font = this.getFontString(style)
    if (this.props.fontVariant) ctx.fontVariant = this.props.fontVariant

    // Process word character by character to find valid break points
    for (const char of word) {
      const testPartText = currentPartText + char
      const testPartWidth = this.addLetterSpacingExtra(testPartText, ctx.measureText(testPartText).width, parsedLetterSpacingPx)

      if (testPartWidth > maxWidth) {
        // Current accumulated text exceeds width - create new segment
        if (currentPartText) {
          brokenSegments.push({
            text: currentPartText,
            ...style,
            width: this.addLetterSpacingExtra(currentPartText, ctx.measureText(currentPartText).width, parsedLetterSpacingPx),
          })
        }

        // Handle current character that caused overflow
        currentPartText = char
        const currentCharWidth = ctx.measureText(currentPartText).width

        if (currentCharWidth > maxWidth) {
          // Single character is too wide - force break after it
          brokenSegments.push({
            text: currentPartText,
            ...style,
            width: currentCharWidth,
          })
          currentPartText = ''
        }
      } else {
        // Character fits - add to current part
        currentPartText = testPartText
      }
    }

    // Handle any remaining text as final segment
    if (currentPartText) {
      brokenSegments.push({
        text: currentPartText,
        ...style,
        width: this.addLetterSpacingExtra(currentPartText, ctx.measureText(currentPartText).width, parsedLetterSpacingPx),
      })
    }

    return brokenSegments.length > 0 ? brokenSegments : [segmentToBreak]
  }

  /**
   * Measures width of space character using base font
   */
  private measureSpaceWidth(ctx: CanvasRenderingContext2D): number {
    const originalFont = ctx.font
    ctx.font = this.getFontString()
    const width = ctx.measureText(' ').width
    ctx.font = originalFont
    return width > 0 ? width : (this.props.fontSize || 16) * 0.3
  }

  /**
   * Applies this.props.fontVariant to the context, or resets to 'normal'.
   * Centralizes the type guard + warn pattern repeated across measure/render paths.
   */
  private _applyFontVariant(ctx: CanvasRenderingContext2D, context: string): void {
    if (typeof this.props.fontVariant === 'string') {
      ctx.fontVariant = this.props.fontVariant
    } else if (this.props.fontVariant !== undefined) {
      console.warn(`[TextNode ${this.key || ''}] Invalid fontVariant prop type in ${context}:`, this.props.fontVariant)
      if (ctx.fontVariant !== 'normal') ctx.fontVariant = 'normal'
    } else {
      if (ctx.fontVariant !== 'normal') ctx.fontVariant = 'normal'
    }
  }

  /**
   *
   * Core features:
   * - Dynamic line heights with leading/spacing controls
   * - Vertical text alignment (top/middle/bottom)
   * - Horizontal text alignment (left/center/right/justify)
   * - Text wrapping within bounds
   * - Ellipsis truncation
   * - Rich text styling per segment (color, weight, size, etc)
   * - Performance optimizations (clipping, visibility checks)
   * @param ctx Canvas rendering context
   * @param x Content box left position in pixels
   * @param y Content box top position in pixels
   * @param width Content box total width including padding
   * @param height Content box total height including padding
   */
  protected override async _renderContent(ctx: CanvasRenderingContext2D, x: number, y: number, width: number, height: number) {
    await super._renderContent(ctx, x, y, width, height)

    ctx.save()
    ctx.textBaseline = 'alphabetic'
    ctx.letterSpacing = this.formatSpacing(this.props.letterSpacing)
    ctx.wordSpacing = 'normal'

    const baseFontSize = this.props.fontSize || 16
    const parsedWordSpacingPx = this.parseSpacingToPx(this.props.wordSpacing, baseFontSize)

    // Calculate content box with padding
    const paddingLeft = this.node.getComputedPadding(Style.Edge.Left) ?? 0
    const paddingTop = this.node.getComputedPadding(Style.Edge.Top) ?? 0
    const paddingRight = this.node.getComputedPadding(Style.Edge.Right) ?? 0
    const paddingBottom = this.node.getComputedPadding(Style.Edge.Bottom) ?? 0
    const contentX = x + paddingLeft
    const contentY = y + paddingTop
    const contentWidth = Math.max(0, width - paddingLeft - paddingRight)
    const contentHeight = Math.max(0, height - paddingTop - paddingBottom)

    if (contentWidth <= 0 || contentHeight <= 0) {
      ctx.restore()
      return
    }

    // Re-calculate lines based on the actual render width to ensure consistency
    // This fixes issues where Yoga Layout might use a cached measurement from a different
    // width constraint (e.g., during a flex shrink pass) but final layout is wider.
    const spaceWidth = this.measureSpaceWidth(ctx)
    // Use a small epsilon for float precision issues
    const epsilon = 0.01
    const parsedLetterSpacingPx = this.parseSpacingToPx(this.props.letterSpacing, baseFontSize)
    const allLines = this.wrapTextRich(ctx, this.segments, contentWidth + epsilon, parsedWordSpacingPx, parsedLetterSpacingPx)

    const needsEllipsis = this.props.ellipsis && this.props.maxLines !== undefined && allLines.length > this.props.maxLines

    // Apply maxLines constraint to get the visible lines
    const visibleLines = this.props.maxLines !== undefined && this.props.maxLines > 0 ? allLines.slice(0, this.props.maxLines) : allLines

    const numLinesToRender = visibleLines.length

    // Recalculate line metrics for the rendered lines
    // We cannot rely on this.lineHeights from measureText because it might correspond to different wrapping
    const lineHeights: number[] = []
    const lineAscents: number[] = []
    const lineContentHeights: number[] = []
    const defaultLineHeightMultiplier = 1.2
    let totalTextHeight = 0

    for (const line of visibleLines) {
      let maxAscent = 0
      let maxDescent = 0
      let maxFontSizeOnLine = 0

      if (line.length === 0) {
        ctx.font = this.getFontString()
        if (this.props.fontVariant) ctx.fontVariant = typeof this.props.fontVariant === 'string' ? this.props.fontVariant : 'normal'
        const metrics = ctx.measureText(this.metricsString)
        maxAscent = metrics.actualBoundingBoxAscent ?? baseFontSize * 0.8
        maxDescent = metrics.actualBoundingBoxDescent ?? baseFontSize * 0.2
        maxFontSizeOnLine = baseFontSize
      } else {
        for (const segment of line) {
          if (/^\s+$/.test(segment.text)) continue
          const segmentSize = segment.size || baseFontSize
          maxFontSizeOnLine = Math.max(maxFontSizeOnLine, segmentSize)

          // Style context for accurate metrics
          ctx.font = this.getFontString(segment)
          if (this.props.fontVariant) ctx.fontVariant = typeof this.props.fontVariant === 'string' ? this.props.fontVariant : 'normal'

          const metrics = ctx.measureText(this.metricsString)
          const ascent = metrics.actualBoundingBoxAscent ?? segmentSize * 0.8
          const descent = metrics.actualBoundingBoxDescent ?? segmentSize * 0.2
          maxAscent = Math.max(maxAscent, ascent)
          maxDescent = Math.max(maxDescent, descent)
        }
      }
      if (maxAscent === 0 && maxDescent === 0 && line.length > 0) {
        // Fallback
        ctx.font = this.getFontString()
        if (this.props.fontVariant) ctx.fontVariant = typeof this.props.fontVariant === 'string' ? this.props.fontVariant : 'normal'
        const metrics = ctx.measureText(this.metricsString)
        maxAscent = metrics.actualBoundingBoxAscent ?? baseFontSize * 0.8
        maxDescent = metrics.actualBoundingBoxDescent ?? baseFontSize * 0.2
        maxFontSizeOnLine = maxFontSizeOnLine || baseFontSize
      }
      maxFontSizeOnLine = maxFontSizeOnLine || baseFontSize
      const actualContentHeight = maxAscent + maxDescent
      const targetLineBoxHeight =
        typeof this.props.lineHeight === 'number' && this.props.lineHeight > 0 ? this.props.lineHeight : maxFontSizeOnLine * defaultLineHeightMultiplier
      const finalLineHeight = Math.max(actualContentHeight, targetLineBoxHeight)

      lineHeights.push(finalLineHeight)
      lineAscents.push(maxAscent)
      lineContentHeights.push(actualContentHeight)
      totalTextHeight += finalLineHeight
    }

    if (numLinesToRender === 0) {
      ctx.restore()
      return
    }

    // Calculate vertical alignment offset
    const lineGapValue = this.props.lineGap
    const totalCalculatedTextHeight = totalTextHeight + Math.max(0, numLinesToRender - 1) * lineGapValue

    let blockStartY: number
    switch (this.props.verticalAlign) {
      case 'middle':
        blockStartY = contentY + (contentHeight - totalCalculatedTextHeight) / 2
        break
      case 'bottom':
        blockStartY = contentY + contentHeight - totalCalculatedTextHeight
        break
      case 'top':
      default:
        blockStartY = contentY
    }

    let currentLineTopY = blockStartY

    // Setup text content clipping region
    ctx.beginPath()
    ctx.rect(contentX, contentY, contentWidth, contentHeight)
    ctx.clip()

    // Configure ellipsis if needed
    const ellipsisChar = typeof this.props.ellipsis === 'string' ? this.props.ellipsis : '...'
    let ellipsisWidth = 0
    let ellipsisStyle: Partial<TextSegment> | undefined = undefined

    if (needsEllipsis) {
      const lastRenderedLine = visibleLines[visibleLines.length - 1]
      // ... ellipsis calculation ...
      const lastTextStyleSegment = [...lastRenderedLine].reverse().find(seg => !/^\s+$/.test(seg.text))
      ellipsisStyle = lastTextStyleSegment
        ? {
            color: lastTextStyleSegment.color,
            weight: lastTextStyleSegment.weight,
            size: lastTextStyleSegment.size,
            b: lastTextStyleSegment.b,
            i: lastTextStyleSegment.i,
          }
        : undefined

      ctx.save()
      ctx.font = this.getFontString(ellipsisStyle)
      if (this.props.fontVariant) {
        ctx.fontVariant = typeof this.props.fontVariant === 'string' ? this.props.fontVariant : 'normal'
      }
      ellipsisWidth = ctx.measureText(ellipsisChar).width
      ctx.restore()
    }

    // Render text content line by line
    for (let i = 0; i < numLinesToRender; i++) {
      const lineSegments = visibleLines[i]
      const currentLineFinalHeight = lineHeights[i]
      const currentLineMaxAscent = lineAscents[i]
      const currentLineContentHeight = lineContentHeights[i]

      // Calculate line spacing metrics
      const currentLineLeading = currentLineFinalHeight - currentLineContentHeight
      const currentLineSpaceAbove = Math.max(0, currentLineLeading / 2)
      const lineY = currentLineTopY + currentLineSpaceAbove + currentLineMaxAscent

      // Visibility culling check
      const lineTop = currentLineTopY
      const lineBottom = currentLineTopY + currentLineFinalHeight

      // Don't skip empty lines - they're intentional (from \n\n)
      // Only skip if the line is completely outside the visible area
      if (lineBottom <= contentY || lineTop >= contentY + contentHeight) {
        currentLineTopY += currentLineFinalHeight + lineGapValue
        continue
      }

      const isLastRenderedLine = i === numLinesToRender - 1

      // Calculate line width metrics for alignment
      let totalLineWidth = 0
      let totalWordsWidth = 0
      let numWordGaps = 0
      let firstWordOnLine = true
      const noSpaceBeforePunctuation = /^[.,!?;:)\]}]/

      for (const segment of lineSegments) {
        const segmentWidth = segment.width ?? 0
        const isSpaceSegment = /^\s+$/.test(segment.text)

        if (!isSpaceSegment) {
          if (!firstWordOnLine) {
            totalLineWidth += spaceWidth + parsedWordSpacingPx
            if (!noSpaceBeforePunctuation.test(segment.text)) {
              numWordGaps++
            }
          }
          totalLineWidth += segmentWidth
          totalWordsWidth += segmentWidth
          firstWordOnLine = false
        }
      }

      // Calculate horizontal alignment position
      const isJustify = this.props.textAlign === 'justify' && !isLastRenderedLine
      const lineTextAlign = isJustify ? 'left' : this.props.textAlign || 'left'
      let currentX: number

      switch (lineTextAlign) {
        case 'center':
          currentX = contentX + (contentWidth - totalLineWidth) / 2
          break
        case 'right':
        case 'end':
          currentX = contentX + contentWidth - totalLineWidth
          break
        case 'left':
        case 'start':
        default:
          currentX = contentX
      }
      currentX = Math.max(contentX, currentX)

      // Calculate justification spacing
      let spacePerWordGapPlusSpacing = spaceWidth + parsedWordSpacingPx
      if (isJustify && numWordGaps > 0 && totalLineWidth < contentWidth) {
        const totalBaseSpacingWidth = numWordGaps * (spaceWidth + parsedWordSpacingPx)
        const remainingSpace = contentWidth - totalWordsWidth - totalBaseSpacingWidth
        if (remainingSpace > 0) {
          spacePerWordGapPlusSpacing += remainingSpace / numWordGaps
        }
      }

      // Render line segments (skip rendering for truly empty lines)
      if (lineSegments.length > 0 && !lineSegments.every(s => s.text.trim() === '')) {
        let accumulatedWidth = 0
        let ellipsisApplied = false
        let firstWordDrawn = false

        for (let j = 0; j < lineSegments.length; j++) {
          const segment = lineSegments[j]
          const segmentWidth = segment.width ?? 0
          const isLastSegmentOnLine = j === lineSegments.length - 1
          const isSpaceSegment = /^\s+$/.test(segment.text)

          // Calculate word spacing
          let spaceToAddBefore = 0
          if (!isSpaceSegment && firstWordDrawn && !noSpaceBeforePunctuation.test(segment.text)) {
            spaceToAddBefore = isJustify ? spacePerWordGapPlusSpacing : spaceWidth + parsedWordSpacingPx
          }

          // Apply segment styles
          ctx.font = this.getFontString(segment)
          ctx.fillStyle = segment.color || this.props.color || 'black'

          this._applyFontVariant(ctx, '_renderContent (segment render)')

          // Handle text truncation and ellipsis
          let textToDraw = segment.text
          let currentSegmentRenderWidth = segmentWidth
          let applyEllipsisAfter = false

          if (isLastRenderedLine && needsEllipsis && !isSpaceSegment) {
            const currentTotalWidth = accumulatedWidth + spaceToAddBefore + segmentWidth
            const spaceNeededAfter = isLastSegmentOnLine ? 0 : isJustify ? spacePerWordGapPlusSpacing : spaceWidth + parsedWordSpacingPx

            if (currentTotalWidth > contentWidth - spaceNeededAfter) {
              const availableWidthForSegment = contentWidth - accumulatedWidth - spaceToAddBefore - ellipsisWidth
              if (availableWidthForSegment > 0) {
                let truncatedText = ''
                for (const char of segment.text) {
                  if (ctx.measureText(truncatedText + char).width <= availableWidthForSegment) {
                    truncatedText += char
                  } else {
                    break
                  }
                }
                textToDraw = truncatedText
                currentSegmentRenderWidth = ctx.measureText(textToDraw).width
              } else {
                textToDraw = ''
                currentSegmentRenderWidth = 0
              }
              applyEllipsisAfter = true
              ellipsisApplied = true
            } else if (isLastSegmentOnLine) {
              applyEllipsisAfter = true
              ellipsisApplied = true
            }
          }

          // Render text segment
          currentX += spaceToAddBefore
          accumulatedWidth += spaceToAddBefore

          const remainingRenderWidth = contentX + contentWidth - currentX
          if (currentSegmentRenderWidth > 0 && remainingRenderWidth > 0 && !isSpaceSegment) {
            ctx.textAlign = 'left'

            const shadows = this.props.textShadow ? (Array.isArray(this.props.textShadow) ? this.props.textShadow : [this.props.textShadow]) : []

            ctx.save()

            // Draw shadows
            for (const shadow of shadows) {
              ctx.shadowColor = shadow.color || 'transparent'
              ctx.shadowBlur = shadow.blur || 0
              ctx.shadowOffsetX = shadow.offsetX || 0
              ctx.shadowOffsetY = shadow.offsetY || 0
              ctx.fillText(textToDraw, currentX, lineY, Math.max(0, remainingRenderWidth + 1))
            }

            // Reset shadow to draw the main text
            ctx.shadowColor = 'transparent'
            ctx.shadowBlur = 0
            ctx.shadowOffsetX = 0
            ctx.shadowOffsetY = 0

            ctx.fillText(textToDraw, currentX, lineY, Math.max(0, remainingRenderWidth + 1))

            ctx.restore()

            firstWordDrawn = true
          }

          currentX += currentSegmentRenderWidth
          accumulatedWidth += currentSegmentRenderWidth

          // Render ellipsis
          if (applyEllipsisAfter) {
            const ellipsisRemainingWidth = contentX + contentWidth - currentX
            if (ellipsisRemainingWidth >= ellipsisWidth) {
              ctx.save()
              ctx.font = this.getFontString(ellipsisStyle)

              this._applyFontVariant(ctx, '_renderContent (ellipsis draw)')

              ctx.fillStyle = ellipsisStyle?.color || this.props.color || 'black'
              ctx.fillText(ellipsisChar, currentX, lineY, Math.max(0, ellipsisRemainingWidth + 1))
              ctx.restore()
            }
            break
          }

          if (ellipsisApplied && currentX >= contentX + contentWidth) break
        }
      }

      currentLineTopY += currentLineFinalHeight + lineGapValue
    }

    ctx.restore()
  }
}

/**
 * Creates a new TextNode instance with rich text support
 */
export const Text = (text: number | string, props?: TextProps): CanvasElement => ({
  __type: 'Text',
  text,
  props,
})
