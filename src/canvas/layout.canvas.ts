import { type CanvasRenderingContext2D, type CanvasGradient } from 'meo-skia-canvas'
import { drawBorders, drawRoundedRectPath, filterSpill, parseBorderRadius, parsePercentage, scaleFilterLengths } from '@/canvas/canvas.helper.js'
import { createGradient } from '@/canvas/gradient.canvas.js'
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
export class BoxNode {
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
      direction = Style.Direction.LTR,
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
    if (positionType !== undefined) this.node.setPositionType(positionType)
    if (flexBasis !== undefined) this.node.setFlexBasis(flexBasis)
    // `position: 0` is falsy and meaningful: an absolute node inset by nothing on all four sides
    // fills its parent.
    if (position !== undefined) {
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
  async render(ctx: CanvasRenderingContext2D, offsetX: number = 0, offsetY: number = 0) {
    const { dither } = this.props
    if (dither === undefined) return this.renderMasked(ctx, offsetX, offsetY)

    // Put back by hand rather than through `save`/`restore`, which would clone the whole graphics
    // state to carry one boolean. Putting it back at all is what keeps the node that draws next
    // reading its own ancestor's answer rather than this one's, and a node that says nothing
    // inherits by leaving the context alone.
    const inherited = ctx.dither
    ctx.dither = dither
    try {
      return await this.renderMasked(ctx, offsetX, offsetY)
    } finally {
      ctx.dither = inherited
    }
  }

  /** {@link render} without the dither state around it: the mask, then the node itself. */
  private async renderMasked(ctx: CanvasRenderingContext2D, offsetX: number, offsetY: number) {
    const mask = this.props.mask
    if (!mask) return this.renderNode(ctx, offsetX, offsetY)

    const layout = this.node.getComputedLayout()
    const box = { x: layout.left + offsetX, y: layout.top + offsetY, width: layout.width, height: layout.height }
    if (box.width <= 0 || box.height <= 0) return

    if (isGradientMask(mask)) {
      const drawn = await drawWithGradientMask(ctx, mask.gradient, box, target => this.renderNode(target, offsetX, offsetY), `[BoxNode ${this.key}]`)
      // A gradient that could not be built is not a reason to lose the node; it draws unmasked,
      // having already said why.
      return drawn ? undefined : this.renderNode(ctx, offsetX, offsetY)
    }

    const path = maskPath(mask, box)
    if (!path) return this.renderNode(ctx, offsetX, offsetY)

    ctx.save()
    try {
      ctx.clip(path, maskFillRule(mask))
      await this.renderNode(ctx, offsetX, offsetY)
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
   * The copy is the size of the whole canvas. A backdrop is a deliberate effect on a handful of
   * nodes rather than something every node pays for, and copying only the node's own region would
   * still have to account for however far the filter reaches beyond it.
   */
  private applyBackdropFilter(ctx: CanvasRenderingContext2D, filter: string, x: number, y: number, width: number, height: number) {
    const surface = ctx.canvas
    if (!surface?.width || !surface?.height) return

    const snapshot = createCanvas(surface.width, surface.height, mirrorEngine(ctx))
    const snapshotCtx = snapshot.getContext('2d')
    snapshotCtx.drawImage(surface, 0, 0)

    ctx.save()
    try {
      drawRoundedRectPath(ctx, x, y, width, height, parseBorderRadius(this.props.borderRadius))
      ctx.clip()
      const matrix = ctx.getTransform()
      ctx.resetTransform()
      ctx.filter = scaleFilterLengths(filter, (Math.hypot(matrix.a, matrix.b) + Math.hypot(matrix.c, matrix.d)) / 2 || 1)
      ctx.drawImage(snapshot, 0, 0)
    } finally {
      ctx.restore()
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
    const pad = filterSpill(filter)
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
  private async renderNode(ctx: CanvasRenderingContext2D, offsetX: number = 0, offsetY: number = 0, groupEffectsApplied: boolean = false) {
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
        if (transform.translateX || transform.translateY) {
          const tx = parsePercentage(transform.translateX, width)
          const ty = parsePercentage(transform.translateY, height)
          if (tx !== 0 || ty !== 0) ctx.translate(tx, ty)
        }
        if (transform.rotate) {
          ctx.rotate((transform.rotate * Math.PI) / 180)
        }
        if (transform.scale || transform.scaleX || transform.scaleY) {
          const scaleX = transform.scaleX ?? transform.scale ?? 1
          const scaleY = transform.scaleY ?? transform.scale ?? 1
          if (scaleX !== 1 || scaleY !== 1) ctx.scale(scaleX, scaleY)
        }
        ctx.translate(-originAbsX, -originAbsY)
      }
      // --- End Transformation Setup ---

      // --- Step 0: Backdrop Filter ---
      // Filters what is already on the canvas behind this node, before the node itself is drawn —
      // CSS paints the element's own background over the filtered backdrop, not under it.
      if (this.props.backdropFilter) {
        this.applyBackdropFilter(ctx, this.props.backdropFilter, x, y, width, height)
      }
      // --- End Backdrop Filter ---

      // --- Step 1: Render Parent Background/Borders/Content ---
      // This renders the current node's own visual appearance first.
      await this._renderContent(ctx, x, y, width, height)

      // --- Step 2: Prepare Children for Stacking ---
      const positionedChildren: { node: BoxNode; zIndex: number; originalIndex: number }[] = []
      const inFlowChildren: BoxNode[] = []

      this.children.forEach((child, index) => {
        // Every absolutely positioned child is a positioned descendant, whether or not it named a
        // zIndex. CSS paints those above in-flow content, and `z-index: auto` shares a layer with
        // `z-index: 0` — so an unindexed one defaults to 0 rather than falling back into the flow,
        // where a later sibling would bury it.
        //
        // `Relative` stays in the flow. It is Yoga's default position type, so it is what every
        // ordinary child already is — CSS `static` rather than CSS `relative` — and treating it as
        // positioned would lift the whole tree into the positioned layer.
        if (child.props.positionType === Style.PositionType.Absolute) {
          positionedChildren.push({
            node: child,
            zIndex: child.props.zIndex ?? 0,
            originalIndex: index, // Keep original order for tie-breaking
          })
        } else {
          inFlowChildren.push(child)
        }
      })

      // Sort positioned children by zIndex, then by original order
      positionedChildren.sort((a, b) => {
        return a.zIndex - b.zIndex || a.originalIndex - b.originalIndex
      })

      // --- Step 3: Handle Clipping (Applies before drawing children) ---
      let savedContextForClip = false
      if (this.props.overflow === Style.Overflow.Hidden && (width > 0 || height > 0)) {
        ctx.save()
        savedContextForClip = true
        const borderLeft = this.node.getComputedBorder(Style.Edge.Left)
        const borderTop = this.node.getComputedBorder(Style.Edge.Top)
        const borderRight = this.node.getComputedBorder(Style.Edge.Right)
        const borderBottom = this.node.getComputedBorder(Style.Edge.Bottom)
        const innerX = x + borderLeft
        const innerY = y + borderTop
        const innerWidth = Math.max(0, width - borderLeft - borderRight)
        const innerHeight = Math.max(0, height - borderTop - borderBottom)
        const outerRadii = parseBorderRadius(this.props.borderRadius)
        const innerRadii = {
          TopLeft: Math.max(0, outerRadii.TopLeft - Math.max(borderLeft, borderTop)),
          TopRight: Math.max(0, outerRadii.TopRight - Math.max(borderRight, borderTop)),
          BottomRight: Math.max(0, outerRadii.BottomRight - Math.max(borderRight, borderBottom)),
          BottomLeft: Math.max(0, outerRadii.BottomLeft - Math.max(borderLeft, borderBottom)),
        }
        if (innerWidth > 0 && innerHeight > 0) {
          drawRoundedRectPath(ctx, innerX, innerY, innerWidth, innerHeight, innerRadii)
          ctx.clip()
        } else {
          ctx.beginPath()
          ctx.rect(innerX, innerY, 0, 0)
          ctx.clip()
        }
      }
      // --- End Clipping Setup ---

      // --- Step 4: Render Children in Stacking Order ---

      // 4a: Render positioned children with negative zIndex
      for (const item of positionedChildren) {
        if (item.zIndex < 0) {
          // Pass parent's layout origin (x, y) as offset
          await item.node.render(ctx, x, y)
        }
      }

      // 4b: Render in-flow children (recursively)
      for (const child of inFlowChildren) {
        // Pass parent's layout origin (x, y) as offset
        await child.render(ctx, x, y)
      }

      // 4c: Render positioned children with zero or positive zIndex
      for (const item of positionedChildren) {
        if (item.zIndex >= 0) {
          // Pass parent's layout origin (x, y) as offset
          await item.node.render(ctx, x, y)
        }
      }
      // --- End Child Rendering ---

      // --- Step 5: Restore Clipping Context ---
      if (savedContextForClip) {
        ctx.restore()
      }
      // --- End Clipping Restoration ---

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
