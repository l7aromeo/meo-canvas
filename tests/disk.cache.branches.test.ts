import { vi } from 'vitest'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { promises as fs } from 'node:fs'
import { setDiskCacheDir, writeDiskCache, readDiskCache, deleteDiskCache, clearDiskCache } from '@/util/disk.cache.js'

/** A distinct directory per test file so a parallel run cannot collide on it. */
const DIR = join(tmpdir(), `meo-canvas-disk-cache-branches-${process.pid}`)

beforeEach(() => setDiskCacheDir(DIR))
afterEach(async () => {
  vi.restoreAllMocks()
  await fs.rm(DIR, { recursive: true, force: true }).catch(() => {})
})

const errorWithCode = (code: string) => Object.assign(new Error(code), { code })

describe('deleteDiskCache', () => {
  it('removes an entry that is there', async () => {
    await writeDiskCache('present', Buffer.from('bytes'))
    await deleteDiskCache('present')
    expect(await readDiskCache('present')).toBeNull()
  })

  it('says nothing when the entry was never written', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {})
    await deleteDiskCache('absent')
    expect(warn).not.toHaveBeenCalled()
  })

  it('warns when the delete fails for a reason other than absence', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {})
    vi.spyOn(fs, 'unlink').mockRejectedValueOnce(errorWithCode('EACCES'))
    await deleteDiskCache('guarded')
    expect(warn).toHaveBeenCalledWith(expect.stringContaining('Failed to delete cache entry'), expect.anything())
    warn.mockRestore()
  })
})

describe('clearDiskCache', () => {
  it('removes the whole directory', async () => {
    await writeDiskCache('one', Buffer.from('a'))
    await clearDiskCache()
    expect(await readDiskCache('one')).toBeNull()
  })

  it('says nothing when the directory was never made', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {})
    await clearDiskCache()
    expect(warn).not.toHaveBeenCalled()
  })

  it('warns when the clear fails for a reason other than absence', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {})
    vi.spyOn(fs, 'rm').mockRejectedValueOnce(errorWithCode('EPERM'))
    await clearDiskCache()
    expect(warn).toHaveBeenCalledWith(expect.stringContaining('Failed to clear cache directory'), expect.anything())
    warn.mockRestore()
  })
})

describe('writeDiskCache', () => {
  it('drops the scratch file when the rename into place fails', async () => {
    vi.spyOn(fs, 'rename').mockRejectedValueOnce(errorWithCode('EXDEV'))
    await expect(writeDiskCache('doomed', Buffer.from('bytes'))).resolves.toBeUndefined()
    expect(await readDiskCache('doomed')).toBeNull()
  })

  it('round-trips bytes through a write and a read', async () => {
    await writeDiskCache('round', Buffer.from('trip'))
    expect((await readDiskCache('round'))?.toString()).toBe('trip')
  })

  it('makes the directory only once across repeated writes', async () => {
    await writeDiskCache('a', Buffer.from('1'))
    const mkdir = vi.spyOn(fs, 'mkdir')
    await writeDiskCache('b', Buffer.from('2'))
    expect(mkdir).not.toHaveBeenCalled()
  })
})
