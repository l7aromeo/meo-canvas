import { jest } from '@jest/globals'

export const existsSync = jest.fn<(val: any) => boolean>(_val => true)

export const promises = {
  mkdir: jest.fn<() => Promise<void>>(() => Promise.resolve()),
  readFile: jest.fn<() => Promise<Buffer>>(() => Promise.reject(new Error('not found'))),
  writeFile: jest.fn<() => Promise<void>>(() => Promise.resolve()),
  unlink: jest.fn<() => Promise<void>>(() => Promise.resolve()),
}

export const __mocks__ = {
  existsSync,
  promises,
  reset: () => {
    existsSync.mockClear()
    existsSync.mockReturnValue(true)
    promises.mkdir.mockClear()
    promises.readFile.mockClear()
    promises.readFile.mockRejectedValue(new Error('not found'))
    promises.writeFile.mockClear()
    promises.unlink.mockClear()
  },
}
