import { Style } from '@/constant/common.const.js'

describe('common.const', () => {
  it('should export BORDER_STYLE_SOLID with value 0', () => {
    expect(Style.Border.Solid).toBe(0)
  })

  it('should export Border.Dashed with value 1', () => {
    expect(Style.Border.Dashed).toBe(1)
  })

  it('should export Border.Dotted with value 1', () => {
    expect(Style.Border.Dotted).toBe(2)
  })
})
