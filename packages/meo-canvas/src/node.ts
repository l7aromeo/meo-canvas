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
 * import { Row, Text } from 'meo-canvas'
 *
 * const card = Row({
 *   gap: 16,
 *   padding: 24,
 *   backgroundColor: '#101014',
 *   children: [Text('Ukasyah', { fontSize: 24, fontWeight: 'bold' })],
 * })
 * ```
 *
 * The style properties sit directly in the props, as v1's `BoxProps` carries
 * them, rather than under a `style` key. The **stored** node keeps them in one
 * object because that shape is about hidden classes rather than about how a
 * caller writes it — two different questions.
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
  /**
   * A text node's paragraph properties, which are not style and do not inherit.
   *
   * Separate from {@link SceneNode.style} because the scene keeps them
   * separate: `maxLines` and `ellipsis` describe the block, not the glyphs, and
   * nothing inherits them from a parent.
   */
  readonly paragraph: ParagraphOptions | undefined
  /**
   * A text node's content as markup, to be parsed by the renderer.
   *
   * Set by {@link Text} and left `undefined` by {@link RichText}, which is how
   * the two are told apart on the wire: rich text of one run is otherwise
   * byte-identical to plain text of one run, and the decoder would have to
   * guess. Parse everything and `RichText` can no longer carry a literal `<`;
   * parse nothing and a caller gets no rich text at all.
   */
  readonly markup: string | undefined
  /** The runs of a text node, built by the caller and not interpreted. */
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
  paragraph: ParagraphOptions | undefined,
  markup: string | undefined,
  segments: readonly TextSegment[] | undefined,
  src: ImageSource | undefined,
  d: string | undefined,
): SceneNode {
  return { kind, style, children, name, paragraph, markup, segments, src, d }
}

/**
 * Anything that can sit inside a container.
 *
 * `false` and `undefined` are members so `condition && Text('…')` reads the way
 * it does in JSX and renders nothing when the condition fails. v1 allows both
 * for that reason, and a conditional written that way is how its users write
 * one — dropping it would break the idiom rather than tidy it.
 */
export type Child = SceneNode | false | undefined

/** One child, or many. */
export type Children = Child | readonly Child[]

/**
 * What every container factory accepts: its style, flat, plus its children.
 *
 * The style properties are the props, as v1 spells them. `children` and `name`
 * are not style properties and no style property is called either, so the
 * encoder — which looks up only the names in its own table — never reads them
 * and the props object is stored as the style without a copy.
 */
export type ContainerProps = Style & {
  /** Its children, drawn in order. A single child need not be wrapped. */
  readonly children?: Children
  /** A name carried through for diagnostics. */
  readonly name?: string
}

/**
 * The array every container with no surviving child shares.
 *
 * One frozen array rather than a fresh `[]` each time: a conditional subtree
 * that renders nothing is common, and none of them can tell the difference.
 */
const NO_CHILDREN: readonly SceneNode[] = Object.freeze([])

/**
 * The children a container actually has: one or many, with the falsy ones gone.
 *
 * Absent stays absent — a container that named no children has `undefined`,
 * not an empty array — and anything else becomes an array, because the node
 * field is one shape and the encoder should not have to ask which.
 *
 * The array a caller passed is handed straight through when every entry is a
 * node, so the common case allocates nothing. A filter runs only when there is
 * something to filter out.
 */
function toChildren(children: Children | undefined): readonly SceneNode[] | undefined {
  if (children === undefined) return undefined
  if (children === false) return NO_CHILDREN
  if (!Array.isArray(children)) return [children as SceneNode]

  const many = children as readonly Child[]
  const kept = many.every(child => child !== false && child !== undefined)
  if (kept) return many as readonly SceneNode[]
  return many.filter((child): child is SceneNode => child !== false && child !== undefined)
}

/**
 * A plain container.
 *
 * Lays its children out as a row, following CSS's `display: flex` rather than
 * Yoga's column.
 */
export function Box(props: ContainerProps = {}): SceneNode {
  return node('box', props, toChildren(props.children), props.name, undefined, undefined, undefined, undefined, undefined)
}

/** A container whose children run horizontally. */
export function Row(props: ContainerProps = {}): SceneNode {
  return node('box', withDirection(props, 'row'), toChildren(props.children), props.name, undefined, undefined, undefined, undefined, undefined)
}

/** A container whose children run vertically. */
export function Column(props: ContainerProps = {}): SceneNode {
  return node('box', withDirection(props, 'column'), toChildren(props.children), props.name, undefined, undefined, undefined, undefined, undefined)
}

/** A container whose children are placed on a grid. */
export function Grid(props: ContainerProps = {}): SceneNode {
  const style: Style = { display: 'grid', ...props }
  return node('box', style, toChildren(props.children), props.name, undefined, undefined, undefined, undefined, undefined)
}

/**
 * The caller's props with a flex direction the factory names.
 *
 * The one place this package copies a style, and it copies once per container
 * rather than once per property: `Row` and `Column` mean a direction, and a
 * caller who states one keeps it — spreading the props after the default is
 * what makes the caller's value win.
 */
function withDirection(props: ContainerProps, flexDirection: 'row' | 'column'): Style {
  return { flexDirection, ...props }
}

/**
 * Properties of a paragraph as a whole, which do not inherit.
 *
 * Held apart from {@link Style} because the scene holds them apart: these
 * describe the block of text, and a child cannot inherit them from a parent the
 * way it inherits a font size.
 */
export interface ParagraphOptions {
  /** How many lines to draw before the text is truncated. Unset draws them all. */
  readonly maxLines?: number
  /**
   * What a truncated last line ends with.
   *
   * Only read when {@link ParagraphOptions.maxLines} truncates something. Unset
   * truncates without a marker.
   */
  readonly ellipsis?: string
}

/** What a text node accepts beyond its content: its style, flat. */
export type TextProps = Style &
  ParagraphOptions & {
    /** A name carried through for diagnostics. */
    readonly name?: string
  }

/**
 * The paragraph properties of `props`, or `undefined` when it sets neither.
 *
 * Each key is added only when it has a value, rather than written as
 * `undefined`: `exactOptionalPropertyTypes` is on, and an explicit `undefined`
 * is a different thing from an absent key to every reader here.
 */
function paragraphOf(props: TextProps): ParagraphOptions | undefined {
  if (props.maxLines === undefined && props.ellipsis === undefined) return undefined
  const paragraph: { maxLines?: number; ellipsis?: string } = {}
  if (props.maxLines !== undefined) paragraph.maxLines = props.maxLines
  if (props.ellipsis !== undefined) paragraph.ellipsis = props.ellipsis
  return paragraph
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
 * const name = Text('Ukasyah', { fontSize: 24 })
 * ```
 */
export function Text(content: string, props: TextProps = {}): SceneNode {
  return node('text', props, undefined, props.name, paragraphOf(props), content, undefined, undefined, undefined)
}

/**
 * Text made of runs that differ in style.
 *
 * The one case a single string cannot express: a sentence with one word bold.
 * Each segment's own style overrides the node's for that run.
 */
export function RichText(segments: readonly TextSegment[], props: TextProps = {}): SceneNode {
  return node('text', props, undefined, props.name, paragraphOf(props), undefined, segments, undefined, undefined)
}

/** What an image node accepts: its source, and its style, flat. */
export type ImageProps = Style & {
  /**
   * Where the bytes come from.
   *
   * A bare string is a local path. A `{ url }` is fetched by the surface that
   * accepted it, never by the renderer.
   */
  readonly src: string | ImageSource
  /** A name carried through for diagnostics. */
  readonly name?: string
}

/**
 * A raster image.
 *
 * ```ts
 * import { Image } from 'meo-canvas'
 *
 * const avatar = Image({ src: 'avatar.png', width: 64, height: 64, objectFit: 'cover' })
 * ```
 */
export function Image(props: ImageProps): SceneNode {
  const src = typeof props.src === 'string' ? { path: props.src } : props.src
  return node('image', props, undefined, props.name, undefined, undefined, undefined, src, undefined)
}

/** What a path node accepts: its data, and its style, flat. */
export type PathProps = Style & {
  /** The SVG `d` attribute, in the node's own coordinates. */
  readonly d: string
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
  return node('path', props, undefined, props.name, undefined, undefined, undefined, undefined, props.d)
}

/**
 * The keys every node carries, in the order the factories write them.
 *
 * Exported so a test can assert the shape rather than trusting it, since the
 * cost of a second hidden class is invisible until something is profiled.
 */
export const NODE_KEYS: readonly string[] = ['kind', 'style', 'children', 'name', 'paragraph', 'markup', 'segments', 'src', 'd']
