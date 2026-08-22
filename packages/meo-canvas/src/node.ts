/**
 * The node factories, and the object shape they all produce.
 *
 * A factory is a plain function returning a plain object. **Nothing crosses into
 * native code here** — the whole tree is built in JavaScript and `Root` encodes
 * it in one pass, because JavaScript evaluates arguments inside out: writing
 * opcodes as each factory ran would land them post-order where the arena is
 * pre-order.
 *
 * ```ts
 * import { Column, Row, Text } from 'meo-canvas'
 *
 * const card = Row({
 *   style: { gap: 16, padding: 24, background: '#101014' },
 *   children: [Text('Ukasyah', { style: { fontSize: 24, fontWeight: 'bold' } })],
 * })
 * ```
 *
 * @packageDocumentation
 */

import type { Style } from './style.js'

/** What a node draws. */
export type NodeKind = 'box' | 'text' | 'image' | 'path'

/** Where an image's bytes come from. */
export type ImageSource = { readonly path: string } | { readonly url: string } | { readonly bytes: Uint8Array }

/** One run of a rich-text node, with a style of its own. */
export interface TextSegment {
  /** The run's text. */
  readonly text: string
  /**
   * Styling for this run alone, overriding the node's.
   *
   * `Style | undefined` rather than optional, so a segment has the same shape
   * whether it is styled or not — the same reason {@link SceneNode} carries
   * every key.
   */
  readonly style: Style | undefined
}

/**
 * One node of the tree.
 *
 * **Every field is present on every node**, `undefined` where it does not
 * apply, and always in this order. That is not tidiness: a node that sometimes
 * carries `src` and sometimes does not gives V8 two hidden classes for one
 * shape, and every property read in the encoder deoptimises to a megamorphic
 * lookup. One shape, one hidden class, and the encoder's reads stay inline.
 *
 * The factories are the only thing that should build one, for that reason —
 * an object literal written by hand is one key order away from a second class.
 */
export interface SceneNode {
  /** What this node draws. */
  readonly kind: NodeKind
  /** How it is styled, or `undefined` for a node that sets nothing. */
  readonly style: Style | undefined
  /** Its children, in paint order before `zIndex` applies. */
  readonly children: readonly SceneNode[] | undefined
  /** A name carried through for diagnostics, which the renderer never reads. */
  readonly name: string | undefined
  /** The runs of a text node. */
  readonly segments: readonly TextSegment[] | undefined
  /** Where an image node's bytes come from. */
  readonly src: ImageSource | undefined
  /** A path node's SVG `d` attribute. */
  readonly d: string | undefined
}

/**
 * Builds a node with every key present.
 *
 * The one place a `SceneNode` is constructed, so there is one key order in the
 * package rather than one per factory. A second literal elsewhere is a second
 * hidden class the moment its keys are written in another order.
 */
function node(
  kind: NodeKind,
  style: Style | undefined,
  children: readonly SceneNode[] | undefined,
  name: string | undefined,
  segments: readonly TextSegment[] | undefined,
  src: ImageSource | undefined,
  d: string | undefined,
): SceneNode {
  return { kind, style, children, name, segments, src, d }
}

/** What every container factory accepts. */
export interface ContainerProps {
  /** How the container is styled. */
  readonly style?: Style
  /** Its children, drawn in order. */
  readonly children?: readonly SceneNode[]
  /** A name carried through for diagnostics. */
  readonly name?: string
}

/**
 * A plain container.
 *
 * Lays its children out as a row, following CSS's `display: flex` rather than
 * Yoga's column.
 */
export function Box(props: ContainerProps = {}): SceneNode {
  return node('box', props.style, props.children, props.name, undefined, undefined, undefined)
}

/** A container whose children run horizontally. */
export function Row(props: ContainerProps = {}): SceneNode {
  return node('box', withDirection(props.style, 'row'), props.children, props.name, undefined, undefined, undefined)
}

/** A container whose children run vertically. */
export function Column(props: ContainerProps = {}): SceneNode {
  return node('box', withDirection(props.style, 'column'), props.children, props.name, undefined, undefined, undefined)
}

/** A container whose children are placed on a grid. */
export function Grid(props: ContainerProps = {}): SceneNode {
  const style: Style = props.style === undefined ? { display: 'grid' } : { display: 'grid', ...props.style }
  return node('box', style, props.children, props.name, undefined, undefined, undefined)
}

/**
 * The caller's style with a flex direction the factory names.
 *
 * The one place this package copies a style, and it copies at most once per
 * container rather than per property: `Row` and `Column` mean a direction, and
 * a caller who states one in `style` keeps it — spreading after the default is
 * what makes the caller's value win.
 */
function withDirection(style: Style | undefined, flexDirection: 'row' | 'column'): Style {
  return style === undefined ? { flexDirection } : { flexDirection, ...style }
}

/** What a text node accepts beyond its content. */
export interface TextProps {
  /** How the text is styled. */
  readonly style?: Style
  /** A name carried through for diagnostics. */
  readonly name?: string
}

/**
 * A run of text.
 *
 * The content is the first argument rather than a key, so it cannot be
 * forgotten — a `Text` with no text is not a thing worth being able to write.
 *
 * ```ts
 * import { Text } from 'meo-canvas'
 *
 * const name = Text('Ukasyah', { style: { fontSize: 24 } })
 * ```
 */
export function Text(content: string, props: TextProps = {}): SceneNode {
  return node('text', props.style, undefined, props.name, [{ text: content, style: undefined }], undefined, undefined)
}

/**
 * Text made of runs that differ in style.
 *
 * The one case a single string cannot express: a sentence with one word bold.
 * Each segment's own style overrides the node's for that run.
 */
export function RichText(segments: readonly TextSegment[], props: TextProps = {}): SceneNode {
  return node('text', props.style, undefined, props.name, segments, undefined, undefined)
}

/** What an image node accepts. */
export interface ImageProps {
  /**
   * Where the bytes come from.
   *
   * A bare string is a local path. A `{ url }` is fetched by the surface that
   * accepted it, never by the renderer.
   */
  readonly src: string | ImageSource
  /** How the image is styled. `fit` and `frame` are read here and nowhere else. */
  readonly style?: Style
  /** A name carried through for diagnostics. */
  readonly name?: string
}

/**
 * A raster image.
 *
 * ```ts
 * import { Image } from 'meo-canvas'
 *
 * const avatar = Image({ src: 'avatar.png', style: { width: 64, height: 64, fit: 'cover' } })
 * ```
 */
export function Image(props: ImageProps): SceneNode {
  const src = typeof props.src === 'string' ? { path: props.src } : props.src
  return node('image', props.style, undefined, props.name, undefined, src, undefined)
}

/** What a path node accepts. */
export interface PathProps {
  /** The SVG `d` attribute, in the node's own coordinates. */
  readonly d: string
  /** How the path is styled. */
  readonly style?: Style
  /** A name carried through for diagnostics. */
  readonly name?: string
}

/**
 * An arbitrary shape from SVG path data.
 *
 * ```ts
 * import { Path } from 'meo-canvas'
 *
 * const tick = Path({ d: 'M2 8 L6 12 L14 3' })
 * ```
 */
export function Path(props: PathProps): SceneNode {
  return node('path', props.style, undefined, props.name, undefined, undefined, props.d)
}

/**
 * The keys every node carries, in the order the factories write them.
 *
 * Exported so a test can assert the shape rather than trusting it, since the
 * cost of a second hidden class is invisible until something is profiled.
 */
export const NODE_KEYS: readonly string[] = ['kind', 'style', 'children', 'name', 'segments', 'src', 'd']
