import { createHash } from 'crypto'
import { promises as fs } from 'fs'
import { join } from 'path'

const CACHE_DIR = join(process.cwd(), '.cache', 'files')
let _dirEnsured = false

async function ensureDir(): Promise<void> {
  if (_dirEnsured) return
  await fs.mkdir(CACHE_DIR, { recursive: true })
  _dirEnsured = true
}

export function hashBuffer(buf: Buffer): string {
  return createHash('sha256').update(buf).digest('hex')
}

export async function readDiskCache(key: string): Promise<Buffer | null> {
  try {
    await ensureDir()
    return await fs.readFile(join(CACHE_DIR, key))
  } catch {
    return null
  }
}

export async function writeDiskCache(key: string, data: Buffer): Promise<void> {
  try {
    await ensureDir()
    await fs.writeFile(join(CACHE_DIR, key), data)
  } catch {
    // best-effort — cache write failures are non-fatal
  }
}

export async function deleteDiskCache(key: string): Promise<void> {
  try {
    await fs.unlink(join(CACHE_DIR, key))
  } catch {
    // non-fatal — file may not exist if write failed earlier
  }
}

/**
 * Delete the entire disk cache directory.
 * Called on process exit to clean up any orphaned cache files.
 */
export async function clearDiskCache(): Promise<void> {
  try {
    await fs.rm(CACHE_DIR, { recursive: true, force: true })
  } catch {
    // non-fatal — directory may not exist
  }
}

// Clean up disk cache on process exit to handle crashes mid-render
process.on('beforeExit', () => {
  // Fire and forget — best effort cleanup
  clearDiskCache()
})

// Also clean up on SIGINT/SIGTERM for graceful shutdowns
const cleanupOnExit = () => {
  clearDiskCache().finally(() => process.exit(0))
}
process.on('SIGINT', cleanupOnExit)
process.on('SIGTERM', cleanupOnExit)
