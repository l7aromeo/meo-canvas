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
