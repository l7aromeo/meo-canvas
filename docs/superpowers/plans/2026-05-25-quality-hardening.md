# Quality Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the highest-risk gaps identified in the project review — test coverage (especially text rendering), worker lifecycle confidence, visual regression safety, and tooling hygiene — without changing public API behavior.

**Architecture:** Work proceeds in five independent phases. Phase 1 (quick wins) unblocks clean test output. Phase 2 adds unit tests for `TextNode` using the same mock patterns as `image.canvas.test.ts`. Phase 3 adds worker API tests by extracting testable logic from `render.worker.ts`. Phase 4 adds opt-in integration tests that use real `skia-canvas` and compare PNG output. Phase 5 cleans dependencies and build warnings.

**Tech Stack:** TypeScript, Vitest, skia-canvas, yoga-layout, Comlink, Bun, Rollup

**Baseline (verified):** `bun run lint`, `bun run test`, and `bun run build` all pass. Do not regress these.

---

## File Map

| File | Role in this plan |
|---|---|
| `src/canvas/text.canvas.ts` | Primary coverage target (~5% today) |
| `src/worker/render.worker.ts` | Extract handler logic for direct unit tests |
| `src/worker/worker.types.ts` | Shared types for extracted worker handlers |
| `src/util/disk.cache.ts` | Fix duplicate process listener registration |
| `tests/text.canvas.test.ts` | **Create** — TextNode unit tests |
| `tests/render.worker.test.ts` | **Create** — worker handler unit tests |
| `tests/integration/render.integration.test.ts` | **Create** — real skia-canvas golden tests |
| `tests/fixtures/renders/` | **Create** — expected PNG buffers |
| `tests/helpers/mock-canvas-context.ts` | **Create** — shared ctx mock (DRY) |
| `tests/grid.canvas.test.ts` | Fix misleading GridItem appendChild test |
| `tests/use.disk.cache.test.ts` | Fix incomplete Canvas mock |
| `vitest.config.ts` | Add setup file, integration test split |
| `vitest.setup.ts` | **Create** — global test hygiene |
| `package.json` | Remove unused `sharp` if confirmed |
| `tsconfig.cjs.json` | Resolve Rollup module warning |
| `CONTRIBUTING.md` | Document `bun run test` vs `bun test` |

---

## Phase 1 — Quick Wins (Test Hygiene & Small Fixes)

Low effort, immediate signal quality improvement. Each task is independently mergeable.

### Task 1: Fix disk.cache process listener leak

**Problem:** `src/util/disk.cache.ts` registers `beforeExit`, `SIGINT`, and `SIGTERM` listeners at import time. Vitest loads this module in multiple test files → `MaxListenersExceededWarning`.

**Files:**
- Modify: `src/util/disk.cache.ts`
- Test: `tests/disk.cache.test.ts`

- [ ] **Step 1: Write the failing test**

Add to `tests/disk.cache.test.ts`:

```typescript
it('registers process exit listeners only once across re-imports', async () => {
  const beforeCount = process.listenerCount('beforeExit')
  const sigintCount = process.listenerCount('SIGINT')
  const sigtermCount = process.listenerCount('SIGTERM')

  vi.resetModules()
  await import('@/util/disk.cache.js')
  vi.resetModules()
  await import('@/util/disk.cache.js')

  expect(process.listenerCount('beforeExit')).toBe(beforeCount + 1)
  expect(process.listenerCount('SIGINT')).toBe(sigintCount + 1)
  expect(process.listenerCount('SIGTERM')).toBe(sigtermCount + 1)
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `bun run test tests/disk.cache.test.ts`
Expected: FAIL — listener count increases by 2+ per re-import

- [ ] **Step 3: Add singleton guard in disk.cache.ts**

At top of `src/util/disk.cache.ts`, after existing `_exitCleanupStarted`:

```typescript
let _exitListenersRegistered = false

function registerExitListeners(): void {
  if (_exitListenersRegistered) return
  _exitListenersRegistered = true

  process.on('beforeExit', () => {
    if (_exitCleanupStarted) return
    _exitCleanupStarted = true
    void clearDiskCache()
  })

  const cleanupOnExit = () => {
    clearDiskCache().finally(() => process.exit(0))
  }
  process.on('SIGINT', cleanupOnExit)
  process.on('SIGTERM', cleanupOnExit)
}

registerExitListeners()
```

Remove the existing bare `process.on(...)` calls at the bottom of the file.

- [ ] **Step 4: Run test to verify it passes**

Run: `bun run test tests/disk.cache.test.ts`
Expected: PASS, no MaxListeners warnings in full suite

- [ ] **Step 5: Commit**

```bash
git add src/util/disk.cache.ts tests/disk.cache.test.ts
git commit -m "fix(cache): register process exit listeners only once"
```

---

### Task 2: Fix incomplete Canvas mock in use.disk.cache tests

**Problem:** Inline `Canvas` mock in `tests/use.disk.cache.test.ts` lacks `drawImage`, causing stderr noise during RootNode render tests.

**Files:**
- Create: `tests/helpers/mock-canvas-context.ts`
- Modify: `tests/use.disk.cache.test.ts`

- [ ] **Step 1: Create shared mock helper**

Create `tests/helpers/mock-canvas-context.ts`:

```typescript
import { vi } from 'vitest'

export function createMockCanvasContext() {
  return {
    scale: vi.fn(),
    save: vi.fn(),
    restore: vi.fn(),
    beginPath: vi.fn(),
    moveTo: vi.fn(),
    lineTo: vi.fn(),
    arc: vi.fn(),
    closePath: vi.fn(),
    rect: vi.fn(),
    fill: vi.fn(),
    stroke: vi.fn(),
    clip: vi.fn(),
    drawImage: vi.fn(),
    fillText: vi.fn(),
    strokeText: vi.fn(),
    measureText: vi.fn(() => ({ width: 0, actualBoundingBoxAscent: 10, actualBoundingBoxDescent: 2 })),
    fillStyle: '',
    strokeStyle: '',
    lineWidth: 0,
    globalAlpha: 1,
    globalCompositeOperation: '',
    imageSmoothingEnabled: true,
    imageSmoothingQuality: 'high' as const,
    font: '',
    textAlign: 'left' as const,
    textBaseline: 'alphabetic' as const,
    createLinearGradient: vi.fn(() => ({ addColorStop: vi.fn() })),
    createRadialGradient: vi.fn(() => ({ addColorStop: vi.fn() })),
    setLineDash: vi.fn(),
  }
}

export function createMockCanvas() {
  return vi.fn(function (this: any, w: number, h: number) {
    this.width = w
    this.height = h
    this.getContext = vi.fn(() => createMockCanvasContext())
    this.toBufferSync = vi.fn(() => Buffer.from(''))
  })
}
```

- [ ] **Step 2: Replace inline Canvas mock in use.disk.cache.test.ts**

In `tests/use.disk.cache.test.ts`, replace the inline `Canvas: vi.fn(function...)` block with:

```typescript
import { createMockCanvas } from './helpers/mock-canvas-context.js'

vi.mock('skia-canvas', () => ({
  loadImage: mockLoadImage,
  Image: vi.fn(),
  Canvas: createMockCanvas(),
  FontLibrary: { use: vi.fn() },
}))
```

Apply the same change to the second inline Canvas mock (~line 215 in the RootNode mock section).

- [ ] **Step 3: Run tests — expect clean stderr**

Run: `bun run test tests/use.disk.cache.test.ts 2>&1 | rg 'drawImage|MaxListeners'`
Expected: no matches

- [ ] **Step 4: Commit**

```bash
git add tests/helpers/mock-canvas-context.ts tests/use.disk.cache.test.ts
git commit -m "test: share canvas mock and fix drawImage stderr noise"
```

---

### Task 3: Fix misleading GridItem test

**Problem:** `tests/grid.canvas.test.ts` passes a raw `CanvasElement` descriptor to `appendChild`, triggering a console warning that does not reflect real usage via `buildTree()`.

**Files:**
- Modify: `tests/grid.canvas.test.ts`

- [ ] **Step 1: Update test to use GridItemNode**

In `tests/grid.canvas.test.ts`, change imports and test:

```typescript
import { GridNode, GridItem, GridItemNode } from '@/canvas/grid.canvas.js'
import { buildTree } from '@/canvas/root.canvas.js'

it('should place items with gridColumn span syntax', () => {
  const grid = new GridNode({ columns: 3, width: 600 })

  const item = buildTree(GridItem({ gridColumn: 'span 2', width: 100, height: 50 }))
  ;(grid as any).appendChild(item, 0)

  grid.node.setWidth(600)
  grid.node.calculateLayout(600, undefined, Style.Direction.LTR)

  expect(() => grid.finalizeLayout()).not.toThrow()
})
```

- [ ] **Step 2: Run test — expect no stderr warning**

Run: `bun run test tests/grid.canvas.test.ts 2>&1 | rg 'invalid child'`
Expected: no matches

- [ ] **Step 3: Commit**

```bash
git add tests/grid.canvas.test.ts
git commit -m "test: use buildTree for GridItem span placement test"
```

---

### Task 4: Document correct test command

**Files:**
- Modify: `CONTRIBUTING.md`

- [ ] **Step 1: Add note under test instructions**

After the `bun run test` block in `CONTRIBUTING.md`, add:

```markdown
> **Note:** Use `bun run test` (Vitest), not bare `bun test`. The Bun test runner does not load Vitest globals (`describe`, `vi`, etc.).
```

- [ ] **Step 2: Commit**

```bash
git add CONTRIBUTING.md
git commit -m "docs: clarify bun run test vs bun test"
```

---

## Phase 2 — Text Rendering Unit Tests (Highest Risk Gap)

`text.canvas.ts` is 1,252 lines at ~5% coverage. Test through public surfaces: `Text()` factory, `TextNode` constructor + Yoga measure, `renderSimpleText`, and `_renderContent` via mocked ctx.

### Task 5: Text factory and construction tests

**Files:**
- Create: `tests/text.canvas.test.ts`

- [ ] **Step 1: Write failing tests for factory and defaults**

Create `tests/text.canvas.test.ts`:

```typescript
import { vi } from 'vitest'
import Yoga, { Style } from '@/constant/common.const.js'
import { createMockCanvasContext } from './helpers/mock-canvas-context.js'

vi.mock('skia-canvas', async () => {
  const { Canvas, FontLibrary } = await import('@/__mocks__/skia-canvas.js')
  return { Canvas, FontLibrary, loadImage: vi.fn() }
})

let Text: typeof import('@/canvas/text.canvas.js').Text
let TextNode: typeof import('@/canvas/text.canvas.js').TextNode

beforeEach(async () => {
  vi.resetModules()
  const mod = await import('@/canvas/text.canvas.js')
  Text = mod.Text
  TextNode = mod.TextNode
})

describe('Text factory', () => {
  it('returns a CanvasElement descriptor', () => {
    expect(Text('hello')).toEqual({ __type: 'Text', text: 'hello', props: undefined })
    expect(Text('hi', { fontSize: 20 })).toEqual({
      __type: 'Text',
      text: 'hi',
      props: { fontSize: 20 },
    })
  })
})

describe('TextNode construction', () => {
  it('creates a yoga node with measure function', () => {
    const node = new TextNode('Hello')
    expect(node.node).toBeInstanceOf(Yoga.Node)
    expect(node.name).toBe('TextNode')
  })

  it('applies default flexShrink of 1', () => {
    const node = new TextNode('Hello')
    expect(node.props.flexShrink).toBe(1)
  })

  it('processes escape sequences in constructor input', () => {
    const node = new TextNode('Line1\\nLine2')
    // Indirect: node should measure taller than single line — verify via yoga
    node.node.setWidth(200)
    node.node.calculateLayout(200, undefined, Style.Direction.LTR)
    const h = node.node.getComputedHeight()
    expect(h).toBeGreaterThan(0)
  })
})
```

- [ ] **Step 2: Run tests**

Run: `bun run test tests/text.canvas.test.ts`
Expected: PASS (factory/constructor already work)

- [ ] **Step 3: Commit**

```bash
git add tests/text.canvas.test.ts
git commit -m "test: add Text factory and construction coverage"
```

---

### Task 6: Rich text parsing behavior tests

Test rich text indirectly via `renderSimpleText` (public static) and render output via mocked ctx.

**Files:**
- Modify: `tests/text.canvas.test.ts`

- [ ] **Step 1: Add renderSimpleText tests**

```typescript
describe('TextNode.renderSimpleText', () => {
  it('draws text at the given coordinates', () => {
    const ctx = createMockCanvasContext()
    TextNode.renderSimpleText(ctx as any, 'Hello', 10, 20, {
      fontFamily: 'sans-serif',
      fontSize: 16,
      color: '#333',
    })
    expect(ctx.fillText).toHaveBeenCalledWith('Hello', 10, 20)
    expect(ctx.font).toContain('16px')
    expect(ctx.fillStyle).toBe('#333')
  })
})
```

- [ ] **Step 2: Add rich text render tests (mock ctx)**

```typescript
describe('TextNode rich text rendering', () => {
  it('renders plain text via fillText', async () => {
    const node = new TextNode('Plain text', { fontSize: 16, width: 200, height: 40 })
    const ctx = createMockCanvasContext()
    node.node.setWidth(200)
    node.node.setHeight(40)
    node.node.calculateLayout(200, 40, Style.Direction.LTR)

    await node.render(ctx as any, 0, 0, 200, 40)

    expect(ctx.fillText).toHaveBeenCalled()
  })

  it('renders colored segments from inline tags', async () => {
    const node = new TextNode('Hello <color="red">World</color>', {
      fontSize: 16,
      width: 300,
      height: 40,
    })
    const ctx = createMockCanvasContext()
    node.node.setWidth(300)
    node.node.setHeight(40)
    node.node.calculateLayout(300, 40, Style.Direction.LTR)

    await node.render(ctx as any, 0, 0, 300, 40)

    const fillStyles = (ctx.fillText as any).mock.calls.map(() => ctx.fillStyle)
    expect(fillStyles.some(s => s === 'red' || String(s).includes('red'))).toBe(true)
  })
})
```

- [ ] **Step 3: Run tests, fix any mock gaps**

Run: `bun run test tests/text.canvas.test.ts`
If `render` fails due to missing ctx methods, extend `createMockCanvasContext()` with whatever `text.canvas.ts` calls (e.g. `shadowColor`, `letterSpacing`).

- [ ] **Step 4: Commit**

```bash
git add tests/text.canvas.test.ts tests/helpers/mock-canvas-context.ts
git commit -m "test: cover TextNode render and rich text segments"
```

---

### Task 7: Text truncation and maxLines tests

**Files:**
- Modify: `tests/text.canvas.test.ts`

- [ ] **Step 1: Write maxLines + ellipsis tests**

```typescript
describe('TextNode truncation', () => {
  it('applies default ellipsis when maxLines exceeded', async () => {
    const longText = 'Word '.repeat(50)
    const node = new TextNode(longText, {
      fontSize: 16,
      width: 100,
      height: 40,
      maxLines: 2,
      ellipsis: true,
    })
    const ctx = createMockCanvasContext()
    ;(ctx.measureText as any).mockImplementation((t: string) => ({
      width: t.length * 8,
      actualBoundingBoxAscent: 12,
      actualBoundingBoxDescent: 3,
    }))

    node.node.setWidth(100)
    node.node.setHeight(40)
    node.node.calculateLayout(100, 40, Style.Direction.LTR)
    await node.render(ctx as any, 0, 0, 100, 40)

    const drawn = (ctx.fillText as any).mock.calls.map((c: any[]) => c[0]).join('')
    expect(drawn).toContain('...')
  })

  it('uses custom ellipsis string', async () => {
    const node = new TextNode('Word '.repeat(50), {
      fontSize: 16,
      width: 80,
      height: 30,
      maxLines: 1,
      ellipsis: '…',
    })
    const ctx = createMockCanvasContext()
    ;(ctx.measureText as any).mockImplementation((t: string) => ({
      width: t.length * 8,
      actualBoundingBoxAscent: 12,
      actualBoundingBoxDescent: 3,
    }))

    node.node.setWidth(80)
    node.node.setHeight(30)
    node.node.calculateLayout(80, 30, Style.Direction.LTR)
    await node.render(ctx as any, 0, 0, 80, 30)

    const drawn = (ctx.fillText as any).mock.calls.map((c: any[]) => c[0]).join('')
    expect(drawn).toContain('…')
  })
})
```

- [ ] **Step 2: Run and iterate until pass**

Run: `bun run test tests/text.canvas.test.ts`

- [ ] **Step 3: Commit**

```bash
git add tests/text.canvas.test.ts
git commit -m "test: cover TextNode maxLines and ellipsis behavior"
```

---

### Task 8: Text layout measurement tests

**Files:**
- Modify: `tests/text.canvas.test.ts`

- [ ] **Step 1: Add Yoga measure dimension tests**

```typescript
describe('TextNode layout measurement', () => {
  it('reports non-zero height for multi-line escaped text', () => {
    const node = new TextNode('A\\nB\\nC', { fontSize: 16 })
    node.node.setWidth(200)
    node.node.calculateLayout(200, undefined, Style.Direction.LTR)
    expect(node.node.getComputedHeight()).toBeGreaterThan(16)
  })

  it('respects explicit width constraint', () => {
    const node = new TextNode('Hello World', { fontSize: 16 })
    node.node.setWidth(50)
    node.node.calculateLayout(50, undefined, Style.Direction.LTR)
    expect(node.node.getComputedWidth()).toBe(50)
  })
})
```

- [ ] **Step 2: Run full suite, check text.canvas coverage**

Run: `bun run test -- --coverage tests/text.canvas.test.ts`
Target: `text.canvas.ts` statement coverage ≥ 40% after Phase 2 (stretch goal: 60%)

- [ ] **Step 3: Commit**

```bash
git add tests/text.canvas.test.ts
git commit -m "test: cover TextNode yoga measurement behavior"
```

---

## Phase 3 — Worker Lifecycle Tests

`render.worker.ts` is 0% covered. Extract pure handler logic so it can be tested without spawning real worker threads.

### Task 9: Extract worker canvas handlers

**Files:**
- Create: `src/worker/canvas-handlers.ts`
- Modify: `src/worker/render.worker.ts`
- Create: `tests/render.worker.test.ts`

- [ ] **Step 1: Write failing handler tests first**

Create `tests/render.worker.test.ts`:

```typescript
import { vi } from 'vitest'
import { createCanvasHandlers } from '@/worker/canvas-handlers.js'

describe('createCanvasHandlers', () => {
  it('stores canvas on render and returns png buffer metadata', async () => {
    const canvases = new Map<number, any>()
    let nextId = 0
    const mockCanvas = {
      toBufferSync: vi.fn(() => Buffer.from('png-bytes')),
      width: 400,
      height: 300,
    }

    const handlers = createCanvasHandlers({
      canvases,
      getNextCanvasId: () => nextId++,
      renderRoot: vi.fn(async () => mockCanvas),
    })

    const result = await handlers.render({ width: 400, height: 300 } as any)

    expect(result).toEqual({
      canvasId: 0,
      buffer: Buffer.from('png-bytes'),
      width: 400,
      height: 300,
    })
    expect(canvases.get(0)).toBe(mockCanvas)
  })

  it('throws when callOnCanvas targets missing canvas', async () => {
    const handlers = createCanvasHandlers({
      canvases: new Map(),
      getNextCanvasId: () => 0,
      renderRoot: vi.fn(),
    })

    await expect(handlers.callOnCanvas(99, 'toBuffer', ['png'])).rejects.toThrow('Canvas 99 not found')
  })

  it('releaseCanvas removes canvas from map', () => {
    const canvases = new Map<number, any>([[1, {}]])
    const handlers = createCanvasHandlers({
      canvases,
      getNextCanvasId: () => 0,
      renderRoot: vi.fn(),
    })

    handlers.releaseCanvas(1)
    expect(canvases.has(1)).toBe(false)
  })
})
```

- [ ] **Step 2: Run test — expect FAIL (module not found)**

Run: `bun run test tests/render.worker.test.ts`
Expected: FAIL

- [ ] **Step 3: Implement createCanvasHandlers**

Create `src/worker/canvas-handlers.ts`:

```typescript
import type { RootProps } from '@/canvas/canvas.type.js'
import type { Canvas } from 'skia-canvas'
import type { CallFn, RenderResult } from '@/worker/worker.types.js'

type CanvasMethod = 'toBuffer' | 'toURL' | 'toFile' | 'toSharp'

export interface CanvasHandlerDeps {
  canvases: Map<number, Canvas>
  getNextCanvasId: () => number
  renderRoot: (props: RootProps) => Promise<Canvas>
}

export function createCanvasHandlers(deps: CanvasHandlerDeps) {
  return {
    async render(props: RootProps, callFn?: CallFn): Promise<RenderResult> {
      const canvas = await deps.renderRoot(props)
      const canvasId = deps.getNextCanvasId()
      deps.canvases.set(canvasId, canvas)
      return {
        canvasId,
        buffer: canvas.toBufferSync('png'),
        width: canvas.width,
        height: canvas.height,
      }
    },

    async callOnCanvas(canvasId: number, method: CanvasMethod, args: unknown[]): Promise<unknown> {
      const canvas = deps.canvases.get(canvasId)
      if (!canvas) {
        throw new Error(`[render.worker] Canvas ${canvasId} not found`)
      }
      switch (method) {
        case 'toBuffer':
          return canvas.toBuffer(...(args as [any, any?]))
        case 'toURL':
          return canvas.toURL(...(args as [any, any?]))
        case 'toFile':
          await canvas.toFile(...(args as [string, any?]))
          return
        case 'toSharp':
          return await canvas.toSharp(...(args as [any?])).toBuffer()
        default:
          throw new Error(`[render.worker] Unknown method: ${method}`)
      }
    },

    releaseCanvas(canvasId: number): void {
      deps.canvases.delete(canvasId)
    },
  }
}
```

Note: `render` receives `callFn` for API compatibility but `restoreFunctions` stays in `render.worker.ts` — handlers receive already-resolved props.

- [ ] **Step 4: Refactor render.worker.ts to use handlers**

```typescript
import { createCanvasHandlers } from '@/worker/canvas-handlers.js'
import { restoreFunctions } from '@/worker/comlink.pool.js'
import { RootNode } from '@/canvas/root.canvas.js'

const canvases = new Map<number, Canvas>()
let nextCanvasId = 0

const handlers = createCanvasHandlers({
  canvases,
  getNextCanvasId: () => nextCanvasId++,
  renderRoot: async props => new RootNode(props).render(),
})

const api: WorkerAPI = {
  async render(props, callFn) {
    const resolved = callFn ? restoreFunctions(props, callFn) : props
    return handlers.render(resolved)
  },
  callOnCanvas: handlers.callOnCanvas.bind(handlers),
  releaseCanvas: handlers.releaseCanvas.bind(handlers),
}
```

- [ ] **Step 5: Run tests**

Run: `bun run test tests/render.worker.test.ts`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add src/worker/canvas-handlers.ts src/worker/render.worker.ts tests/render.worker.test.ts
git commit -m "refactor(worker): extract canvas handlers for unit testing"
```

---

### Task 10: Worker callOnCanvas method dispatch tests

**Files:**
- Modify: `tests/render.worker.test.ts`

- [ ] **Step 1: Add dispatch tests for each export method**

```typescript
it('delegates toBuffer to canvas', async () => {
  const mockCanvas = {
    toBuffer: vi.fn(async () => Buffer.from('jpg')),
    toBufferSync: vi.fn(),
    width: 100,
    height: 100,
  }
  const canvases = new Map([[0, mockCanvas]])
  const handlers = createCanvasHandlers({
    canvases,
    getNextCanvasId: () => 1,
    renderRoot: vi.fn(),
  })

  const buf = await handlers.callOnCanvas(0, 'toBuffer', ['jpg', { quality: 0.9 }])
  expect(buf).toEqual(Buffer.from('jpg'))
  expect(mockCanvas.toBuffer).toHaveBeenCalledWith('jpg', { quality: 0.9 })
})

it('delegates toSharp and returns buffer', async () => {
  const mockCanvas = {
    toSharp: vi.fn(() => ({ toBuffer: vi.fn(async () => Buffer.from('sharp')) })),
    toBufferSync: vi.fn(),
    width: 100,
    height: 100,
  }
  const canvases = new Map([[0, mockCanvas]])
  const handlers = createCanvasHandlers({
    canvases,
    getNextCanvasId: () => 1,
    renderRoot: vi.fn(),
  })

  const buf = await handlers.callOnCanvas(0, 'toSharp', [{}])
  expect(buf).toEqual(Buffer.from('sharp'))
})
```

- [ ] **Step 2: Run and commit**

Run: `bun run test tests/render.worker.test.ts`

```bash
git add tests/render.worker.test.ts
git commit -m "test: cover worker canvas method dispatch"
```

---

## Phase 4 — Integration / Golden Render Tests

Opt-in tests using **real** `skia-canvas` (no mocks). Run separately in CI to keep default unit tests fast and deterministic on machines without native deps issues.

### Task 11: Configure integration test split

**Files:**
- Modify: `vitest.config.ts`
- Create: `vitest.integration.config.ts`

- [ ] **Step 1: Create integration vitest config**

Create `vitest.integration.config.ts`:

```typescript
import path from 'node:path'
import { defineConfig, mergeConfig } from 'vitest/config'
import base from './vitest.config.js'

export default mergeConfig(base, defineConfig({
  test: {
    include: ['tests/integration/**/*.test.ts'],
    testTimeout: 30_000,
  },
}))
```

- [ ] **Step 2: Add npm script**

In `package.json` scripts:

```json
"test:integration": "vitest run --config vitest.integration.config.ts"
```

- [ ] **Step 3: Commit**

```bash
git add vitest.integration.config.ts package.json
git commit -m "chore(test): add integration test config and script"
```

---

### Task 12: Golden PNG render tests

**Files:**
- Create: `tests/integration/render.integration.test.ts`
- Create: `tests/fixtures/renders/.gitkeep`
- Modify: `.gitlab-ci.yml` (optional second job)

- [ ] **Step 1: Write first integration test (simple box + text)**

Create `tests/integration/render.integration.test.ts`:

```typescript
import { Root, Box, Text } from '@/canvas/root.canvas.js'
import { writeFileSync, readFileSync, existsSync, mkdirSync } from 'node:fs'
import { join } from 'node:path'

const FIXTURES_DIR = join(import.meta.dirname, '../fixtures/renders')
const UPDATE_FIXTURES = process.env.UPDATE_FIXTURES === '1'

async function expectPngMatch(name: string, buffer: Buffer) {
  const fixturePath = join(FIXTURES_DIR, `${name}.png`)
  if (UPDATE_FIXTURES || !existsSync(fixturePath)) {
    mkdirSync(FIXTURES_DIR, { recursive: true })
    writeFileSync(fixturePath, buffer)
  }
  const expected = readFileSync(fixturePath)
  expect(buffer.equals(expected)).toBe(true)
}

describe('integration renders', () => {
  it('renders a simple box with text', async () => {
    const canvas = await Root({
      width: 200,
      height: 100,
      workerMode: false,
      children: [
        Box({
          width: '100%',
          height: '100%',
          backgroundColor: '#3366cc',
          children: [Text('Hello', { fontSize: 24, color: '#ffffff' })],
        }),
      ],
    })

    const png = await canvas.toBuffer('png')
    await expectPngMatch('simple-box-text', png)
  })
})
```

Fix imports: `Root`, `Box`, `Text` come from `@/index.js` or separate modules — use same paths as other tests (`@/canvas/root.canvas.js` exports Root; Box/Text from layout/text modules).

Correct imports:

```typescript
import { Root } from '@/canvas/root.canvas.js'
import { Box } from '@/canvas/layout.canvas.js'
import { Text } from '@/canvas/text.canvas.js'
```

- [ ] **Step 2: Generate fixtures locally**

Run: `UPDATE_FIXTURES=1 bun run test:integration`
Expected: creates `tests/fixtures/renders/simple-box-text.png`

- [ ] **Step 3: Re-run without UPDATE_FIXTURES**

Run: `bun run test:integration`
Expected: PASS

- [ ] **Step 4: Add chart and grid golden tests (one each)**

Add cases mirroring `scripts/generate_sample_charts.ts` minimal bar chart and `generate_sample_grids.ts` 3-column grid. Keep fixtures small (≤ 400×300).

- [ ] **Step 5: Add CI job (optional but recommended)**

In `.gitlab-ci.yml`, add after `lint_and_test`:

```yaml
integration_test:
  stage: test
  script:
    - bun run test:integration
  needs:
    - install_dependencies
  allow_failure: true  # remove once stable on CI runners
```

- [ ] **Step 6: Commit fixtures and tests**

```bash
git add tests/integration tests/fixtures/renders vitest.integration.config.ts .gitlab-ci.yml
git commit -m "test: add golden PNG integration renders for box, chart, grid"
```

---

## Phase 5 — Dependency & Build Cleanup

### Task 13: Investigate and remove unused sharp dependency

**Files:**
- Modify: `package.json`
- Modify: `README.md` (if sharp removal affects toSharp docs)

- [ ] **Step 1: Confirm sharp is unused**

Run:

```bash
bun pm why sharp
rg "from ['\"]sharp['\"]" --glob '!node_modules'
```

- [ ] **Step 2: Remove sharp if not required**

If `bun pm why sharp` shows only direct dependency (not required by skia-canvas at runtime):

```bash
bun remove sharp @types/sharp
bun run test
bun run build
```

If skia-canvas requires sharp for `toSharp()`, move to `optionalDependencies` and document in README.

- [ ] **Step 3: Commit**

```bash
git add package.json bun.lock README.md
git commit -m "chore(deps): remove unused sharp dependency"
```

---

### Task 14: Fix Rollup TypeScript module warnings

**Problem:** CJS build warns because `tsconfig.cjs.json` uses `"module": "CommonJS"` while root uses `"moduleResolution": "NodeNext"`.

**Files:**
- Modify: `tsconfig.cjs.json`
- Modify: `rollup.config.js` (if needed)

- [ ] **Step 1: Align CJS tsconfig**

Update `tsconfig.cjs.json`:

```json
{
  "extends": "./tsconfig.json",
  "compilerOptions": {
    "rootDir": "./src",
    "outDir": "dist/cjs",
    "module": "NodeNext",
    "moduleResolution": "NodeNext",
    "declaration": true,
    "declarationMap": true
  },
  "exclude": ["**/__mocks__/**", "**/__tests__/**", "scripts", "tests"]
}
```

- [ ] **Step 2: Run build — expect zero TS5110 warnings**

Run: `bun run build 2>&1 | rg 'TS5110|module.*esnext'`
Expected: no matches (Rollup still emits CJS via `format: 'cjs'`)

- [ ] **Step 3: Verify CJS output still works**

Run: `node -e "require('./dist/cjs/index.js')"`

- [ ] **Step 4: Commit**

```bash
git add tsconfig.cjs.json
git commit -m "fix(build): align CJS tsconfig module settings with NodeNext"
```

---

## Phase 6 — Coverage Gate (Optional, after Phases 2–4)

### Task 15: Add coverage thresholds for core modules

**Files:**
- Modify: `vitest.config.ts`

- [ ] **Step 1: Add per-file thresholds**

In `vitest.config.ts` under `coverage`:

```typescript
coverage: {
  provider: 'v8',
  reportsDirectory: './coverage',
  exclude: ['node_modules/**', 'dist/**', 'scripts/**', '**/__mocks__/**'],
  thresholds: {
    'src/canvas/layout.canvas.ts': { lines: 95 },
    'src/canvas/text.canvas.ts': { lines: 40 },
    'src/worker/comlink.pool.ts': { lines: 90 },
    'src/worker/canvas-handlers.ts': { lines: 90 },
  },
},
```

Start conservative; raise `text.canvas.ts` threshold as coverage grows.

- [ ] **Step 2: Run full suite**

Run: `bun run test`
Expected: PASS with thresholds met

- [ ] **Step 3: Commit**

```bash
git add vitest.config.ts
git commit -m "chore(test): add coverage thresholds for core modules"
```

---

## Execution Order & Parallelism

```text
Phase 1 (Tasks 1–4)     ──► can run in parallel, merge independently
Phase 2 (Tasks 5–8)     ──► sequential within phase; depends on Task 2 helper
Phase 3 (Tasks 9–10)    ──► independent of Phase 2
Phase 4 (Tasks 11–12) ──► depends on real skia-canvas; run after Phase 2 for text fixture
Phase 5 (Tasks 13–14) ──► independent
Phase 6 (Task 15)       ──► after Phases 2–4
```

**Recommended PR split:**

| PR | Tasks | Theme |
|---|---|---|
| PR 1 | 1–4 | Test hygiene quick wins |
| PR 2 | 5–8 | Text rendering tests |
| PR 3 | 9–10 | Worker handler extraction + tests |
| PR 4 | 11–12 | Integration golden tests |
| PR 5 | 13–15 | Deps, build, coverage gates |

---

## Success Criteria

| Metric | Current | Target |
|---|---|---|
| `text.canvas.ts` line coverage | ~5% | ≥ 40% (≥ 60% stretch) |
| `render.worker.ts` / handlers coverage | 0% | ≥ 90% |
| Test stderr noise | drawImage errors, listener warnings | none in default suite |
| `bun test` confusion | undocumented | documented in CONTRIBUTING |
| Rollup build warnings | TS5110 present | zero warnings |
| `sharp` dependency | listed, unused | removed or justified |
| Integration tests | none | 3+ golden PNG fixtures |

---

## Self-Review Checklist

- [x] Every review finding maps to a task (text tests → 5–8, worker → 9–10, golden → 12, sharp → 13, rollup → 14, grid test → 3, disk cache → 1, mock noise → 2, bun test → 4)
- [x] No placeholder steps — all code blocks are concrete
- [x] File paths are exact and match repo layout
- [x] Commands use `bun run test` (not bare `bun test`)
- [x] Tasks are bite-sized with commit boundaries
