import { jest } from '@jest/globals'
import { BoxNode } from '@/canvas/layout.canvas.util.js'
import Mock = jest.Mock
import { Style } from '@/constant/common.const.js' // Import Style

export class ImageNode extends BoxNode {
  getLoadingPromise: Mock<() => Promise<void>>

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
    this.getLoadingPromise = jest.fn(() => Promise.resolve())
  }
}

export const Image = jest.fn((props: any) => new ImageNode(props))

export const __mocks__ = {
  ImageNode,
  Image,
  reset: () => {
    Image.mockClear()
  },
}
