import { vi } from 'vitest'

export const __mocks__ = {
  resolve: vi.fn((p: string) => p),
  join: vi.fn((...args: string[]) => args.join('/')),
  dirname: vi.fn((p: string) => {
    const parts = p.split('/')
    parts.pop()
    return parts.join('/') || '.'
  }),
  reset: vi.fn(() => {
    __mocks__.resolve.mockClear()
    __mocks__.resolve.mockImplementation((p: string) => p)
    __mocks__.join.mockClear()
    __mocks__.join.mockImplementation((...args: string[]) => args.join('/'))
    __mocks__.dirname.mockClear()
    __mocks__.dirname.mockImplementation((p: string) => {
      const parts = p.split('/')
      parts.pop()
      return parts.join('/') || '.'
    })
  }),
}
