import { createHash } from 'crypto'
import { promises as fs } from 'fs'
import { join } from 'path'

let _cacheDir = join(process.cwd(), '.cache', 'files')
let _dirEnsured = false

/**
 * Override the default disk cache directory.
 * Must be called before any cache read/write operations.
 */
export function setDiskCacheDir(dir: string): void {
  _cacheDir = dir
  _dirEnsured = false
}

async function ensureDir(): Promise<void> {
  if (_dirEnsured) return
  await fs.mkdir(_cacheDir, { recursive: true })
  _dirEnsured = true
}

export function hashBuffer(buf: Buffer): string {
  return createHash('sha256').update(buf).digest('hex')
}

export async function readDiskCache(key: string): Promise<Buffer | null> {
  try {
    await ensureDir()
    return await fs.readFile(join(_cacheDir, key))
  } catch {
    return null
  }
}

export async function writeDiskCache(key: string, data: Buffer): Promise<void> {
  try {
    await ensureDir()
    await fs.writeFile(join(_cacheDir, key), data)
  } catch {
    // best-effort — cache write failures are non-fatal
  }
}

export async function deleteDiskCache(key: string): Promise<void> {
  try {
    await fs.unlink(join(_cacheDir, key))
  } catch (err) {
    // non-fatal — file may not exist if write failed earlier
    if ((err as NodeJS.ErrnoException).code !== 'ENOENT') {
      console.warn(`[disk.cache] Failed to delete cache entry "${key}":`, (err as Error).message)
    }
  }
}

/**
 * Delete the entire disk cache directory.
 * Called on process exit to clean up any orphaned cache files.
 */
export async function clearDiskCache(): Promise<void> {
  _dirEnsured = false
  try {
    await fs.rm(_cacheDir, { recursive: true, force: true })
  } catch (err) {
    // non-fatal — directory may not exist
    if ((err as NodeJS.ErrnoException).code !== 'ENOENT') {
      console.warn('[disk.cache] Failed to clear cache directory:', (err as Error).message)
    }
  }
}

// Clean up disk cache on process exit to handle crashes mid-render.
// Guard prevents re-entry: clearDiskCache() is async, so without this the
// beforeExit → schedule I/O → idle → beforeExit cycle loops forever.
let _exitCleanupStarted = false
process.on('beforeExit', () => {
  if (_exitCleanupStarted) return
  _exitCleanupStarted = true
  void clearDiskCache()
})

// Also clean up on SIGINT/SIGTERM for graceful shutdowns
const cleanupOnExit = () => {
  clearDiskCache().finally(() => process.exit(0))
}
process.on('SIGINT', cleanupOnExit)
process.on('SIGTERM', cleanupOnExit)
