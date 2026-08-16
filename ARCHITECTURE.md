# Architecture

## Overview

`meo-canvas` renders a declarative component tree into a raster image. It combines a **flexbox layout engine** (yoga-layout) with a **2D drawing library** (meo-skia-canvas), plus an optional **worker-thread pool** (Comlink) for non-blocking server-side rendering.

```
User Code
  │
  ▼
Root({ width, children: [...] })
  │
  ▼
┌─────────────────────────────────────────┐
│ RootNode.render()                       │
│                                         │
│  1. Register fonts (serialized)         │
│  2. Load images (concurrency-limited)    │
│  3. Layout tree (yoga-layout)           │
│  4. Draw tree (meo-skia-canvas)             │
│  5. Export (PNG/JPEG/PDF)               │
└─────────────────────────────────────────┘
         │                    │
    workerMode: false    workerMode: true
         │                    │
         ▼                    ▼
   Direct render      ComlinkPool.render()
   on main thread         │
                          ▼
                    render.worker.ts
                    RootNode.render()
                          │
                          ▼
                    Buffer returned
                    to main thread
```

## Directory Structure

```
src/
├── canvas/              # Core rendering nodes
│   ├── canvas.type.ts   # TypeScript types & interfaces
│   ├── canvas.helper.ts # Shared drawing utilities (borders)
│   ├── root.canvas.ts   # Root() entry point & rendering pipeline
│   ├── page.plan.ts     # Page-count resolution & page builder invocation
│   ├── layout.canvas.ts # Box, Column, Row (flexbox via yoga-layout)
│   ├── text.canvas.ts   # Text with inline HTML-like styling
│   ├── image.canvas.ts  # Image loading, caching, fit/position
│   ├── chart.canvas.ts  # Bar, Line, Pie, Doughnut charts
│   └── grid.canvas.ts   # CSS Grid-like layout
├── worker/                # Worker thread infrastructure
│   ├── comlink.pool.ts    # Fixed-size worker pool with queue
│   ├── comlink.setup.ts   # Comlink adapter for worker_threads
│   ├── canvas-handlers.ts # Allowlisted canvas methods callable from the pool
│   ├── sync.bridge.ts     # Blocking channel for the *Sync export methods
│   ├── render.worker.ts   # Worker entry point (exposes WorkerAPI)
│   └── worker.types.ts    # Worker message types
├── util/
│   └── disk.cache.ts    # Disk-based image cache for re-decodes
├── constant/
│   └── common.const.ts  # Style enums (Border), Yoga constants re-export
└── index.ts             # Public API barrel export
```

## Node Hierarchy

```
BaseNode               (abstract — name, key, __type)
  └── BoxNode           (base layout node — flexbox, margins, padding, borders, bg)
        ├── RootNode    (entry point — font registration, image loading, render)
        ├── ColumnNode  (shorthand: flexDirection = 'column')
        └── RowNode     (shorthand: flexDirection = 'row')
  ├── TextNode          (rich text with <color>, <weight>, <size>, <b>, <i>)
  ├── ImageNode         (URL / file / buffer, objectFit, objectPosition, saturate)
  ├── ChartNode         (Bar / Line / Pie / Doughnut)
  ├── GridNode          (CSS Grid container)
  └── GridItemNode      (Grid cell with column/row span)
```

## Rendering Pipeline

### Phase 1 — Font Registration

Fonts are registered into `meo-skia-canvas`'s `FontLibrary` with a **serialization lock** (`_fontRegistrationLock`). Only new fonts (not already in `registeredFonts`) trigger `FontLibrary.use()`. The lock prevents concurrent `Root()` calls from racing on font registration.

### Phase 2 — Image Loading

All `ImageNode` instances in the tree are collected via BFS. Images from URLs or local paths are fetched concurrently (default: 5 at a time, configurable via `imageConcurrency`). A `RenderImageCache` deduplicates identical `src` + `color` combinations. The cache spans the whole render rather than one page, so a source referenced by every frame of an animation is fetched once. Optional `useDiskCache` writes fetched images to disk for faster re-decode within the same render pass.

### Phase 3 — Layout

Each node creates a `yoga-layout` node and wires up flexbox properties (`width`, `height`, `flexDirection`, `justifyContent`, `alignItems`, `gap`, `margin`, `padding`, `border`). The tree is calculated top-down — `RootNode` calls `calculateLayout()` on the root yoga node, then each child reads its computed position/dimensions via `getComputedLayout()`.

### Phase 4 — Drawing

With layout computed, each node draws itself on the `meo-skia-canvas` context:

- **BoxNode** draws background color, background image, and borders before recursing into children
- **TextNode** parses inline HTML-like tags and draws styled text segments
- **ImageNode** draws the loaded image with `objectFit` / `objectPosition` / `saturate`
- **ChartNode** draws chart elements (axes, bars, lines, pie slices)
- **GridNode** positions children in a 2D grid based on column/row definitions

### Phase 5 — Export

The canvas is handed back unencoded; the caller chooses a format. `toBuffer(format, options)` reaches the matching `meo-skia-canvas` encoder on the worker (or main thread) — `png`, `jpg`, `webp`, `avif`, `tiff`, `bmp`, `ico`, `svg`, `pdf`, `gif`, `apng` and `raw`.

Export signatures are split by format: `fps`, `loop` and `frameDelays` are accepted for `gif` and `apng` and rejected for everything else, which turns a documented runtime `TypeError` into a compile error.

## Paged Rendering

A page is a frame for `gif` and `apng`, a sheet for `pdf` and `tiff`, and a size for `ico`. Passing a function as `children` renders a sequence — one page per call — and `page.plan.ts` owns the arithmetic:

- `resolvePageCount()` turns `pages` or `duration * fps` into a count and rejects every contradictory combination. It runs at runtime because the type system cannot reach JavaScript callers, `as any`, or props arriving over the worker boundary; the `Root` overloads reject the same shapes at compile time.
- `pageInfoAt()` builds the `PageInfo` a builder receives — `index`, `count`, `progress` for interpolation, and `time` for physics integration.
- `planPages()` runs the builder once per page, sequentially, so page order is the array order and a data-loading builder does not burst every request at once.

`renderPages()` then builds **one `RootNode` per page**. The tree is constructed in the node's constructor and freed once drawn, and a freed Yoga node cannot be laid out again — so pages cannot share a node. What is expensive is shared instead: one image cache and one font registration for the whole sequence. Each page's tree is released in a `finally` as soon as it is drawn, so memory stays flat across a long sequence rather than holding every page's layout at once.

The first page owns the canvas; each later one is appended with `newPage(width, height)`.

## Worker Architecture

### Why workers?

Server-side canvas rendering is CPU-heavy. Running it on the main thread blocks the event loop. Workers move rendering off-thread, keeping the main thread responsive.

### Pool design

`ComlinkPool` maintains a fixed pool of N workers (default: `cpus() - 1`). Each worker wraps a `WorkerAPI` object exposed via Comlink.

**Idle path**: If a worker is free, the render occurs immediately.

**Queued path**: If all workers are busy, tasks queue in FIFO order. When a worker finishes, `drain()` dequeues the next task.

### Function serialization

Some props contain callback functions (e.g., `ChartProps.renderValue`). Functions can't be `structuredClone`'d. The pool uses a **sentinel protocol**:

1. `extractFunctions()` replaces every function with `{ __comlinkFnId: number }` and stores the original in a `Map`.
2. A single `Comlink.proxy()` callback is created that dispatches `{__comlinkFnId, args}` back to the main thread.
3. On the worker side, `restoreFunctions()` replaces sentinels with async proxy calls.

This adds 1 additional round-trip per function call, but avoids the complexity of per-function proxies.

### Lifecycle

- `Root({ workerMode: false })` — renders on the main thread, returns a bare `Canvas`. No `.release()`.
- `Root({ workerMode: true })` — renders in a worker, returns a `WorkerCanvas` proxy. Calls to `.toBuffer()`, `.toURL()`, `.toFile()` are proxied back to the worker via `callOnCanvas()`.
- `.release()` — tells the worker to free the canvas from its internal `Map`. Optional (garbage collection via `FinalizationRegistry`).
- `terminate()` — kills all worker threads. The pool lazily re-initializes on the next `Root()` call.

## Key Design Decisions

### Factory functions, not JSX

Each component is a plain function call: `Box({ ... })`, `Text('hello', { ... })`. No transpilation step needed. This keeps the API simple for server-side use cases.

### Yoga-based layout

Flexbox via yoga-layout gives predictable, CSS-like layouts without a browser DOM. Each node gets a yoga node; the parent calculates layout; children read their computed positions.

### `CanvasElement` discriminated union

For serialization across the worker boundary, the tree is convertible to a `CanvasElement[]` array — a discriminated union keyed by `__type`. This is the transport format between main thread and worker.

It also has no function member, which is what lets `typeof children === 'function'` tell a page builder from ordinary children without ambiguity.

### Page builders resolve on the calling thread

A worker render resolves the builder before dispatch and ships plain data. A function cannot be structured-cloned, and while the pool's sentinel protocol could marshal it, that would cost a round trip per page — and a tree returned back through the callback proxy would hit `DataCloneError`, since the `Image` factory's `Omit` is type-level only and its callbacks survive at runtime.

### No garbage collected canvas in workers

Workers hold canvases in a `Map<number, Canvas>`. The main-thread `FinalizationRegistry` calls `releaseCanvas()` when a `WorkerCanvas` proxy is GC'd. This is a safety net — users should call `.release()` explicitly for predictable cleanup.

### Font registration mutex

A promise-based lock (`_fontRegistrationLock`) serializes `FontLibrary.use()` calls. Without this, concurrent `Root()` calls could both call `FontLibrary.use()` for the same font, causing a meo-skia-canvas error.
