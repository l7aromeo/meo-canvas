// The addon stays loaded after the only worker that loaded it ends. Node closes
// an addon with the environment that loaded it, and on Windows that unmapped
// the DLL while its thread pools were parked in its code, so a thread that woke
// later executed freed memory (`l7aromeo/meo-canvas#254`). A child process, so
// the main thread loading it is never vitest's.

import { execFileSync } from 'node:child_process'
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { afterAll, describe, expect, it } from 'vitest'

import { encodeScene } from './arena.js'
import { Box } from './index.js'

const HERE = dirname(fileURLToPath(import.meta.url))
const ADDON = resolve(HERE, '../meo-canvas.node')

/** Long enough for a pool thread parked in the addon to wake after its worker ends. */
const AFTER_EXIT_MS = 2000

/**
 * Loads the addon in a worker only, encodes a GIF there, which starts rayon's
 * pool and the GIF palette's, lets the worker end, waits, and prints whether
 * the addon is still among the process's loaded modules.
 */
const CHILD = `
import { basename } from 'node:path'
import { Worker } from 'node:worker_threads'
const [addon, scene] = process.argv.slice(2)
const worker = new Worker(\`
  const { parentPort, workerData } = require('node:worker_threads')
  const native = require(workerData.addon)
  const scene = JSON.parse(require('node:fs').readFileSync(workerData.scene, 'utf8'))
  const canvas = native.paint(Float64Array.from(scene.slots), scene.values, { fonts: [] })
  canvas.encodeAsync('gif', {}).then(bytes => parentPort.postMessage(bytes.length))
\`, { eval: true, workerData: { addon, scene } })
// Listened for at once: a worker can end before its message is handled.
const exited = new Promise(done => worker.once('exit', done))
const size = await new Promise((done, fail) => { worker.once('message', done); worker.once('error', fail) })
await exited
await new Promise(done => setTimeout(done, ${AFTER_EXIT_MS}))
const loaded = process.report.getReport().sharedObjects.some(path => basename(path).toLowerCase() === 'meo-canvas.node')
process.stdout.write(JSON.stringify({ size, loaded }))
`

const scratch = mkdtempSync(join(tmpdir(), 'meo-canvas-unload-'))
afterAll(() => rmSync(scratch, { recursive: true, force: true }))

describe('a worker-only addon', () => {
  it('stays loaded, and the process exits cleanly, after its only worker ends', () => {
    const arena = encodeScene([Box({ width: 8, height: 8, backgroundColor: '#ff0000' })], 8, 8, false, 1)
    // The scene crosses as JSON, so it must carry no bytes.
    expect(arena.values.every(value => typeof value === 'string')).toBe(true)
    const scene = join(scratch, 'scene.json')
    writeFileSync(scene, JSON.stringify({ slots: [...arena.slots], values: arena.values }))
    const child = join(scratch, 'child.mjs')
    writeFileSync(child, CHILD)

    let output: string
    try {
      output = execFileSync(process.execPath, [child, ADDON, scene], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] })
    } catch (cause) {
      // A crash leaves no stderr, so its status is the whole report.
      const { status, signal, stderr } = cause as { status: number | null; signal: string | null; stderr: string }
      const hex = status === null ? '' : ` (0x${(status >>> 0).toString(16).toUpperCase()})`
      throw new Error(`the child exited ${status}${hex}${signal === null ? '' : `, signal ${signal}`}: ${stderr.trim() || 'no stderr'}`, { cause })
    }
    const report = JSON.parse(output) as { size: number; loaded: boolean }
    expect(report.size).toBeGreaterThan(0)
    expect(report.loaded).toBe(true)
  }, 30_000)
})
