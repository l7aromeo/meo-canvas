import { type CanvasRenderingContext2D, type CanvasGradient } from 'meo-skia-canvas'
import { drawBorders, drawRoundedRectPath, filterSpill, parseBorderRadius, parsePercentage, scaleFilterLengths, shadowSpill } from '@/canvas/canvas.helper.js'
import { createGradient } from '@/canvas/gradient.canvas.js'
import { paintBackgroundImage } from '@/canvas/background.canvas.js'
import { resolveCanvasImage, type RenderImageCache } from '@/canvas/image.loader.js'
import type { Image as CanvasImage } from 'meo-skia-canvas'
import { createCanvas, mirrorEngine } from '@/canvas/canvas.engine.js'
import { drawWithGradientMask, isGradientMask, maskFillRule, maskPath } from '@/canvas/mask.canvas.js'
import type { BaseProps, BoxProps, BoxShadowProps, CanvasElement } from '@/canvas/canvas.type.js'
import Yoga, { Style, Node } from '@/constant/common.const.js'

const _HEX_ALPHA_RE = /^#([0-9a-fA-F]{8})$/

/**
 * @class BoxNode
 * Base node class for rendering rectangular boxes with layout, styling, and children.
 * It uses the Yoga layout engine for positioning and sizing.
 */
/** A clipping ancestor, remembered so a node lifted past it is still cut by it. */
interface ClipFrame {
  node: BoxNode
  x: number
  y: number
  width: number
  height: number
  radii: { TopLeft: number; TopRight: number; BottomRight: number; BottomLeft: number }
  /** Whether this clipper is the containing block an `Absolute` node resolves against. */
  containsAbsolutes: boolean
  /** Whether it is the one a `Fixed` node resolves against, which only a transform or filter is. */
  containsFixed: boolean
}

/** One node in a stacking order, with where to paint it and what still clips it. */
interface StackingEntry {
  node: BoxNode
  zIndex: number
  order: number
  originX: number
  originY: number
  clips: ClipFrame[]
}

export class BoxNode {
  /**
   * Set where a layout places this node absolutely for its own reasons rather than because the
   * caller asked. `Grid` does it to put items on their tracks, and a node placed that way becomes a
   * containing block Yoga resolves against though nothing named a `positionType` -- CSS keeps a
   * grid item static, so the paint pass shifts its absolute descendants back out.
   */
  protected placedByLayout = false

  /** Original props passed to the constructor before any modifications. */
  initialProps: Partial<BoxProps>

  /** The Yoga layout engine node. */
  node: Node

  /** Child nodes. */
  children: BoxNode[]

  /** Current props including defaults and inherited values. */
  props: BoxProps & BaseProps

  /** Node type name. */
  readonly name?: string

  /** Unique node identifier. */
  key?: string

  /**
   * Creates a new BoxNode instance
   * @param props Initial box properties and styling
   */
  constructor(props: BoxProps & BaseProps = {}) {
    const children = (Array.isArray(props?.children) ? props.children : [props.children]).filter(child => child)
    this.initialProps = { ...props, children }
    this.node = Yoga.Node.create()
    this.children = []

    this.props = {
      key: this.key,
      borderColor: 'black',
      borderStyle: Style.Border.Solid,
      boxSizing: Style.BoxSizing.BorderBox,
      opacity: 1,
      flexShrink: 1,
      ...this.initialProps,
    }
    this.name = this.props.name || 'Box'
    this.key = this.props.key || `${this.name}-0`

    this.setLayout(this.props)
  }

  /**
   * Processes and appends any children passed in the initial props.
   */
  public processInitialChildren() {
    if (this.props.children) {
      const childrenToAdd = Array.isArray(this.props.children) ? this.props.children : [this.props.children]
      childrenToAdd.forEach((child, index) => {
        if (child) {
          this.appendChild(child as BoxNode, index)
        }
      })
    }
  }

  /**
   * Inherits styles from the parent node.
   * @param {BoxProps & BaseProps} parentProps Parent node properties to inherit from.
   */
  protected resolveInheritedStyles(parentProps: BoxProps & BaseProps) {
    if (parentProps.key) {
      this.key = `${parentProps.key}-${this.key}`
      this.props.key = this.key
    }

    const inheritableKeys = [
      'fontSize',
      'fontFamily',
      'fontWeight',
      'fontStyle',
      'color',
      'textAlign',
      'verticalAlign',
      'lineHeight',
      'lineGap',
      'letterSpacing',
      'wordSpacing',
      'textDecoration',
      'maxLines',
      'fontVariant',
    ] as const

    const initialPropsRec = this.initialProps as Record<string, unknown>
    const parentPropsRec = parentProps as Record<string, unknown>
    const propsRec = this.props as Record<string, unknown>
    for (const key of inheritableKeys) {
      if (initialPropsRec[key] === undefined && parentPropsRec[key] !== undefined) {
        propsRec[key] = parentPropsRec[key]
      }
    }

    if (!this.node.isDirty()) {
      this.node.markDirty()
    }
  }

  /**
   * Applies node type-specific default values after inheritance.
   */
  protected applyDefaults(): void {
    // Base implementation does nothing; subclasses can override.
  }

  /**
   * Appends a child node at the specified index.
   * @param {BoxNode} child Child node to append.
   * @param index Index to insert child at
   */
  protected appendChild(child: BoxNode, index: number) {
    if (!child || !child.node) {
      console.warn('Attempted to append an invalid child node.', child)
      return
    }

    const { children: _c, ...inheritedProps } = this.props
    child.resolveInheritedStyles(inheritedProps)
    child.applyDefaults()
    this.children.push(child)
    this.node.insertChild(child.node, index)
    child.processInitialChildren()
  }

  /**
   * Performs final layout adjustments recursively after the main layout calculation.
   * @returns {boolean} Whether any node was marked as dirty during finalization.
   */
  public finalizeLayout(): boolean {
    let wasDirty = false
    this.updateLayoutBasedOnComputedSize()
    if (this.node.isDirty()) {
      wasDirty = true
    }

    for (const child of this.children) {
      child.finalizeLayout()
      if (child.node.isDirty()) {
        wasDirty = true
      }
    }

    return wasDirty
  }

  /**
   * Hook for subclasses to update layout based on computed size.
   */
  protected updateLayoutBasedOnComputedSize() {
    // Base implementation does nothing; subclasses can override.
  }

  /**
   * Applies layout properties to the Yoga node.
   * @param props Box properties containing layout values
   */
  protected setLayout(props: BoxProps) {
    // --- Yoga layout property application ---
    // (This entire block remains unchanged as it interacts with Yoga, not the canvas library)
    const {
      width,
      height,
      minWidth,
      minHeight,
      maxWidth,
      maxHeight,
      flexDirection,
      justifyContent,
      alignItems,
      alignSelf,
      alignContent,
      flexGrow,
      flexShrink,
      flexBasis,
      positionType,
      position,
      gap,
      margin,
      padding,
      border,
      aspectRatio,
      overflow,
      display,
      boxSizing = Style.BoxSizing.BorderBox,
      // Inherited rather than defaulted to `LTR`: defaulting meant every node was given an
      // explicit direction, so a page set to `RTL` never reached its children and nothing reversed.
      direction = Style.Direction.Inherit,
      flexWrap,
    } = props

    if (width !== undefined) this.node.setWidth(width)
    if (height !== undefined) this.node.setHeight(height)
    if (minWidth !== undefined) this.node.setMinWidth(minWidth)
    if (minHeight !== undefined) this.node.setMinHeight(minHeight)
    if (maxWidth !== undefined) this.node.setMaxWidth(maxWidth)
    if (maxHeight !== undefined) this.node.setMaxHeight(maxHeight)
    if (flexDirection !== undefined) this.node.setFlexDirection(flexDirection)
    if (justifyContent !== undefined) this.node.setJustifyContent(justifyContent)
    if (alignItems !== undefined) this.node.setAlignItems(alignItems)
    if (alignSelf !== undefined) this.node.setAlignSelf(alignSelf)
    if (alignContent !== undefined) this.node.setAlignContent(alignContent)
    if (flexGrow !== undefined) this.node.setFlexGrow(flexGrow)
    if (flexShrink !== undefined) this.node.setFlexShrink(flexShrink)
    // Defaulted to `Static` rather than left at Yoga's own default of `Relative`, because the two
    // disagree about containing blocks and CSS is on `Static`'s side: an absolutely positioned node
    // resolves against its nearest *positioned* ancestor, skipping every static box between. Yoga
    // offers `Static` for exactly that, and leaving the default in place made every ancestor a
    // containing block -- so an absolute node landed against its immediate parent wherever CSS
    // would have gone further up.
    // Yoga has no `Fixed`, so it is laid out as `Absolute` and moved to its own containing block
    // when it is painted -- see `collectStacking`.
    // A sticky node stays in the flow, so Yoga lays it out as `Relative`; its insets are a
    // constraint on where it may sit rather than an offset, and are applied when it is painted.
    const sticky = positionType === Style.PositionType.Sticky
    type YogaPosition = Parameters<typeof this.node.setPositionType>[0]
    const yogaPosition =
      positionType === Style.PositionType.Fixed
        ? Style.PositionType.Absolute
        : sticky
          ? Style.PositionType.Relative
          : (positionType ?? Style.PositionType.Static)
    this.node.setPositionType(yogaPosition as YogaPosition)
    if (flexBasis !== undefined) this.node.setFlexBasis(flexBasis)
    // `position: 0` is falsy and meaningful: an absolute node inset by nothing on all four sides
    // fills its parent.
    if (position !== undefined && !sticky) {
      if (typeof position === 'number') {
        this.node.setPosition(Style.Edge.All, position)
      } else if (typeof position === 'string' && position.endsWith('%')) {
        this.node.setPositionPercent(Style.Edge.All, parseFloat(position))
      } else {
        for (const [edge, value] of Object.entries(position)) {
          if (edge in Style.Edge) {
            const edgeKey = edge as keyof typeof Style.Edge
            if (typeof value === 'string' && value.endsWith('%')) {
              this.node.setPositionPercent(Style.Edge[edgeKey], parseFloat(value))
            } else {
              this.node.setPosition(Style.Edge[edgeKey], value as number)
            }
          }
        }
      }
    }
    if (gap)
      _setEdgeValues(
        gap,
        Style.Gutter,
        (e: any, v: any) => this.node.setGap(e, v),
        (e: any, v: any) => this.node.setGapPercent(e, v),
      )
    if (margin)
      _setEdgeValues(
        margin,
        Style.Edge,
        (e: any, v: any) => this.node.setMargin(e, v),
        (e: any, v: any) => this.node.setMarginPercent(e, v),
        true,
      )
    if (padding)
      _setEdgeValues(
        padding,
        Style.Edge,
        (e: any, v: any) => this.node.setPadding(e, v),
        (e: any, v: any) => this.node.setPaddingPercent(e, v),
      )
    if (border) {
      if (typeof border === 'number') {
        this.node.setBorder(Style.Edge.All, border)
      } else {
        for (const [edge, value] of Object.entries(border)) {
          if (edge in Style.Edge) this.node.setBorder(Style.Edge[edge as keyof typeof Style.Edge], value)
        }
      }
    }
    if (aspectRatio !== undefined) this.node.setAspectRatio(aspectRatio)
    if (overflow !== undefined) this.node.setOverflow(overflow)
    if (display !== undefined) this.node.setDisplay(display)
    if (boxSizing !== undefined) this.node.setBoxSizing(boxSizing)
    if (direction !== undefined) this.node.setDirection(direction)
    if (flexWrap !== undefined) this.node.setFlexWrap(flexWrap)
    // --- End Yoga layout property application ---
  }

  /**
   * Draws this node, through its `mask` when it has one.
   *
   * Every component's drawing arrives here — `Text`, `Image`, `Chart` and `Grid` override
   * `_renderContent` rather than this — so masking one node type means masking all of them.
   *
   * The two kinds of mask are applied differently because they are different operations. A shape or
   * path clips, which is a yes-or-no test per pixel and costs nothing but a `save`/`restore`. A
   * gradient cannot: its whole purpose is the answers in between, so the node is composited through
   * one instead. Both are applied to the node's layout box, before its own `transform`, which is
   * what keeps the two consistent with each other.
   *
   * `dither` is held on the context around all of it, so the node's mask, background, content and
   * children are drawn with one answer and whatever draws next is left with its own.
   */
  async render(ctx: CanvasRenderingContext2D, offsetX: number = 0, offsetY: number = 0, suppressStacking: boolean = false) {
    const { dither } = this.props
    if (dither === undefined) return this.renderMasked(ctx, offsetX, offsetY, suppressStacking)

    // Put back by hand rather than through `save`/`restore`, which would clone the whole graphics
    // state to carry one boolean. Putting it back at all is what keeps the node that draws next
    // reading its own ancestor's answer rather than this one's, and a node that says nothing
    // inherits by leaving the context alone.
    const inherited = ctx.dither
    ctx.dither = dither
    try {
      return await this.renderMasked(ctx, offsetX, offsetY, suppressStacking)
    } finally {
      ctx.dither = inherited
    }
  }

  /** {@link render} without the dither state around it: the mask, then the node itself. */
  private async renderMasked(ctx: CanvasRenderingContext2D, offsetX: number, offsetY: number, suppressStacking: boolean = false) {
    const mask = this.props.mask
    if (!mask) return this.renderNode(ctx, offsetX, offsetY, false, suppressStacking)

    const layout = this.node.getComputedLayout()
    const box = { x: layout.left + offsetX, y: layout.top + offsetY, width: layout.width, height: layout.height }
    if (box.width <= 0 || box.height <= 0) return

    if (isGradientMask(mask)) {
      const drawn = await drawWithGradientMask(
        ctx,
        mask.gradient,
        box,
        target => this.renderNode(target, offsetX, offsetY, false, suppressStacking),
        `[BoxNode ${this.key}]`,
      )
      // A gradient that could not be built is not a reason to lose the node; it draws unmasked,
      // having already said why.
      return drawn ? undefined : this.renderNode(ctx, offsetX, offsetY, false, suppressStacking)
    }

    const path = maskPath(mask, box)
    if (!path) return this.renderNode(ctx, offsetX, offsetY, false, suppressStacking)

    ctx.save()
    try {
      ctx.clip(path, maskFillRule(mask))
      await this.renderNode(ctx, offsetX, offsetY, false, suppressStacking)
    } finally {
      ctx.restore()
    }
  }

  /**
   * Redraws what is behind the node through a filter, clipped to the node's own box.
   *
   * CSS filters the backdrop where the element sits, corners included, and then paints the
   * element's background on top of the result. There is no way to filter pixels already on a
   * canvas in place, so the canvas is copied and the copy drawn back through the clip.
   *
   * The clip is set while the node's transform is still in force and therefore survives the
   * transform being reset, which is what lets the copy be drawn back pixel for pixel — a rotated
   * or scaled node filters the region it actually covers rather than an upright box near it.
   *
   * Only the node's own region is copied, grown by how far the filter reaches in from outside it —
   * a blur pulls in colour from past the edges, and cutting the copy at the box would leave a seam
   * there. Copying the whole surface instead made a backdrop cost the page's area rather than the
   * node's, which is the difference between 4ms and 33ms a node as a page grows.
   *
   * `target` and `source` come apart when the node also carries a `filter` or a blend: the subtree
   * is drawn into an offscreen, so the backdrop has to be read from the page and painted into that
   * offscreen instead — `deviceOffset` is where the page's origin falls in it. Painted onto the
   * page directly in that case, the backdrop would sit under the offscreen and be covered by it;
   * read from the offscreen, there would be no backdrop at all, since nothing has been drawn there
   * yet. CSS puts the backdrop inside the filtered picture, so a greyscaled node greys what shows
   * through it.
   */
  private applyBackdropFilter(
    target: CanvasRenderingContext2D,
    source: CanvasRenderingContext2D,
    filter: string,
    x: number,
    y: number,
    width: number,
    height: number,
    deviceOffsetX: number = 0,
    deviceOffsetY: number = 0,
  ) {
    const surface = source.canvas
    if (!surface?.width || !surface?.height) return

    const matrix = target.getTransform()
    const pageScale = (Math.hypot(matrix.a, matrix.b) + Math.hypot(matrix.c, matrix.d)) / 2 || 1
    const scaled = scaleFilterLengths(filter, pageScale)

    // Only the part of the page the node can actually show, grown by how far the filter reaches in
    // from beyond the edges. Copying the whole surface instead costs the page's area per backdrop
    // node rather than the node's own: on a 15 Mpx page that was 33ms a node against 4ms on a
    // small one, for a node covering a hundredth of it either way.
    const reach = filterSpill(scaled)
    const corners = [
      [x, y],
      [x + width, y],
      [x, y + height],
      [x + width, y + height],
    ].map(([cornerX, cornerY]) => ({
      x: matrix.a * cornerX + matrix.c * cornerY + matrix.e - deviceOffsetX,
      y: matrix.b * cornerX + matrix.d * cornerY + matrix.f - deviceOffsetY,
    }))

    const left = Math.max(0, Math.floor(Math.min(...corners.map(corner => corner.x)) - reach))
    const top = Math.max(0, Math.floor(Math.min(...corners.map(corner => corner.y)) - reach))
    const right = Math.min(surface.width, Math.ceil(Math.max(...corners.map(corner => corner.x)) + reach))
    const bottom = Math.min(surface.height, Math.ceil(Math.max(...corners.map(corner => corner.y)) + reach))

    const regionWidth = right - left
    const regionHeight = bottom - top
    if (regionWidth <= 0 || regionHeight <= 0) return

    const snapshot = createCanvas(regionWidth, regionHeight, mirrorEngine(source))
    snapshot.getContext('2d').drawImage(surface, left, top, regionWidth, regionHeight, 0, 0, regionWidth, regionHeight)

    target.save()
    try {
      drawRoundedRectPath(target, x, y, width, height, parseBorderRadius(this.props.borderRadius))
      target.clip()
      target.resetTransform()
      target.filter = scaled
      target.drawImage(snapshot, left + deviceOffsetX, top + deviceOffsetY)
    } finally {
      target.restore()
    }
  }

  /** The decoded background picture, once the render's load pass has fetched it. */
  protected backgroundBitmap: CanvasImage | null = null

  /**
   * Fetches this node's background picture, if it has one.
   *
   * Runs in the same pass as the images, before layout, and through the same cache — so a picture
   * used as one node's background and another's image is fetched once. A failure leaves the node
   * without a picture rather than failing the render, which is what a missing background should do.
   */
  public async loadBackgroundImage(cache?: RenderImageCache, diskCacheKeys?: Set<string>): Promise<void> {
    const background = this.props.backgroundImage
    if (!background?.src) return

    try {
      this.backgroundBitmap = await resolveCanvasImage(
        { src: background.src, color: background.color, httpOptions: background.httpOptions },
        cache,
        diskCacheKeys,
      )
    } catch (error) {
      console.warn(`[BoxNode ${this.key}] Background image failed to load:`, error)
      this.backgroundBitmap = null
    }
  }

  /**
   * The CSS filter chain this node draws through, or an empty string for none.
   *
   * `saturate` came first and stays a shorthand for the same machinery, so it leads the chain and
   * `filter` follows — the order they would appear in if the shorthand were written out.
   */
  protected filterChain(): string {
    const parts: string[] = []
    const saturate = (this.props as { saturate?: number }).saturate
    if (saturate !== undefined && saturate !== 1) parts.push(`saturate(${saturate})`)
    if (this.props.filter) parts.push(this.props.filter.trim())
    return parts.join(' ').trim()
  }

  /**
   * The blend mode this node composites with, or an empty string for the ordinary source-over.
   *
   * `normal` is the default and means exactly source-over, so it is not worth an offscreen.
   */
  protected blendMode(): string {
    const mode = this.props.mixBlendMode
    return !mode || mode === Style.BlendMode.Normal ? '' : mode
  }

  /**
   * How far this node's own drawing reaches past its box, beyond what its box model accounts for.
   *
   * Zero for a box, which paints within its bounds; {@link TextNode} overrides it, since a line too
   * long for its box still draws unless something clips it. Used to size a group's offscreen, where
   * anything unaccounted for is cut off at a hard edge.
   */
  protected inkOverflow(): number {
    return 0
  }

  /**
   * How far anything in this node's subtree reaches past its own box, in pixels.
   *
   * The group offscreen is sized from this. CSS only clips a subtree under `overflow: hidden`, so a
   * child laid out or translated beyond its parent, or casting its own shadow, still draws — and an
   * offscreen cut to the parent's box would lose it. Each descendant contributes its own box, its
   * transform, and whatever its shadows and filters spill.
   *
   * The result is one symmetric distance rather than four. A rectangle per side would allocate less
   * for a node whose subtree escapes one way, at the cost of carrying the asymmetry through the
   * offscreen's origin and the composite; that is worth doing only if the offscreen's size is ever
   * measured to matter.
   */
  private subtreeReach(originX: number, originY: number, width: number, height: number): number {
    let reach = 0

    const walk = (node: BoxNode, parentX: number, parentY: number) => {
      const layout = node.node.getComputedLayout()
      let left = parentX + layout.left
      let top = parentY + layout.top
      let right = left + layout.width
      let bottom = top + layout.height

      const transform = node.props.transform
      if (transform) {
        // Translation moves the box; a scale grows it about its centre. Rotation is taken as the
        // box's diagonal, which is the most any rotation of it can reach.
        const scaleX = transform.scaleX ?? transform.scale ?? 1
        const scaleY = transform.scaleY ?? transform.scale ?? 1
        const centreX = (left + right) / 2
        const centreY = (top + bottom) / 2
        let halfWidth = ((right - left) * Math.abs(scaleX)) / 2
        let halfHeight = ((bottom - top) * Math.abs(scaleY)) / 2

        if (transform.rotate) {
          const diagonal = Math.hypot(halfWidth, halfHeight)
          halfWidth = diagonal
          halfHeight = diagonal
        }

        // The translation is written inside the scale and the rotation, so it carries their
        // magnitude: 30 under `scale: 2` reaches 60. A rotation turns it any way at all, so its
        // whole length is allowed for in both axes rather than being resolved along one.
        const translateX = (typeof transform.translateX === 'number' ? transform.translateX : 0) * scaleX
        const translateY = (typeof transform.translateY === 'number' ? transform.translateY : 0) * scaleY
        if (transform.rotate) {
          const span = Math.hypot(translateX, translateY)
          halfWidth += span
          halfHeight += span
          left = centreX - halfWidth
          right = centreX + halfWidth
          top = centreY - halfHeight
          bottom = centreY + halfHeight
        } else {
          left = centreX - halfWidth + translateX
          right = centreX + halfWidth + translateX
          top = centreY - halfHeight + translateY
          bottom = centreY + halfHeight + translateY
        }
      }

      const spill = shadowSpill(node.props.boxShadow) + filterSpill(node.filterChain()) + node.inkOverflow()

      reach = Math.max(
        reach,
        originX - (left - spill),
        top !== undefined ? originY - (top - spill) : 0,
        right + spill - (originX + width),
        bottom + spill - (originY + height),
      )

      for (const child of node.children) walk(child, parentX + layout.left, parentY + layout.top)
    }

    for (const child of this.children) walk(child, originX, originY)

    return Math.max(0, Math.ceil(reach))
  }

  /**
   * Draws the subtree into an offscreen and composites it back in one go.
   *
   * The offscreen is built at device resolution — the transform in force is read off the context
   * and reproduced — so a filtered node on a `scale: 2` root is not drawn at half size and
   * enlarged. It is also grown by however far the chain's blurs and drop shadows reach, since CSS
   * lets a filter spill past the box rather than clipping to it.
   */
  private async renderAsGroup(
    ctx: CanvasRenderingContext2D,
    filter: string,
    blend: string,
    x: number,
    y: number,
    width: number,
    height: number,
    offsetX: number,
    offsetY: number,
  ) {
    // The offscreen has to hold everything the subtree paints, not just this node's box. A filter
    // spills, an outset shadow spills, and a child can be laid out or translated clean outside its
    // parent — CSS clips none of that unless `overflow: hidden` says so, and an offscreen cut to
    // the box turns each of them into a hard edge.
    const pad = filterSpill(filter) + shadowSpill(this.props.boxShadow) + this.subtreeReach(x, y, width, height)
    const matrix = ctx.getTransform()
    // Magnitudes rather than `a` and `d`: an ancestor's rotation puts the scale across both terms.
    const scaleX = Math.hypot(matrix.a, matrix.b) || 1
    const scaleY = Math.hypot(matrix.c, matrix.d) || 1

    const boxWidth = width + pad * 2
    const boxHeight = height + pad * 2
    const pixelWidth = Math.max(1, Math.ceil(boxWidth * scaleX))
    const pixelHeight = Math.max(1, Math.ceil(boxHeight * scaleY))

    const offscreen = createCanvas(pixelWidth, pixelHeight, mirrorEngine(ctx))
    const offCtx = offscreen.getContext('2d')
    offCtx.scale(scaleX, scaleY)
    offCtx.translate(-(x - pad), -(y - pad))

    // The backdrop belongs inside the group: CSS filters and blends a node's picture with what
    // shows through it included, so it is read from the page and painted into the offscreen first.
    if (this.props.backdropFilter) {
      this.applyBackdropFilter(offCtx, ctx, this.props.backdropFilter, x, y, width, height, -(x - pad) * scaleX, -(y - pad) * scaleY)
    }

    const desiredOpacity = Math.max(0, Math.min(1, this.props.opacity ?? 1))
    await this.renderNode(offCtx, offsetX, offsetY, true)

    ctx.save()
    try {
      if (desiredOpacity < 1) ctx.globalAlpha = desiredOpacity
      if (filter) ctx.filter = scaleFilterLengths(filter, (scaleX + scaleY) / 2)
      if (blend) ctx.globalCompositeOperation = blend as CanvasRenderingContext2D['globalCompositeOperation']
      ctx.drawImage(offscreen, x - pad, y - pad, boxWidth, boxHeight)
    } finally {
      ctx.restore()
    }
  }

  /**
   * Draws the node and everything inside it.
   *
   * `groupEffectsApplied` is set on the recursive call this method makes into an offscreen while
   * applying a filter: opacity and the filter itself belong to the group as a whole and have
   * already been dealt with by the caller, so the inner pass draws the subtree plainly.
   */
  /** A clipper a lifted node passed through on its way up, with where it was painted. */
  protected clipFrame(originX: number, originY: number): ClipFrame | null {
    if (this.props.overflow !== Style.Overflow.Hidden) return null
    const layout = this.node.getComputedLayout()
    if (!(layout.width > 0 || layout.height > 0)) return null
    // The padding box, not the border box: CSS clips content inside the border, and the corner
    // radius shrinks by the border it sits behind.
    const borderLeft = this.node.getComputedBorder(Style.Edge.Left)
    const borderTop = this.node.getComputedBorder(Style.Edge.Top)
    const borderRight = this.node.getComputedBorder(Style.Edge.Right)
    const borderBottom = this.node.getComputedBorder(Style.Edge.Bottom)
    const outer = parseBorderRadius(this.props.borderRadius)
    return {
      node: this,
      x: originX + borderLeft,
      y: originY + borderTop,
      width: Math.max(0, layout.width - borderLeft - borderRight),
      height: Math.max(0, layout.height - borderTop - borderBottom),
      radii: {
        TopLeft: Math.max(0, outer.TopLeft - Math.max(borderLeft, borderTop)),
        TopRight: Math.max(0, outer.TopRight - Math.max(borderRight, borderTop)),
        BottomRight: Math.max(0, outer.BottomRight - Math.max(borderRight, borderBottom)),
        BottomLeft: Math.max(0, outer.BottomLeft - Math.max(borderLeft, borderBottom)),
      },
      // An absolute node is cut by a clipper only where that clipper is its containing block.
      containsAbsolutes: this.props.positionType !== undefined && this.props.positionType !== Style.PositionType.Static,
      containsFixed: this.capturesFixed(),
    }
  }

  /**
   * The direction this node ends up laid out in.
   *
   * Yoga resolves `Inherit` while it lays out but keeps no record of the answer: `getDirection`
   * hands back what was set, and there is no computed counterpart. So the chain is walked to the
   * first ancestor that named one, which is what Yoga itself would have found.
   */
  protected resolvedDirection(): ReturnType<typeof this.node.getDirection> {
    let node: ReturnType<typeof this.node.getParent> | typeof this.node = this.node
    while (node) {
      const own = node.getDirection()
      if (own !== Style.Direction.Inherit) return own
      node = node.getParent()
    }
    return Style.Direction.LTR
  }

  /**
   * Whether this node takes part in its parent's stacking order rather than painting in the flow.
   *
   * Naming a `zIndex` is enough on its own: every node here is a flex item, and CSS applies
   * `z-index` to a flex item whatever its `position` says. Naming a `positionType` other than
   * `Static` is the other way in, since a positioned box paints above in-flow content.
   */
  private stacksAmongSiblings(): boolean {
    const positioned = this.props.positionType !== undefined && this.props.positionType !== Style.PositionType.Static
    return positioned || this.props.zIndex !== undefined
  }

  /**
   * Whether this node forms a stacking context, which is what stops a descendant escaping it.
   *
   * CSS forms one for a non-auto `z-index`, and for anything that composites the subtree as a
   * single picture -- opacity below 1, a transform, a filter, a blend mode, a mask. A positioned
   * box whose `z-index` is `auto` forms none, which is why its positioned descendants compete in
   * the nearest ancestor's context rather than being trapped inside it.
   */

  /**
   * Whether this node is the box a `Fixed` descendant resolves against. CSS: a transform or a
   * filter captures one, and nothing else between it and the page does.
   */

  /**
   * How far a sticky node is pushed off its flow position by its own insets.
   *
   * The insets are a constraint rather than an offset: the node moves only where the flow would
   * put it nearer an edge of the scrollport than the inset allows. Nothing scrolls here, so this is
   * the whole of what sticky does -- and it is what Chrome does with no scrolling ancestor too.
   */
  protected stickyShift(flowX: number, flowY: number, port: { x: number; y: number; width: number; height: number }) {
    const layout = this.node.getComputedLayout()
    const edge = (value: unknown, extent: number): number | undefined => {
      if (typeof value === 'number') return value
      if (typeof value === 'string' && value.endsWith('%')) return (parseFloat(value) / 100) * extent
      return undefined
    }
    const position = this.props.position
    const named = typeof position === 'object' && position !== null ? (position as Record<string, unknown>) : {}
    const all = typeof position === 'number' || typeof position === 'string' ? position : undefined

    let x = flowX
    let y = flowY
    const left = edge(named.Left ?? all, port.width)
    const top = edge(named.Top ?? all, port.height)
    const right = edge(named.Right ?? all, port.width)
    const bottom = edge(named.Bottom ?? all, port.height)
    if (left !== undefined) x = Math.max(x, port.x + left)
    if (top !== undefined) y = Math.max(y, port.y + top)
    if (right !== undefined) x = Math.min(x, port.x + port.width - right - layout.width)
    if (bottom !== undefined) y = Math.min(y, port.y + port.height - bottom - layout.height)
    return { x: x - flowX, y: y - flowY }
  }

  private capturesFixed(): boolean {
    return Boolean(this.props.transform || this.props.filter || this.props.backdropFilter)
  }

  private createsStackingContext(): boolean {
    if (this.props.zIndex !== undefined) return true
    // CSS forms one for sticky whatever its z-index says, unlike relative and absolute.
    if (this.props.positionType === Style.PositionType.Sticky) return true
    if ((this.props.opacity ?? 1) < 1) return true
    if (this.props.transform) return true
    if (this.props.filter || this.props.backdropFilter) return true
    if (this.props.mixBlendMode) return true
    if (this.props.mask) return true
    return false
  }

  /**
   * The nodes taking part in this node's stacking order, including any lifted out of a descendant.
   *
   * A child forming no stacking context does not contain its own positioned descendants: they
   * belong to the nearest ancestor that does, which may be this node. Each entry carries the origin
   * its own parent was painted at, so a lifted node still lands where its parent put it, and the
   * `lifted` set tells that parent not to paint it a second time.
   */
  private collectStacking(
    node: BoxNode,
    originX: number,
    originY: number,
    into: StackingEntry[],
    lifted: Set<BoxNode>,
    depth = 0,
    clips: ClipFrame[] = [],
    positionedOrigin: { x: number; y: number } = { x: originX, y: originY },
    fixedOrigin: { x: number; y: number } = { x: originX, y: originY },
    // Where Yoga actually resolved an absolute node from, which is not always the box CSS would
    // use: `Grid` places its items by making them absolute in Yoga, and that makes each one a
    // containing block Yoga will resolve against though nothing asked it to be one.
    yogaOrigin: { x: number; y: number } = { x: originX, y: originY },
    // Nothing scrolls, so the scrollport a sticky node is held inside is the page. Taken from the
    // outermost node this walk starts at, which is that page.
    port: { x: number; y: number; width: number; height: number } = {
      x: originX,
      y: originY,
      width: node.node.getComputedLayout().width,
      height: node.node.getComputedLayout().height,
    },
  ): void {
    node.children.forEach((child, index) => {
      if (child.stacksAmongSiblings()) {
        // A fixed node is laid out against the nearest positioned ancestor, because that is all
        // Yoga can do. Shifting the origin by the distance between that box and its real containing
        // block lands it where CSS puts it, without re-resolving its insets by hand.
        // Both kinds are placed by shifting from where Yoga put them to where CSS wants them: an
        // absolute node to its nearest positioned ancestor, a fixed one to the box that captures it.
        const fixed = child.props.positionType === Style.PositionType.Fixed
        const absolute = child.props.positionType === Style.PositionType.Absolute
        const target = fixed ? fixedOrigin : positionedOrigin
        let shiftX = fixed || absolute ? target.x - yogaOrigin.x : 0
        let shiftY = fixed || absolute ? target.y - yogaOrigin.y : 0
        if (child.props.positionType === Style.PositionType.Sticky) {
          const own = child.node.getComputedLayout()
          const held = child.stickyShift(originX + own.left, originY + own.top, port)
          shiftX = held.x
          shiftY = held.y
        }
        into.push({
          node: child,
          // `z-index: auto` shares a layer with `0`, so an absent index sorts as 0 rather than as a
          // tier of its own.
          zIndex: child.props.zIndex ?? 0,
          // Depth first, so a lifted node ties after the siblings it was lifted past.
          order: depth * 1_000_000 + index,
          originX: originX + shiftX,
          originY: originY + shiftY,
          // Escaping a stacking context does not escape a clip: the two are independent, and a
          // node lifted out of its parent is still cut by whatever clipped it on the way up.
          clips,
        })
        if (depth > 0) lifted.add(child)
      }
      if (!child.createsStackingContext()) {
        const layout = child.node.getComputedLayout()
        const childX = originX + layout.left
        const childY = originY + layout.top
        const nested = child.clipFrame(childX, childY)
        const childPositioned = child.props.positionType !== undefined && child.props.positionType !== Style.PositionType.Static
        this.collectStacking(
          child,
          childX,
          childY,
          into,
          lifted,
          depth + 1,
          nested ? [...clips, nested] : clips,
          // The box a `Absolute` descendant resolves against, and the one a `Fixed` one does. They
          // part company at a transform or a filter, which captures `Fixed` and nothing else.
          childPositioned ? { x: childX, y: childY } : positionedOrigin,
          child.capturesFixed() ? { x: childX, y: childY } : fixedOrigin,
          childPositioned || child.placedByLayout ? { x: childX, y: childY } : yogaOrigin,
          port,
        )
      }
    })
  }

  private async renderNode(
    ctx: CanvasRenderingContext2D,
    offsetX: number = 0,
    offsetY: number = 0,
    groupEffectsApplied: boolean = false,
    suppressStacking: boolean = false,
  ) {
    const layout = this.node.getComputedLayout()
    const x = layout.left + offsetX
    const y = layout.top + offsetY
    const width = layout.width
    const height = layout.height

    // Exit early if the node is invisible or has no dimensions.
    if (width <= 0 || height <= 0 || this.props.display === Style.Display.None) {
      return
    }

    // --- Filter Setup ---
    //
    // CSS applies a filter to the element and its descendants as one picture: the subtree is drawn,
    // then the chain is applied to the result. Setting `ctx.filter` and drawing normally would
    // filter every draw on its own, and two overlapping children would come out filtered twice —
    // the same mistake `opacity` used to make with `globalAlpha`.
    //
    // Opacity stays outside this, because CSS fades the filtered result rather than filtering a
    // faded one.
    //
    // A blend mode needs the same treatment for the same reason: CSS blends the element as one
    // picture with what is behind it, so the subtree is composited into the offscreen first and the
    // blend applied to the result rather than to each draw inside it.
    const filter = groupEffectsApplied ? '' : this.filterChain()
    const blend = groupEffectsApplied ? '' : this.blendMode()
    if (filter || blend) {
      await this.renderAsGroup(ctx, filter, blend, x, y, width, height, offsetX, offsetY)
      return
    }
    // --- End Filter Setup ---

    // --- Opacity Setup ---
    //
    // A layer, not `globalAlpha`. CSS composites the whole subtree once and fades the result, so
    // two overlapping children inside a half-transparent parent are as dark as one of them is.
    // Setting `globalAlpha` fades every draw on its own instead, and the overlap comes out twice as
    // opaque: two 50% reds on white read rgb(255,63,63) where CSS gives rgb(255,127,127).
    //
    // No bounds are passed: they would clip the layer, and a node's drawing reaches past its box
    // through shadows, transforms and text allowed to overflow.
    const desiredOpacity = groupEffectsApplied ? 1 : Math.max(0, Math.min(1, this.props.opacity ?? 1))
    let appliedOpacity = false
    if (desiredOpacity < 1) {
      ctx.saveLayer(desiredOpacity)
      appliedOpacity = true
    }
    // --- End Opacity Setup ---

    try {
      // --- Transformation Setup ---
      const transform = this.props.transform
      const needsTransform =
        transform && (transform.translateX || transform.translateY || transform.rotate || transform.scale || transform.scaleX || transform.scaleY)

      let savedContextForTransform = false
      if (needsTransform) {
        ctx.save()
        savedContextForTransform = true
        const originXRaw = transform.originX ?? '50%'
        const originYRaw = transform.originY ?? '50%'
        const originOffsetX = parsePercentage(originXRaw, width)
        const originOffsetY = parsePercentage(originYRaw, height)
        const originAbsX = x + originOffsetX
        const originAbsY = y + originOffsetY
        ctx.translate(originAbsX, originAbsY)
        // The chain reads `scale() rotate() translate()`, applied in that order about the origin.
        // Each function is written in the coordinate system the ones before it left behind, so the
        // translation moves the box by scaled, rotated distance -- `scale: 2, translateY: 30` moves
        // it 60 device pixels down, which is what Chrome does with `transform: scale(2)
        // translateY(30px)`. Translating first instead would move it 30 whatever the scale said.
        //
        // Lengths are still resolved against the untransformed border box, as CSS resolves them:
        // `translateY: '60%'` of a 50-tall box is 30 before the scale reaches it, not after.
        if (transform.scale || transform.scaleX || transform.scaleY) {
          const scaleX = transform.scaleX ?? transform.scale ?? 1
          const scaleY = transform.scaleY ?? transform.scale ?? 1
          if (scaleX !== 1 || scaleY !== 1) ctx.scale(scaleX, scaleY)
        }
        if (transform.rotate) {
          ctx.rotate((transform.rotate * Math.PI) / 180)
        }
        if (transform.translateX || transform.translateY) {
          const tx = parsePercentage(transform.translateX, width)
          const ty = parsePercentage(transform.translateY, height)
          if (tx !== 0 || ty !== 0) ctx.translate(tx, ty)
        }
        ctx.translate(-originAbsX, -originAbsY)
      }
      // --- End Transformation Setup ---

      // --- Step 0: Backdrop Filter ---
      // Filters what is already on the canvas behind this node, before the node itself is drawn —
      // CSS paints the element's own background over the filtered backdrop, not under it.
      if (this.props.backdropFilter && !groupEffectsApplied) {
        this.applyBackdropFilter(ctx, ctx, this.props.backdropFilter, x, y, width, height)
      }
      // --- End Backdrop Filter ---

      // --- Step 1: Render Parent Background/Borders/Content ---
      // This renders the current node's own visual appearance first.
      await this._renderContent(ctx, x, y, width, height)

      // --- Step 2: Prepare Children for Stacking ---
      //
      // A child forming no stacking context does not contain its own positioned descendants: CSS
      // gives those to the nearest ancestor that does form one, so a `z-index` deep in the tree can
      // beat a shallow sibling. They are gathered here with the origin their own parent was painted
      // at, and the parent is told to leave them alone.
      //
      // `suppressStacking` is that telling. An ancestor has already taken every stacking descendant
      // reachable through nodes that form no context, so this node paints only what is left.
      const stackingEntries: StackingEntry[] = []
      const lifted = new Set<BoxNode>()
      if (!suppressStacking) this.collectStacking(this, x, y, stackingEntries, lifted)
      stackingEntries.sort((a, b) => a.zIndex - b.zIndex || a.order - b.order)

      const inFlowChildren = this.children.filter(child => !child.stacksAmongSiblings())
      void lifted

      // --- Step 3: Prepare Clipping ---
      //
      // `overflow: hidden` clips this node's content, but not every descendant is its content. An
      // absolutely positioned node is laid out against its containing block, and a box it merely
      // sits inside does not clip it -- CSS clips such a node only where the clipper is that
      // containing block, or lies between it and one. So a static box clips its in-flow children
      // and lets an absolute one through, which is what Chrome does.
      //
      // Applied per child rather than once around them all, so the two kinds can be interleaved
      // without disturbing the paint order worked out above.
      const clips = this.props.overflow === Style.Overflow.Hidden && (width > 0 || height > 0)

      /** Applies one remembered clipper's rectangle. */
      const applyFrame = (frame: ClipFrame) => {
        if (frame.width > 0 && frame.height > 0) {
          drawRoundedRectPath(ctx, frame.x, frame.y, frame.width, frame.height, frame.radii)
          ctx.clip()
        } else {
          ctx.beginPath()
          ctx.rect(frame.x, frame.y, 0, 0)
          ctx.clip()
        }
      }

      /** Draws one child under whatever clips still reach it. */
      const paintChild = async (child: BoxNode, originX: number = x, originY: number = y, inherited: ClipFrame[] = []) => {
        const absolute = child.props.positionType === Style.PositionType.Absolute
        const fixed = child.props.positionType === Style.PositionType.Fixed
        // Whether a clipper reaches this child at all. A positioned node is cut only where the
        // clipper is its containing block, and the two kinds do not share one: an absolute node's
        // is any positioned ancestor, a fixed node's is only a transform or a filter -- so a plain
        // `overflow: hidden` never cuts a fixed node however it is positioned.
        const reaches = (frame: ClipFrame) => (fixed ? frame.containsFixed : absolute ? frame.containsAbsolutes : true)
        const own = clips ? this.clipFrame(x, y) : null
        const frames = [...inherited, ...(own ? [own] : [])].filter(reaches)

        if (frames.length > 0) ctx.save()
        try {
          for (const frame of frames) applyFrame(frame)
          // A child forming no stacking context has had its own stacking descendants taken above,
          // so it must not paint them again.
          await child.render(ctx, originX, originY, !child.createsStackingContext())
        } finally {
          if (frames.length > 0) ctx.restore()
        }
      }

      // --- Step 4: Render Children in Stacking Order ---

      // 4a: Anything with a negative zIndex, below the flow
      for (const entry of stackingEntries) {
        if (entry.zIndex < 0) await paintChild(entry.node, entry.originX, entry.originY, entry.clips)
      }

      // 4b: Direct children that stay in the flow
      for (const child of inFlowChildren) {
        await paintChild(child)
      }

      // 4c: Everything else, ordered by zIndex then by how deep and how late it was found
      for (const entry of stackingEntries) {
        if (entry.zIndex >= 0) await paintChild(entry.node, entry.originX, entry.originY, entry.clips)
      }
      // --- End Child Rendering ---

      // --- Step 6: Restore Transformation Context ---
      if (savedContextForTransform) {
        ctx.restore()
      }
      // --- End Transformation Restoration ---
    } finally {
      // --- Opacity Restoration ---
      // Closing the layer is what composites it onto the page at the node's opacity.
      if (appliedOpacity) {
        ctx.restore()
      }
      // --- End Opacity Restoration ---
    }
  }

  /**
   * Renders the node's visual content including background fills, shadows, and borders.
   * This is an internal method used by the render() pipeline.
   * @param ctx The skia-canvas 2D rendering context to draw into
   * @param x The absolute x-coordinate where drawing should begin
   * @param y The absolute y-coordinate where drawing should begin
   * @param width The width of the content area to render
   * @param height The height of the content area to render
   */
  protected async _renderContent(ctx: CanvasRenderingContext2D, x: number, y: number, width: number, height: number) {
    // Calculate border radius values for all corners
    const radii = { TopLeft: 0, TopRight: 0, BottomRight: 0, BottomLeft: 0 }
    if (this.props.borderRadius) {
      // Handle both number and object border radius specifications
      if (typeof this.props.borderRadius === 'number') {
        radii.TopLeft = radii.TopRight = radii.BottomRight = radii.BottomLeft = this.props.borderRadius
      } else {
        // Extract individual corner radii, defaulting to 0 if not specified
        radii.TopLeft = this.props.borderRadius.TopLeft ?? 0
        radii.TopRight = this.props.borderRadius.TopRight ?? 0
        radii.BottomRight = this.props.borderRadius.BottomRight ?? 0
        radii.BottomLeft = this.props.borderRadius.BottomLeft ?? 0
      }
      // Ensure all radii are non-negative
      radii.TopLeft = Math.max(0, radii.TopLeft)
      radii.TopRight = Math.max(0, radii.TopRight)
      radii.BottomRight = Math.max(0, radii.BottomRight)
      radii.BottomLeft = Math.max(0, radii.BottomLeft)
    }

    // Process shadow configurations
    let shadows: BoxShadowProps[] = []
    if (this.props.boxShadow) {
      shadows = Array.isArray(this.props.boxShadow) ? this.props.boxShadow : [this.props.boxShadow]
    }
    // Split shadows into outset (normal) and inset types
    const outsetShadows = shadows.filter(s => !s.inset)
    const insetShadows = shadows.filter(s => s.inset)

    // Determine if background is fully opaque for shadow optimization
    const backgroundColor = this.props.backgroundColor
    let isOpaque = false
    if (backgroundColor && !this.props.gradient) {
      isOpaque = _isColorOpaque(backgroundColor)
    }

    // Render outset shadows if present
    if (outsetShadows.length > 0) {
      const subtractOffset = 0.75
      if (isOpaque) {
        // Optimized rendering path for opaque backgrounds
        ctx.save()
        ctx.fillStyle = 'black' // Shadow source color
        for (const shadow of outsetShadows) {
          ctx.shadowColor = shadow.color ?? 'black'
          ctx.shadowOffsetX = shadow.offsetX ?? 0
          ctx.shadowOffsetY = shadow.offsetY ?? 0
          ctx.shadowBlur = shadow.blur ?? Math.max(shadow.offsetX ?? 0, shadow.offsetY ?? 0)
          const shape = spreadShape(x + subtractOffset / 2, y + subtractOffset / 2, width - subtractOffset, height - subtractOffset, radii, shadow.spread ?? 0)
          drawRoundedRectPath(ctx, shape.x, shape.y, shape.width, shape.height, shape.radii)
          ctx.fill()
        }
        ctx.restore()
      } else {
        // Complex shadow rendering for transparent/gradient backgrounds
        let maxBlur = 0
        let maxOffsetX = 0
        let maxOffsetY = 0

        // Calculate maximum shadow extents
        for (const shadow of outsetShadows) {
          const currentOffsetX = shadow.offsetX ?? 0
          const currentOffsetY = shadow.offsetY ?? 0
          const currentBlur = shadow.blur ?? Math.max(currentOffsetX, currentOffsetY)
          maxBlur = Math.max(maxBlur, currentBlur + Math.max(0, shadow.spread ?? 0))
          maxOffsetX = Math.max(maxOffsetX, Math.abs(currentOffsetX))
          maxOffsetY = Math.max(maxOffsetY, Math.abs(currentOffsetY))
        }

        // Calculate offscreen canvas size with padding for shadows
        const blurPaddingMultiplier = 2
        const shadowPadding = Math.ceil(maxBlur * blurPaddingMultiplier + Math.max(maxOffsetX, maxOffsetY))
        const offscreenWidth = Math.ceil(width + shadowPadding * 2)
        const offscreenHeight = Math.ceil(height + shadowPadding * 2)

        if (offscreenWidth > 0 && offscreenHeight > 0) {
          // Create temporary canvas for shadow composition
          const offscreenCanvas = createCanvas(offscreenWidth, offscreenHeight, mirrorEngine(ctx))
          const offCtx = offscreenCanvas.getContext('2d')
          offCtx.imageSmoothingEnabled = true
          offCtx.imageSmoothingQuality = 'high'
          const shapeOffsetX = shadowPadding
          const shapeOffsetY = shadowPadding

          // Render each shadow individually onto offscreen canvas
          for (const shadow of outsetShadows) {
            offCtx.save()
            const shadowOffsetX = shadow.offsetX ?? 0
            const shadowOffsetY = shadow.offsetY ?? 0
            const blur = shadow.blur ?? Math.max(shadowOffsetX, shadowOffsetY)
            offCtx.shadowColor = shadow.color ?? 'black'
            offCtx.shadowOffsetX = shadowOffsetX
            offCtx.shadowOffsetY = shadowOffsetY
            offCtx.shadowBlur = Math.max(0, blur)
            const shape = spreadShape(
              shapeOffsetX + subtractOffset / 2,
              shapeOffsetY + subtractOffset / 2,
              width - subtractOffset,
              height - subtractOffset,
              radii,
              shadow.spread ?? 0,
            )
            drawRoundedRectPath(offCtx, shape.x, shape.y, shape.width, shape.height, shape.radii)
            offCtx.fillStyle = 'rgba(0,0,0,1)'
            offCtx.fill()
            offCtx.restore()
          }

          // Cut out the shape from accumulated shadows
          offCtx.save()
          offCtx.globalCompositeOperation = 'destination-out'
          drawRoundedRectPath(offCtx, shapeOffsetX, shapeOffsetY, width, height, radii)
          offCtx.fillStyle = 'rgba(0,0,0,1)'
          offCtx.fill()
          offCtx.restore()

          // Composite shadow result onto main canvas
          ctx.drawImage(offscreenCanvas, x - shadowPadding, y - shadowPadding)
        }
      }
    }

    // Render background fill (solid color or gradient)
    // This logic uses standard context methods and remains unchanged.
    const hasFill = this.props.gradient || this.props.backgroundColor
    if (hasFill) {
      let fillStyle: string | CanvasGradient = this.props.backgroundColor || 'transparent'
      if (this.props.gradient) {
        const { gradient: grad, reason } = createGradient(ctx, this.props.gradient, { x, y, width, height })
        if (grad) {
          fillStyle = grad
        } else {
          console.warn(`[BoxNode ${this.key}] ${reason} Falling back to backgroundColor.`)
        }
      }
      if (fillStyle && fillStyle !== 'transparent') {
        ctx.fillStyle = fillStyle
        drawRoundedRectPath(ctx, x, y, width, height, radii)
        ctx.fill()
      }
    }

    // Render the background picture, over the fill and under everything else — CSS order.
    if (this.backgroundBitmap) {
      paintBackgroundImage(ctx, this.backgroundBitmap, this.props.backgroundImage!, { x, y, width, height }, radii)
    }

    // Render inset shadows
    //
    // Built on an offscreen the size of the node: flood it with the shadow colour, then erase the
    // node's own shape from it, offset and blurred. What survives is the colour hugging the edges
    // the shape has moved away from -- which is what CSS draws, and why `inset 20px 20px` darkens
    // the top and left rather than the bottom and right.
    //
    // Erasing rather than casting a canvas shadow, because a canvas shadow is cast by what is
    // actually painted and the paint would have to sit inside the clip to cast inward. This used to
    // stroke a path with `strokeStyle = 'transparent'` on the note that only the shadow mattered;
    // nothing is painted by a transparent stroke, so inset shadows drew nothing at all.
    if (insetShadows.length > 0 && width > 0 && height > 0) {
      for (const shadow of insetShadows) {
        const shadowOffsetX = shadow.offsetX ?? 0
        const shadowOffsetY = shadow.offsetY ?? 0
        const blur = Math.max(0, shadow.blur ?? 0)
        const spread = shadow.spread ?? 0

        const offscreen = createCanvas(Math.ceil(width), Math.ceil(height), mirrorEngine(ctx))
        const offCtx = offscreen.getContext('2d')

        offCtx.fillStyle = shadow.color ?? 'black'
        offCtx.fillRect(0, 0, width, height)

        // `filter` takes a standard deviation where a shadow blur takes a diameter, so the radius
        // CSS names is half of it.
        if (blur > 0) offCtx.filter = `blur(${blur / 2}px)`
        offCtx.globalCompositeOperation = 'destination-out'

        const holeRadii = {
          TopLeft: Math.max(0, radii.TopLeft - spread),
          TopRight: Math.max(0, radii.TopRight - spread),
          BottomRight: Math.max(0, radii.BottomRight - spread),
          BottomLeft: Math.max(0, radii.BottomLeft - spread),
        }
        drawRoundedRectPath(
          offCtx,
          shadowOffsetX + spread,
          shadowOffsetY + spread,
          Math.max(0, width - spread * 2),
          Math.max(0, height - spread * 2),
          holeRadii,
        )
        offCtx.fillStyle = 'rgba(0,0,0,1)'
        offCtx.fill()

        ctx.save()
        drawRoundedRectPath(ctx, x, y, width, height, radii)
        ctx.clip()
        ctx.drawImage(offscreen, x, y)
        ctx.restore()
      }
    }

    // Render border strokes
    // (This logic uses standard context methods via drawBorders helper and remains unchanged)
    drawBorders({
      ctx,
      node: this.node,
      x,
      y,
      width,
      height,
      radii,
      borderColor: this.props.borderColor,
      borderStyle: this.props.borderStyle,
    })
  }
}

/**
 * A corner radius grown by `spread`.
 *
 * A square corner stays square however far the shadow spreads — CSS grows a radius only where there
 * is one, so a spread shadow on a plain rectangle is a larger rectangle rather than a rounded one.
 */
function grownRadius(radius: number, spread: number): number {
  return radius > 0 ? Math.max(0, radius + spread) : 0
}

/**
 * The silhouette a shadow is cast from: the node's box grown by `spread`, corners included.
 *
 * CSS grows the box before the blur is applied, so a spread shadow is a larger copy of the shape
 * rather than a wider blur. A negative spread shrinks it, and a corner cannot curve by less than
 * nothing.
 */
function spreadShape(
  x: number,
  y: number,
  width: number,
  height: number,
  radii: { TopLeft: number; TopRight: number; BottomRight: number; BottomLeft: number },
  spread: number,
) {
  return {
    x: x - spread,
    y: y - spread,
    width: Math.max(0, width + spread * 2),
    height: Math.max(0, height + spread * 2),
    radii: {
      TopLeft: grownRadius(radii.TopLeft, spread),
      TopRight: grownRadius(radii.TopRight, spread),
      BottomRight: grownRadius(radii.BottomRight, spread),
      BottomLeft: grownRadius(radii.BottomLeft, spread),
    },
  }
}

/**
 * Normalizes children into a flat CanvasElement array, filtering falsy values.
 */
export function normalizeDescriptorChildren(children: BoxProps['children']): CanvasElement[] | undefined {
  if (children === undefined || children === null || children === false) return undefined
  const arr = (Array.isArray(children) ? children : [children]).filter(Boolean) as CanvasElement[]
  return arr.length > 0 ? arr : undefined
}

/**
 * Generic helper to set gap/margin/padding edge values on a Yoga node.
 * Handles scalar (number | string), percent strings, and per-edge object notation.
 */
function _setEdgeValues(
  value: number | string | Record<string, number | string>,
  keys: { [key: string]: any },
  setFn: (edge: any, val: number | string) => void,
  percentFn?: (edge: any, val: number) => void,
  allowAuto = false,
): void {
  if (typeof value === 'number' || (allowAuto && value === 'auto')) {
    setFn(keys.All, value)
  } else if (typeof value === 'string' && percentFn && value.endsWith('%')) {
    percentFn(keys.All, parseFloat(value))
  } else if (typeof value === 'object') {
    for (const [key, val] of Object.entries(value)) {
      if (key in keys) {
        const edgeKey = keys[key]
        if (typeof val === 'string' && percentFn && val.endsWith('%')) {
          percentFn(edgeKey, parseFloat(val))
        } else {
          setFn(edgeKey, val as number)
        }
      }
    }
  }
}

/**
 * Checks if a CSS color string represents a fully opaque color (alpha = 1).
 * Handles hex (#RGB, #RRGGBB, #RRGGBBAA), rgb()/rgba(), and transparent.
 */
function _isColorOpaque(color: string): boolean {
  if (color === 'transparent') return false

  const hexAlpha = _HEX_ALPHA_RE.exec(color)
  if (hexAlpha) {
    return parseInt(hexAlpha[1].slice(6), 16) === 255
  }
  // #RGB or #RRGGBB are always opaque
  if (color.startsWith('#')) return true

  // rgba(r, g, b, a)
  const rgba = /rgba?\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)\s*(?:,\s*([\d.]+))?\s*\)/.exec(color)
  if (rgba) {
    const a = rgba[4] !== undefined ? parseFloat(rgba[4]) : 1
    return a === 1
  }

  // Unknown format — assume opaque (covers named colors like 'black', 'white', etc.)
  return true
}

/**
 * A rectangle that lays its children out with flexbox.
 *
 * The base of every layout here: `Column` and `Row` are this with `flexDirection` preset.
 * @param props Layout, style and children.
 * @returns A descriptor the renderer turns into a node.
 * @example
 * ```ts
 * Box({
 *   width: 200,
 *   padding: 16,
 *   backgroundColor: '#0b1120',
 *   borderRadius: 12,
 *   children: [Text('hello', { color: '#f8fafc' })],
 * })
 * ```
 */
export const Box = ({ children, ...rest }: BoxProps): CanvasElement => ({
  __type: 'Box',
  props: rest,
  children: normalizeDescriptorChildren(children),
})

/**
 * @class ColumnNode
 * Node class for vertical column layout
 */
export class ColumnNode extends BoxNode {
  constructor(props: BoxProps & BaseProps = {}) {
    super({
      display: Style.Display.Flex,
      flexDirection: Style.FlexDirection.Column,
      flexShrink: 1,
      flexBasis: props.flexGrow === 1 ? 0 : undefined,
      ...props,
    })
  }
}

/**
 * A {@link Box} that stacks its children vertically.
 * @example
 * ```ts
 * Column({ gap: 8, children: [Text('title'), Text('subtitle')] })
 * ```
 */
export const Column = ({ children, ...rest }: BoxProps): CanvasElement => ({
  __type: 'Column',
  props: rest,
  children: normalizeDescriptorChildren(children),
})

/**
 * @class RowNode
 * Node class for horizontal row layout.
 */
export class RowNode extends BoxNode {
  constructor(props: BoxProps & BaseProps = {}) {
    super({
      name: 'Row',
      display: Style.Display.Flex,
      flexDirection: Style.FlexDirection.Row,
      flexShrink: 1, // Default shrink for rows
      ...props,
    })
  }
}

/**
 * A {@link Box} that lays its children out side by side.
 *
 * A row stretches to its parent's width by default, as flexbox does, so
 * `justifyContent` has space to distribute without being given one.
 * @example
 * ```ts
 * Row({
 *   justifyContent: Style.Justify.SpaceBetween,
 *   alignItems: Style.Align.Center,
 *   children: [Text('left'), Text('right')],
 * })
 * ```
 */
export const Row = ({ children, ...rest }: BoxProps): CanvasElement => ({
  __type: 'Row',
  props: rest,
  children: normalizeDescriptorChildren(children),
})
