/**
 * Memory reclamation check script.
 *
 * Run with:
 *   yarn check:memory
 *
 * The --expose-gc flag enables global.gc() for forced GC between samples,
 * giving cleaner readings. The script still runs without it but readings
 * will be noisier.
 *
 * What it checks:
 *   1. Non-worker mode: memory across N renders with ImageNode — verifies
 *      images are not accumulating in the LRU cache between renders.
 *   2. Worker mode — no release(): renders N times without calling
 *      WorkerCanvas.release(), showing Canvas objects accumulate in the worker.
 *      Note: A FinalizationRegistry provides a safety net, but explicit .release()
 *      is still recommended for deterministic cleanup in production.
 *   3. Worker mode — with release(): same renders but release() is called,
 *      verifying memory is reclaimed.
 *
 * Worker sections (2 & 3) require compiled .js output. The script runs
 * `yarn build` automatically at startup to ensure dist/ is up to date.
 */

import { existsSync } from 'fs'
import { execSync } from 'child_process'
import { cpus } from 'os'
import type { Root as RootFn, terminate as terminateFn } from '@/canvas/root.canvas.js'
import type { Image as ImageFn } from '@/canvas/image.canvas.js'

// ---------------------------------------------------------------------------
// Worker availability — the compiled .js must exist; tsx cannot run .ts workers
// ---------------------------------------------------------------------------

const _workerJs = 'dist/esm/worker/render.worker.js'
const WORKER_AVAILABLE = existsSync(_workerJs)

const IMAGE_URL = 'https://raw.githubusercontent.com/MadeBaruna/paimon-moe/refs/heads/main/static/images/banners/Void%20Star%27s%20Advent%201.png'

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function heapMB() {
  return process.memoryUsage().heapUsed / 1024 / 1024
}

function tryGC() {
  if (typeof (global as any).gc === 'function') {
    ;(global as any).gc()
  }
}

function printSamples(label: string, samples: number[]) {
  const first = samples[0]
  const last = samples[samples.length - 1]
  const peak = Math.max(...samples)
  const growth = last - first

  console.log(`\n  ${label}`)
  console.log(`    renders : ${samples.length}`)
  console.log(`    start   : ${first.toFixed(2)} MB`)
  console.log(`    end     : ${last.toFixed(2)} MB`)
  console.log(`    peak    : ${peak.toFixed(2)} MB`)
  console.log(`    growth  : ${growth > 0 ? '+' : ''}${growth.toFixed(2)} MB  ${growth > 5 ? '⚠ possible leak' : '✓ looks fine'}`)
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

async function main() {
  console.log('Memory reclamation check')
  console.log(`Node ${process.version}  |  GC available: ${typeof (global as any).gc === 'function'}`)
  console.log(`Image source: ${IMAGE_URL}`)

  // Build first to ensure dist/ is up to date
  console.log('\nRunning yarn build...')
  try {
    execSync('yarn build', { stdio: 'inherit' })
  } catch (err) {
    console.error('Build failed:', err)
    process.exit(1)
  }

  // Runtime imports from dist (required for worker path resolution).
  // Paths are stored in variables so TS skips static module resolution on them.
  const [rootDistPath, imageDistPath] = ['dist/esm/canvas/root.canvas.js', 'dist/esm/canvas/image.canvas.js'] as string[]
  const [{ Root, terminate }, { Image }] = (await Promise.all([import(rootDistPath), import(imageDistPath)])) as [
    { Root: typeof RootFn; terminate: typeof terminateFn },
    { Image: typeof ImageFn },
  ]

  function makeTree() {
    return {
      width: 400,
      height: 400,
      children: [Image({ src: IMAGE_URL, width: 200, height: 400 }), Image({ src: IMAGE_URL, width: 200, height: 400 })],
    } as Parameters<typeof Root>[0]
  }

  // ---------------------------------------------------------------------------
  // Section 1: Non-worker mode
  // ---------------------------------------------------------------------------

  async function checkNonWorkerMode() {
    console.log('\n========================================')
    console.log(' Section 1: Non-worker mode')
    console.log('========================================')

    const RENDERS = 30
    const samples: number[] = []

    // Warm-up — let Node settle allocations
    for (let i = 0; i < 3; i++) {
      const canvas = await Root({ ...makeTree(), workerMode: false })
      canvas.toBufferSync('png')
    }

    tryGC()

    for (let i = 0; i < RENDERS; i++) {
      const canvas = await Root({ ...makeTree(), workerMode: false })
      canvas.toBufferSync('png')
      tryGC()
      samples.push(heapMB())
    }

    printSamples(`${RENDERS} renders × 2 ImageNodes (same URL)`, samples)
  }

  // ---------------------------------------------------------------------------
  // Section 2: Worker mode — no release()
  // ---------------------------------------------------------------------------

  async function checkWorkerModeNoRelease() {
    console.log('\n========================================')
    console.log(' Section 2: Worker mode — no release()')
    console.log('========================================')

    if (!WORKER_AVAILABLE) {
      console.log('\n  ⚠ SKIPPED — tsx runs source .ts files; worker threads require compiled .js.')
      console.log('  Add scripts/ to the rollup input, run `yarn build`, then:')
      console.log('    node --expose-gc dist/esm/scripts/check_memory.js')
      return
    }

    console.log(' (Canvas objects accumulate inside the worker until release() is called)')

    const RENDERS = 20
    const samples: number[] = []

    for (let i = 0; i < 2; i++) {
      const canvas = (await Root({ ...makeTree(), workerMode: true, workers: Math.max(1, cpus().length - 1) })) as any
      canvas.toBufferSync('png')
      // intentionally NOT calling canvas.release()
    }

    tryGC()

    for (let i = 0; i < RENDERS; i++) {
      const canvas = (await Root({ ...makeTree(), workerMode: true, workers: Math.max(1, cpus().length - 1) })) as any
      canvas.toBufferSync('png')
      // intentionally NOT calling canvas.release()
      tryGC()
      samples.push(heapMB())
    }

    printSamples(`${RENDERS} renders, release() never called`, samples)
    console.log('\n  NOTE: Canvas objects live in the worker heap — main-thread numbers')
    console.log('  above under-report the true retained memory in the worker.')
  }

  // ---------------------------------------------------------------------------
  // Section 3: Worker mode — with release()
  // ---------------------------------------------------------------------------

  async function checkWorkerModeWithRelease() {
    console.log('\n========================================')
    console.log(' Section 3: Worker mode — with release()')
    console.log('========================================')

    if (!WORKER_AVAILABLE) {
      console.log('\n  ⚠ SKIPPED — worker .js not found (see Section 2 note).')
      return
    }

    const RENDERS = 20
    const samples: number[] = []

    for (let i = 0; i < 2; i++) {
      const canvas = (await Root({ ...makeTree(), workerMode: true, workers: Math.max(1, cpus().length - 1) })) as any
      canvas.toBufferSync('png')
      canvas.release()
    }

    tryGC()

    for (let i = 0; i < RENDERS; i++) {
      const canvas = (await Root({ ...makeTree(), workerMode: true, workers: Math.max(1, cpus().length - 1) })) as any
      canvas.toBufferSync('png')
      canvas.release()
      tryGC()
      samples.push(heapMB())
    }

    printSamples(`${RENDERS} renders, release() called after each`, samples)
  }

  // ---------------------------------------------------------------------------
  // Run checks
  // ---------------------------------------------------------------------------

  try {
    await checkNonWorkerMode()
    await checkWorkerModeNoRelease()
    await checkWorkerModeWithRelease()
  } catch (err) {
    console.error('\nScript error:', err)
    process.exit(1)
  }

  console.log('\n========================================')
  console.log(' Done')
  console.log('========================================\n')

  terminate()
}

void main()
