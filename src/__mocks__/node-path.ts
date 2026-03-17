import { jest } from '@jest/globals'
import nodePath from 'node:path'

export const __mocks__ = {
  resolve: jest.fn(p => p), // Default to returning the path itself
  // Pass-throughs for utilities used internally (e.g. worker file path resolution)
  join: jest.fn((...args: string[]) => nodePath.join(...args)),
  dirname: jest.fn((p: string) => nodePath.dirname(p)),
  reset: jest.fn(() => {
    __mocks__.resolve.mockClear()
    __mocks__.resolve.mockImplementation(p => p) // Reset to default behavior
    __mocks__.join.mockClear()
    __mocks__.join.mockImplementation((...args: string[]) => nodePath.join(...args))
    __mocks__.dirname.mockClear()
    __mocks__.dirname.mockImplementation((p: string) => nodePath.dirname(p))
  }),
}
