import { vi } from 'vitest'

export const existsSync = vi.fn<(val: any) => boolean>(_val => true)

export const promises = {
  mkdir: vi.fn<() => Promise<void>>(() => Promise.resolve()),
  readFile: vi.fn<() => Promise<Buffer>>(() => Promise.reject(new Error('not found'))),
  writeFile: vi.fn<() => Promise<void>>(() => Promise.resolve()),
  unlink: vi.fn<() => Promise<void>>(() => Promise.resolve()),
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
