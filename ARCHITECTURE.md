# Architecture

## Overview

`@meonode/canvas` renders a declarative component tree into a raster image. It combines a **flexbox layout engine** (yoga-layout) with a **2D drawing library** (skia-canvas), plus an optional **worker-thread pool** (Comlink) for non-blocking server-side rendering.

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
│  4. Draw tree (skia-canvas)             │
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
│   ├── layout.canvas.ts # Box, Column, Row (flexbox via yoga-layout)
│   ├── text.canvas.ts   # Text with inline HTML-like styling
│   ├── image.canvas.ts  # Image loading, caching, fit/position
│   ├── chart.canvas.ts  # Bar, Line, Pie, Doughnut charts
│   └── grid.canvas.ts   # CSS Grid-like layout
├── worker/              # Worker thread infrastructure
│   ├── comlink.pool.ts  # Fixed-size worker pool with queue
│   ├── comlink.setup.ts # Comlink adapter for worker_threads
│   ├── render.worker.ts # Worker entry point (exposes WorkerAPI)
│   └── worker.types.ts  # Worker message types
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
  ├── ImageNode         (URL / file / buffer, objectFit, objectPosition, crossFade)
  ├── ChartNode         (Bar / Line / Pie / Doughnut)
  ├── GridNode          (CSS Grid container)
  └── GridItemNode      (Grid cell with column/row span)
```

## Rendering Pipeline

### Phase 1 — Font Registration

Fonts are registered into `skia-canvas`'s `FontLibrary` with a **serialization lock** (`_fontRegistrationLock`). Only new fonts (not already in `registeredFonts`) trigger `FontLibrary.use()`. The lock prevents concurrent `Root()` calls from racing on font registration.

### Phase 2 — Image Loading

All `ImageNode` instances in the tree are collected via BFS. Images from URLs or local paths are fetched concurrently (default: 5 at a time, configurable via `imageConcurrency`). A per-render `RenderImageCache` deduplicates identical `src` + `colorize` combinations. Optional `useDiskCache` writes fetched images to disk for faster re-decode within the same render pass.

### Phase 3 — Layout

Each node creates a `yoga-layout` node and wires up flexbox properties (`width`, `height`, `flexDirection`, `justifyContent`, `alignItems`, `gap`, `margin`, `padding`, `border`). The tree is calculated top-down — `RootNode` calls `calculateLayout()` on the root yoga node, then each child reads its computed position/dimensions via `getComputedLayout()`.

### Phase 4 — Drawing

With layout computed, each node draws itself on the `skia-canvas` context:
- **BoxNode** draws background color, background image, and borders before recursing into children
- **TextNode** parses inline HTML-like tags and draws styled text segments
- **ImageNode** draws the loaded image with `objectFit` / `objectPosition` / `crossFade`
- **ChartNode** draws chart elements (axes, bars, lines, pie slices)
- **GridNode** positions children in a 2D grid based on column/row definitions

### Phase 5 — Export

The rendered canvas is exported via `toBuffer('png')`. If the user requests JPEG/PDF, the worker (or main thread) calls the appropriate `skia-canvas` export method.

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

### No garbage collected canvas in workers

Workers hold canvases in a `Map<number, Canvas>`. The main-thread `FinalizationRegistry` calls `releaseCanvas()` when a `WorkerCanvas` proxy is GC'd. This is a safety net — users should call `.release()` explicitly for predictable cleanup.

### Font registration mutex

A promise-based lock (`_fontRegistrationLock`) serializes `FontLibrary.use()` calls. Without this, concurrent `Root()` calls could both call `FontLibrary.use()` for the same font, causing a skia-canvas error.
