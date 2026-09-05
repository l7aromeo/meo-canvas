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

import { resolveAddon } from './addon.js'
import { encodeScene, type SideValue, type SurfaceOptions } from './arena.js'
import { Canvas, type NativeCanvas } from './canvas.js'
import { Box, type Children, type SceneNode } from './node.js'
import type { ColorSpace, ColorType } from './index.js'
import type { Style } from './style.js'

/**
 * How long a fetch may take, connection and body together.
 *
 * **Sixty seconds, matching `GLOBAL_TIMEOUT` in `meo-canvas-core`'s `resolve`,
 * so the two surfaces wait the same length of time.** `fetch` has no timeout of
 * its own: without this, a URL that accepts a connection and then says nothing
 * holds the render for as long as the peer cares to hold it, which on a server
 * is a request that never completes rather than one that fails.
 *
 * That a caller *can* pass their own `AbortSignal` is not a reason to leave it
 * unbounded. A default that hangs is a defect whether or not it is overridable,
 * and it is the same argument the crate rejected for itself.
 */
const FETCH_TIMEOUT_MS = 60_000

/**
 * The signal a fetch runs under: this renderer's ceiling, and the caller's.
 *
 * **A ceiling rather than a default, and the difference is the whole point.** A
 * bound `httpOptions` could raise would be this defect with a supported
 * spelling: a signal that never fires, or an omitted one, and the hang is
 * reachable again through the documented API. `AbortSignal.any` aborts on the
 * first of the two, so a caller who knows their host may ask for five seconds
 * and get five, and nobody gets sixty-one.
 *
 * Nothing that works today breaks: a caller's existing signal keeps behaving
 * exactly as it did, because tightening is all it could ever do. A caller who
 * wants a different policy fetches the bytes themselves and passes them inline,
 * which is the same escape the crate offers and is what makes the two surfaces
 * consistent rather than merely similar.
 *
 * The ceiling is returned beside the composed signal because the two failures
 * have to be told apart afterwards: a limit this renderer chose and a caller's
 * own abort send a reader to different places.
 *
 * `ms` is a parameter so the ceiling can be asserted in a test without waiting
 * a minute for it. It is not reachable from {@link RootProps} — which is the
 * property under test.
 */
export function fetchDeadline(
  caller: AbortSignal | null | undefined,
  ms: number = FETCH_TIMEOUT_MS,
): { readonly signal: AbortSignal; readonly ceiling: AbortSignal } {
  const ceiling = AbortSignal.timeout(ms)
  const signal = caller === undefined || caller === null ? ceiling : AbortSignal.any([ceiling, caller])
  return { ceiling, signal }
}

/**
 * The largest image this surface will fetch.
 *
 * Thirty-two mebibytes, matching `MAX_IMAGE_BYTES` in `meo-canvas-core`. Sixty
 * seconds against it is a floor of about 4.5 Mbit/s, which is why the timeout
 * and the cap are one decision rather than two.
 */
const MAX_IMAGE_BYTES = 32 * 1024 * 1024

/**
 * The response body, refused once it passes {@link MAX_IMAGE_BYTES}.
 *
 * **Counted while reading rather than believed from `content-length`**, which a
 * server may omit and may lie about — the crate's own fetch says the same, and
 * a cap that trusts a header is a cap an attacker sets. `arrayBuffer()` cannot
 * do this: it has already allocated the whole body by the time it returns.
 *
 * The limit is named as this renderer's in the message, because a caller
 * meeting it needs to know whose number it is before they can decide whether to
 * fetch the bytes themselves and pass them as an inline source — which is the
 * escape, and the same one the crate offers.
 */
async function bounded(url: string, response: Response): Promise<Uint8Array> {
  const reader = response.body?.getReader()
  // A body with no stream — an empty response, or a `fetch` a test replaced
  // with something simpler. Nothing to read incrementally, so the check is the
  // same one against what arrived.
  if (reader === undefined) {
    const whole = new Uint8Array(await response.arrayBuffer())
    if (whole.byteLength > MAX_IMAGE_BYTES) throw tooLarge(url)
    return whole
  }

  const chunks: Uint8Array[] = []
  let total = 0
  for (;;) {
    const { done, value } = await reader.read()
    if (done) break
    total += value.byteLength
    if (total > MAX_IMAGE_BYTES) {
      // Stop the transfer rather than draining it: the point of counting while
      // reading is that the bytes past the limit are never taken.
      await reader.cancel()
      throw tooLarge(url)
    }
    chunks.push(value)
  }

  const bytes = new Uint8Array(total)
  let at = 0
  for (const chunk of chunks) {
    bytes.set(chunk, at)
    at += chunk.byteLength
  }
  return bytes
}

/** The refusal, worded as the crate words its own. */
function tooLarge(url: string): TypeError {
  return new TypeError(`cannot fetch ${JSON.stringify(url)}: the image is larger than the ${MAX_IMAGE_BYTES / (1024 * 1024)} MiB this renderer fetches`)
}

/**
 * A font family, and the files that make it up.
 *
 * # Registering a family changes the process, and cannot be undone
 *
 * **A family registered by one render is registered for every render after it,
 * on that thread, until the process exits.** Nothing unregisters anything,
 * passing different files under the same name replaces what that name draws
 * from then on, and a render that names a family it never registered will use
 * whatever an earlier one left behind rather than failing.
 *
 * That last part is the one to leave with, because it is not an error a log
 * will show: it is the wrong typeface in a picture nobody looks at twice. A
 * server that registers per request, one tenant's face at a time, is a server
 * where the next request quietly renders in the previous tenant's font.
 *
 * ```ts
 * // Register once, where the process starts.
 * const FONTS = [{ family: 'Brand', paths: ['./fonts/Brand-Regular.ttf'] }]
 * const canvas = await Root({ width: 200, height: 100, fonts: FONTS, children: [] })
 * canvas.release()
 * ```
 *
 * Registering the same list on every render is fine and costs nothing beyond
 * the file read — it is *varying* it that is the hazard. Faces belong at
 * start-up, named once, and every render after that names the same ones.
 *
 * Each worker thread is its own process for this purpose: they do not share a
 * registry, so each has to register its own faces, and each is contaminated
 * only by itself. Two renders already in flight keep their own faces.
 *
 * The registry lives in `meo-skia-canvas`, below the addon, and neither surface
 * can scope it — see `crates/meo-canvas-core/src/resolve.rs`, which says the
 * same thing to a Rust caller and has the measurement behind it.
 */
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
   * call. The function form needs either {@link RootProps}'s `pages` or
   * {@link RootProps}'s `duration`.
   */
  readonly children?: Children | PageBuilder
  /** How many pages to render. Not with {@link RootProps}'s `duration`. */
  readonly pages?: number
  /** How long the sequence runs, in seconds; the page count is `ceil(duration * fps)`. */
  readonly duration?: number
  /**
   * The rate {@link RootProps}'s `duration` and {@link PageInfo.time} are derived at.
   *
   * Describes the render, not the encode. An animation encoded to play at this
   * rate needs it passed to `toBuffer('gif', { fps })` as well.
   */
  readonly fps?: number
  /**
   * The canvas width in pixels. Text cannot wrap without knowing its room.
   *
   * # Nothing bounds this, and a size too large fails late
   *
   * **There is no maximum.** The limit is what the machine can allocate, and
   * the allocation does not happen here: painting records a drawing rather than
   * a bitmap, so `Root` returns cheaply at any size —
   * `{ width: 200000, height: 200000 }` resolves with the process still under
   * 80 MB — and the whole cost arrives at
   * {@link Canvas.toBuffer}. A size that cannot work therefore fails *after*
   * the render has been paid for rather than when it was set.
   *
   * Measured on one machine, as the shape rather than a specification:
   * 8000×8000 succeeded at 610 MB, 16384×16384 at 2244 MB and 5.7 s, and
   * 32768×32768 threw `Could not allocate new 32768×32768 bitmap`. Failure at
   * the top is clean; the hazard is the middle, where two gigabytes are spent
   * without anything objecting.
   *
   * **So a width that came from a request needs a bound of the caller's own.**
   * The one ceiling this package enforces is on node count, not on pixels: a
   * scene above 1048576 nodes is refused with `the arena declares N nodes, the
   * limit is 1048576`.
   */
  readonly width: number
  /**
   * The canvas height in pixels, or omitted for the height of the content.
   *
   * Width has no such form and cannot: text breaks into lines against a width,
   * so a width has to be known before anything can be measured. A height is a
   * result of that measuring, which is why only this one can be left out.
   *
   * `minHeight` on the same props is the floor when this is omitted, so a page
   * can be "as tall as its content, and at least this tall".
   */
  readonly height?: number
  /** Device pixel ratio. */
  readonly scale?: number
  /**
   * Font families to register before this render.
   *
   * **Not only for this render.** Registration is process-wide and permanent —
   * see {@link FontRegistration} for what that costs a server. Name the same
   * list every time.
   */
  readonly fonts?: readonly FontRegistration[]
  /**
   * Passed to `fetch` for every URL source in the scene.
   *
   * `RequestInit` as the platform defines it, so headers, credentials, an
   * `AbortSignal` and a proxy agent all work the way they do everywhere else in
   * this runtime rather than through a second set of options this package
   * invented. One object for the whole render: a per-source variant would be a
   * larger promise than v1 made and nothing has asked for it.
   *
   * Only the URLs are fetched — **bytes cross the wire to the renderer, never a
   * URL** — so a `credentials` or `Authorization` set here reaches the origin
   * and nothing else.
   */
  readonly httpOptions?: RequestInit
  /**
   * Rasterise on the GPU when there is one. `false` forces the CPU.
   *
   * Asking is not getting: a build without GPU support or a driver that
   * declines falls back. Set it `false` for output that must be identical
   * between machines — the two rasterisers resolve anti-aliased edges a level or
   * two apart, which a pixel comparison sees.
   */
  readonly gpu?: boolean
  /**
   * The pixel layout the canvas composites in.
   *
   * Governs the precision everything is drawn at and the depth the encoded
   * formats that carry one write. `'F32'` keeps colour outside sRGB rather than
   * clipping it as it is drawn, at the cost of the GPU — no GPU composites
   * float. Absent leaves it to the renderer.
   */
  readonly colorType?: ColorType
  /**
   * The colour space the canvas composites in.
   *
   * Fixed for the whole render rather than chosen per export: colours are
   * interpreted in it, and one outside its gamut is clipped as it is drawn.
   * Absent leaves it to the renderer.
   */
  readonly colorSpace?: ColorSpace
  /** A name carried through for diagnostics. */
  readonly name?: string
}

/**
 * What the addon needs beyond the scene.
 *
 * Only the fonts. `gpu`, `colorType` and `colorSpace` are the **scene's** —
 * they cross in the arena header beside the scale, because the canvas is the
 * sized, coloured thing and the scene is what describes it. Saying any of them
 * here as well would be two places that can disagree.
 *
 * Fonts stay here because a renderer outlives any one scene: a server registers
 * its families once and draws a thousand pictures with them. **That reads as a
 * scope and is not one** — the registry is the process's, so registering once
 * is the pattern to follow rather than a constraint the API enforces. See
 * {@link FontRegistration}.
 */
export interface PaintOptions {
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
  /**
   * Resolves, measures, lays out and paints. Once.
   *
   * Synchronous, because the addon's is: the painted surface holds Skia types
   * that are not `Send`, so the paint cannot leave the loop the way `render`'s
   * does. `Root` is asynchronous anyway — a page builder may fetch.
   */
  paint(slots: Float64Array, values: readonly SideValue[], options: PaintOptions): NativeCanvas
}

/**
 * What `Root` reaches for, injected so a caller can supply their own.
 *
 * **The filesystem used to be here too, and is not any more.** `toFile` writes
 * the file where it encodes it rather than handing bytes back, so there is
 * nothing left for an injected writer to receive; the native surface is the
 * one seam, and it is the seam a test wanted anyway.
 */
export interface RootDependencies {
  /** What paints the scene. */
  readonly renderer: NativeRenderer
}

/**
 * The addon, which is what `Root` uses when told nothing.
 *
 * Resolved on the first call rather than when this module loads: a caller who
 * supplies their own renderer — a test, or a host without a filesystem — should
 * not need the native module present to import the package.
 */
function installed(): RootDependencies {
  return { renderer: load() }
}

/** What `Root` calls into. */
type Addon = NativeRenderer

/** The built addon, or an error naming what is missing. */
function load(): Addon {
  const module = resolveAddon<Partial<Addon>>()
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
  // Absent rather than defaulted: the arena carries "the caller said nothing"
  // as a distinct thing from "the caller said true", and it is the renderer
  // that decides when nothing was said.
  const surface: SurfaceOptions = {
    ...(props.gpu === undefined ? {} : { gpu: props.gpu }),
    ...(props.colorType === undefined ? {} : { colorType: props.colorType }),
    ...(props.colorSpace === undefined ? {} : { colorSpace: props.colorSpace }),
  }
  // The tree is built **once**, not once per encode: a page builder is a
  // caller's function and may fetch, count or otherwise refuse to be run twice.
  const tree = await pages(props)
  // A height that was not given is a height the content decides. The floor
  // travels in the same field, because a stated `minHeight` is what "at least
  // this tall" means and the renderer reads it as the page's minimum.
  const contentHeight = props.height === undefined
  const height = props.height ?? (typeof props.minHeight === 'number' ? props.minHeight : 0)
  let arena = encodeScene(tree, props.width, height, contentHeight, scale, surface)

  // **Fetched here, at the surface, and only bytes cross the wire.**
  //
  // `meo-canvas-core` can fetch too, behind a default-off `net` feature, and
  // that is deliberate rather than duplication: with the feature off it refuses
  // a URL exactly as this surface used to, so **the two surfaces fail the same
  // way and the difference between them is a build flag rather than a
  // capability gap.** Doing it here as well means the addon needs no HTTP stack
  // and a Node caller gets `fetch`, with the platform's own proxy, TLS and DNS
  // rather than a second set inside a native module.
  //
  // The second encode is the price of not having a second walker. This module
  // would otherwise need its own idea of everywhere a source can appear — image
  // `src`, background image, mask — which is exactly the kind of duplicate that
  // drifts the first time a source moves. The encoder already knows; the first
  // pass asks it, and a scene naming no URL never runs the second.
  if (arena.urls.length > 0) {
    const wanted = [...new Set(arena.urls)]
    const fetched = new Map<string, Uint8Array>()
    await Promise.all(
      wanted.map(async url => {
        const caller = props.httpOptions?.signal
        const { ceiling, signal } = fetchDeadline(caller)

        let response: Response
        try {
          response = await fetch(url, { ...props.httpOptions, signal })
        } catch (cause) {
          // Ours or theirs is worth distinguishing: one is a limit this
          // renderer chose and the other is the caller's own abort, and a
          // reader who cannot tell them apart looks in the wrong place.
          if (ceiling.aborted && !(caller?.aborted ?? false)) {
            throw new TypeError(`cannot fetch ${JSON.stringify(url)}: it took longer than the ${FETCH_TIMEOUT_MS / 1000} seconds this renderer waits`, {
              cause,
            })
          }
          throw new TypeError(`cannot fetch ${JSON.stringify(url)}: ${String(cause)}`, { cause })
        }
        if (!response.ok) {
          throw new TypeError(`cannot fetch ${JSON.stringify(url)}: ${response.status} ${response.statusText}`)
        }
        fetched.set(url, await bounded(url, response))
      }),
    )
    arena = encodeScene(tree, props.width, height, contentHeight, scale, surface, fetched)
  }

  const native = dependencies.renderer.paint(arena.slots, arena.values, {
    fonts: props.fonts ?? [],
  })

  return new Canvas(native)
}
