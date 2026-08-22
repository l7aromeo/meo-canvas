/**
 * The front door: a scene, painted, with the ways to read it back.
 *
 * `Root` is the only thing in this package that touches native code. Everything
 * else builds plain objects; this walks them into an arena, hands that across
 * once, and returns a {@link Canvas} over the painted surface.
 *
 * **Resolve, measure, layout and paint happen here, once.** Every method on the
 * canvas that comes back is an encode of work already done, which is why two
 * formats cost one paint.
 *
 * @packageDocumentation
 */

import { writeFileSync } from 'node:fs'
import { writeFile } from 'node:fs/promises'
import { createRequire } from 'node:module'

import { encodeScene, type SideValue } from './arena.js'
import { Canvas, type NativeCanvas } from './canvas.js'
import { Box, type Children, type SceneNode } from './node.js'
import type { Style } from './style.js'

/** A font family, and the files that make it up. */
export interface FontRegistration {
  /** The name `fontFamily` refers to. Any name may be chosen; it need not match the file. */
  readonly family: string
  /** Paths to the font files of that family — one per weight or style. */
  readonly paths: readonly string[]
}

/**
 * Where one page sits in a sequence.
 *
 * The four derived numbers rather than the index alone, because each is the
 * right one for a different job and deriving the wrong one is a bug that looks
 * like a design choice.
 */
export interface PageInfo {
  /** Zero-based position in the sequence. */
  readonly index: number
  /** Total pages in this render. */
  readonly count: number
  /**
   * Position along the sequence, `0` on the first page and `1` on the last.
   *
   * `index / (count - 1)`, which spans the sequence inclusively: a one-shot
   * animation should finish at its end value on the frame the viewer stops on.
   * The wrong curve for anything that repeats — see {@link PageInfo.cycle}. A
   * single-page render reports `0`.
   */
  readonly progress: number
  /**
   * Position around a loop, `0` on the first page and approaching `1` without
   * reaching it.
   *
   * `index / count`, and the one to feed anything periodic. `1` and `0` are the
   * same point on a circle, so driving a rotation from {@link PageInfo.progress}
   * makes the last page a copy of the first and the animation stutters for one
   * frame on every repeat. A single-page render reports `0`.
   */
  readonly cycle: number
  /**
   * Seconds elapsed at this page, `index / fps`.
   *
   * What physics and spring integration want. Spans `[0, duration)` for the
   * reason {@link PageInfo.cycle} does.
   */
  readonly time: number
}

/**
 * Builds the content of one page.
 *
 * Only `Root` takes this form: pages exist at the canvas level, so a nested
 * element has no page of its own to describe. May be asynchronous, because a
 * page's content may have to be fetched.
 */
export type PageBuilder = (page: PageInfo) => Children | Promise<Children>

/**
 * What `Root` accepts: the canvas, and the page root's own style.
 *
 * The style properties sit here directly, as they do on every other factory —
 * a page root is an ordinary node, so it takes the ordinary style set.
 */
export type RootProps = Style & {
  /**
   * What to draw.
   *
   * Elements for a single page, or a function for a sequence — one page per
   * call. The function form needs either {@link RootProps.pages} or
   * {@link RootProps.duration}.
   */
  readonly children?: Children | PageBuilder
  /** How many pages to render. Not with {@link RootProps.duration}. */
  readonly pages?: number
  /** How long the sequence runs, in seconds; the page count is `ceil(duration * fps)`. */
  readonly duration?: number
  /**
   * The rate {@link RootProps.duration} and {@link PageInfo.time} are derived at.
   *
   * Describes the render, not the encode. An animation encoded to play at this
   * rate needs it passed to `toBuffer('gif', { fps })` as well.
   */
  readonly fps?: number
  /** The canvas width in pixels. Text cannot wrap without knowing its room. */
  readonly width: number
  /**
   * The canvas height in pixels.
   *
   * Required, where v1 derives it from the content when it is left out. The
   * renderer has no content-sizing pass for a page root — it gives the root the
   * scene's extent on any axis left automatic — so a height derived from
   * content is not something this surface can honour yet rather than something
   * it chooses not to. Making it optional later takes nothing away.
   */
  readonly height: number
  /** Device pixel ratio. */
  readonly scale?: number
  /** Font files to register for this render. */
  readonly fonts?: readonly FontRegistration[]
  /**
   * Rasterise on the GPU when there is one. `false` forces the CPU.
   *
   * Asking is not getting: a build without GPU support or a driver that
   * declines falls back. Set it `false` for output that must be identical
   * between machines — the two rasterisers resolve anti-aliased edges a level or
   * two apart, which a pixel comparison sees.
   */
  readonly gpu?: boolean
  /** A name carried through for diagnostics. */
  readonly name?: string
}

/** What the addon has to give back: a painted surface, ready to encode. */
export interface PaintOptions {
  /** Whether to rasterise on the GPU. */
  readonly gpu: boolean
  /** The families to register before laying anything out. */
  readonly fonts: readonly FontRegistration[]
}

/**
 * The native side of `Root`.
 *
 * Declared rather than imported, for the reason {@link NativeCanvas} is: this
 * file compiles without the addon, and the shape the addon has to satisfy is
 * written down in one place.
 */
export interface NativeRenderer {
  /** Resolves, measures, lays out and paints. Once. */
  paint(slots: Float64Array, values: readonly SideValue[], options: PaintOptions): Promise<NativeCanvas>
}

/** How the filesystem is reached. Injected so a caller without `node:fs` can supply their own. */
export interface RootDependencies {
  /** What paints the scene. */
  readonly renderer: NativeRenderer
  /** Writes bytes to a path. */
  readonly writeFile: (path: string, bytes: Uint8Array) => Promise<void>
  /** Writes bytes to a path, blocking. */
  readonly writeFileSync: (path: string, bytes: Uint8Array) => void
}

/**
 * The addon, and the filesystem, which is what `Root` uses when told nothing.
 *
 * Resolved on the first call rather than when this module loads: a caller who
 * supplies their own renderer — a test, or a host without a filesystem — should
 * not need the native module present to import the package.
 */
function installed(): RootDependencies {
  const addon = load()
  return {
    renderer: {
      paint: async (slots, values, options) => addon.paint(slots, values, options),
    },
    writeFile,
    writeFileSync,
  }
}

/** What `Root` calls into. */
interface Addon {
  /** Paints a scene and returns the surface, retained until it is released. */
  paint(slots: Float64Array, values: readonly SideValue[], options: PaintOptions): Promise<NativeCanvas>
}

/** The built addon, or an error naming what is missing. */
function load(): Addon {
  let module: Partial<Addon>
  try {
    module = createRequire(import.meta.url)('../meo-canvas.node') as Partial<Addon>
  } catch (cause) {
    throw new Error('the addon is not built; run `just addon`', { cause })
  }
  if (typeof module.paint !== 'function') {
    throw new TypeError(
      'the addon exports no `paint`. `Root` needs a painted surface that is retained until it is released — `render` returns encoded bytes, so every format would repaint the scene and the sync methods could not exist at all.',
    )
  }
  return module as Addon
}

/** The rate a sequence is timed at when nothing says otherwise. */
const DEFAULT_FPS = 30

/** The device pixel ratio when nothing says otherwise. */
const DEFAULT_SCALE = 1

/** Whether a value is a page builder rather than content. */
function isBuilder(children: Children | PageBuilder | undefined): children is PageBuilder {
  return typeof children === 'function'
}

/**
 * How many pages the props ask for, and at what rate.
 *
 * `pages` and `duration` are two spellings of one number and naming both is a
 * contradiction rather than a preference, so it is refused. A page count named
 * without a builder is refused too: there is no per-page content to vary, so
 * `pages: 5` beside static children asked for something that would not happen.
 */
function sequence(props: RootProps): { count: number; fps: number } {
  const fps = props.fps ?? DEFAULT_FPS

  if (props.pages !== undefined && props.duration !== undefined) {
    throw new TypeError('name `pages` or `duration`, not both; they are two spellings of one page count')
  }

  const asked = props.pages ?? (props.duration === undefined ? undefined : Math.ceil(props.duration * fps))
  if (asked === undefined) {
    if (isBuilder(props.children)) {
      throw new TypeError('a page builder needs `pages` or `duration`; without one there is no sequence to build')
    }
    return { count: 1, fps }
  }

  if (!isBuilder(props.children)) {
    throw new TypeError('`pages` and `duration` describe a sequence, so `children` has to be a function that builds one')
  }
  if (!Number.isInteger(asked) || asked < 1) {
    throw new RangeError(`a render has at least one page; ${asked} is not a count`)
  }
  return { count: asked, fps }
}

/** Where one page sits, given the sequence it belongs to. */
function pageInfo(index: number, count: number, fps: number): PageInfo {
  return {
    index,
    count,
    // A single page is the start of its own sequence and the whole of it at
    // once, so both curves report zero rather than dividing by no interval.
    progress: count > 1 ? index / (count - 1) : 0,
    cycle: count > 1 ? index / count : 0,
    time: index / fps,
  }
}

/** The page roots the props describe, in order. */
async function pages(props: RootProps): Promise<readonly SceneNode[]> {
  const { count, fps } = sequence(props)
  const builder = props.children

  if (!isBuilder(builder)) {
    return [Box({ ...props, children: builder })]
  }

  const built: SceneNode[] = []
  for (let index = 0; index < count; index += 1) {
    // Sequentially rather than all at once: a builder may fetch, and a
    // thousand-page render firing a thousand requests at once is a denial of
    // service a caller did not ask for.
    // eslint-disable-next-line no-await-in-loop
    const children = await builder(pageInfo(index, count, fps))
    built.push(Box({ ...props, children }))
  }
  return built
}

/**
 * Paints a scene and returns the canvas.
 *
 * ```ts
 * import { Root, Row, Text } from 'meo-canvas'
 *
 * const canvas = await Root({
 *   width: 520,
 *   height: 180,
 *   backgroundColor: '#101014',
 *   children: Row({ padding: 24, children: Text('Ukasyah', { fontSize: 26 }) }),
 * })
 *
 * await canvas.toFile('card.png')
 * ```
 */
export async function Root(props: RootProps, dependencies: RootDependencies = installed()): Promise<Canvas> {
  const scale = props.scale ?? DEFAULT_SCALE
  const arena = encodeScene(await pages(props), props.width, props.height, scale)

  const native = await dependencies.renderer.paint(arena.slots, arena.values, {
    gpu: props.gpu ?? true,
    fonts: props.fonts ?? [],
  })

  return new Canvas(native, dependencies.writeFile, dependencies.writeFileSync)
}
