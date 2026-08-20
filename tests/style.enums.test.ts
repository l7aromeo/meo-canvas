import { Style } from '@/constant/common.const.js'
import type { BoxProps, TextProps, ImageProps } from '@/canvas/canvas.type.js'

describe('Style enums', () => {
  it('carries the CSS keyword as its value, so it needs no lookup table', () => {
    // The point of a string enum here: `Style.BlendMode.Multiply` can go straight into a canvas
    // context's `globalCompositeOperation`, and a caller's plain `'multiply'` still typechecks.
    expect(Style.BlendMode.Multiply).toBe('multiply')
    expect(Style.BlendMode.ColorDodge).toBe('color-dodge')
    expect(Style.PaintOrder.Stroke).toBe('stroke')
    expect(Style.BackgroundRepeat.NoRepeat).toBe('no-repeat')
    expect(Style.BackgroundSize.Cover).toBe('cover')
    expect(Style.GradientType.Conic).toBe('conic')
    expect(Style.ObjectFit.ScaleDown).toBe('scale-down')
    expect(Style.TextAlign.Justify).toBe('justify')
    expect(Style.VerticalAlign.Middle).toBe('middle')
    expect(Style.TextDecoration.LineThrough).toBe('line-through')
  })

  it('covers every CSS blend mode', () => {
    expect(Object.values(Style.BlendMode)).toEqual([
      'normal',
      'multiply',
      'screen',
      'overlay',
      'darken',
      'lighten',
      'color-dodge',
      'color-burn',
      'hard-light',
      'soft-light',
      'difference',
      'exclusion',
      'hue',
      'saturation',
      'color',
      'luminosity',
    ])
  })

  it('keeps Yoga’s own constants and the older Border enum reachable', () => {
    expect(Style.Align.Center).toBeDefined()
    expect(Style.PositionType.Absolute).toBeDefined()
    expect(Style.Border.Dashed).toBeDefined()
  })

  it('accepts an enum or a plain string wherever a value was a string before', () => {
    // Widening these props must not cost an existing caller their string literal.
    const viaEnum: TextProps = { textAlign: Style.TextAlign.Center, verticalAlign: Style.VerticalAlign.Middle }
    const viaString: TextProps = { textAlign: 'center', verticalAlign: 'middle' }
    const image: ImageProps = { src: 'x.png', objectFit: Style.ObjectFit.Cover }
    const imageString: ImageProps = { src: 'x.png', objectFit: 'cover' }
    const box: BoxProps = { borderStyle: Style.Border.Dotted }

    expect([viaEnum.textAlign, viaString.textAlign]).toEqual(['center', 'center'])
    expect([viaEnum.verticalAlign, viaString.verticalAlign]).toEqual(['middle', 'middle'])
    expect([image.objectFit, imageString.objectFit]).toEqual(['cover', 'cover'])
    expect(box.borderStyle).toBe(Style.Border.Dotted)
  })
})
