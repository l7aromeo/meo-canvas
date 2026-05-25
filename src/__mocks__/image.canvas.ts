import { vi, type MockInstance } from 'vitest'
import { BoxNode } from '@/canvas/layout.canvas.js'
import { Style } from '@/constant/common.const.js' // Import Style

export class ImageNode extends BoxNode {
  load: MockInstance<() => Promise<void>>
  getLoadingPromise: MockInstance<() => Promise<void>>

  constructor(props: any) {
    // Apply ImageNode's specific defaults here in the mock
    const defaultImageProps = {
      objectFit: 'fill',
      overflow: Style.Overflow.Hidden,
      saturate: 1,
      objectPosition: { Left: '50%', Top: '50%' },
    }
    const mergedProps = { ...defaultImageProps, ...props }

    super({ name: 'Image', ...mergedProps }) // Pass the merged props to super
    this.load = vi.fn(() => Promise.resolve())
    this.getLoadingPromise = vi.fn(() => Promise.resolve())
  }
}

export const Image = vi.fn((props: any) => new ImageNode(props))

export type RenderImageCache = Map<string, Promise<any>>

export const __mocks__ = {
  ImageNode,
  Image,
  RenderImageCache: Map,
  reset: () => {
    Image.mockClear()
  },
}
