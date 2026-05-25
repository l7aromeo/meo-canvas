import { vi } from 'vitest'

export function createMockCanvasContext() {
  return {
    scale: vi.fn(),
    save: vi.fn(),
    restore: vi.fn(),
    translate: vi.fn(),
    rotate: vi.fn(),
    setTransform: vi.fn(),
    beginPath: vi.fn(),
    moveTo: vi.fn(),
    lineTo: vi.fn(),
    arc: vi.fn(),
    closePath: vi.fn(),
    rect: vi.fn(),
    fill: vi.fn(),
    stroke: vi.fn(),
    clip: vi.fn(),
    drawImage: vi.fn(),
    clearRect: vi.fn(),
    fillText: vi.fn(),
    strokeText: vi.fn(),
    measureText: vi.fn(() => ({ width: 0, actualBoundingBoxAscent: 10, actualBoundingBoxDescent: 2 })),
    fillStyle: '',
    strokeStyle: '',
    lineWidth: 0,
    lineCap: 'butt' as const,
    lineJoin: 'miter' as const,
    globalAlpha: 1,
    globalCompositeOperation: '',
    shadowOffsetX: 0,
    shadowOffsetY: 0,
    shadowBlur: 0,
    shadowColor: '',
    imageSmoothingEnabled: true,
    imageSmoothingQuality: 'high' as const,
    font: '',
    textAlign: 'left' as const,
    textBaseline: 'alphabetic' as const,
    letterSpacing: 'normal',
    wordSpacing: 'normal',
    fontVariant: 'normal',
    createLinearGradient: vi.fn(() => ({ addColorStop: vi.fn() })),
    createRadialGradient: vi.fn(() => ({ addColorStop: vi.fn() })),
    setLineDash: vi.fn(),
  }
}

export function createMockCanvas() {
  return vi.fn(function (this: any, w: number, h: number) {
    this.width = w
    this.height = h
    this.getContext = vi.fn(() => createMockCanvasContext())
    this.toBufferSync = vi.fn(() => Buffer.from(''))
  })
}
