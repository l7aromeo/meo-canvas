import { type CanvasRenderingContext2D, type CanvasGradient } from 'meo-skia-canvas'
import { drawBorders, drawRoundedRectPath, parseBorderRadius, parsePercentage } from '@/canvas/canvas.helper.js'
import { createGradient } from '@/canvas/gradient.canvas.js'
import { createCanvas, mirrorEngine } from '@/canvas/canvas.engine.js'
import { drawWithGradientMask, isGradientMask, maskFillRule, maskPath } from '@/canvas/mask.canvas.js'
import type { BaseProps, BoxProps, BoxShadowProps, CanvasElement } from '@/canvas/canvas.type.js'
import Yoga, { Style, Node } from '@/constant/common.const.js'

const _HEX_ALPHA_RE = /^#([0-9a-fA-F]{8})$/

/**
 * @class BoxNode
 * @classdesc Base node class for rendering rectangular boxes with layout, styling, and children.
 * It uses the Yoga layout engine for positioning and sizing.
 */
export class BoxNode {
  /**
   * @property {Partial<BoxProps>} initialProps - Original props passed to the constructor before any modifications.
   */
  initialProps: Partial<BoxProps>

  /**
   * @property {Node} node - The Yoga layout engine node.
   */
  node: Node

  /**
   * @property {BoxNode[]} children - Child nodes.
   */
  children: BoxNode[]

  /**
   * @property {BoxProps & BaseProps} props - Current props including defaults and inherited values.
   */
  props: BoxProps & BaseProps

  /**
   * @property {string} name - Node type name.
   */
  readonly name?: string

  /**
   * @property {string} key - Unique node identifier.
   */
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
   * Renders the node and its children to the canvas.
   * @param {CanvasRenderingContext2D} ctx Canvas rendering context (from skia-canvas).
   * @param {number} offsetX X offset for rendering.
   * @param {number} offsetY Y offset for rendering.
   */

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
   */
  async render(ctx: CanvasRenderingContext2D, offsetX: number = 0, offsetY: number = 0) {
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

  private async renderNode(ctx: CanvasRenderingContext2D, offsetX: number = 0, offsetY: number = 0) {
    const layout = this.node.getComputedLayout()
    const x = layout.left + offsetX
    const y = layout.top + offsetY
    const width = layout.width
    const height = layout.height

    // Exit early if the node is invisible or has no dimensions.
    if (width <= 0 || height <= 0 || this.props.display === Style.Display.None) {
      return
    }

    // --- Opacity Setup ---
    const desiredOpacity = Math.max(0, Math.min(1, this.props.opacity ?? 1))
    let originalAlpha: number | undefined = undefined
    let appliedOpacity = false
    if (desiredOpacity < 1) {
      originalAlpha = ctx.globalAlpha
      ctx.globalAlpha = originalAlpha * desiredOpacity
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

      // --- Step 1: Render Parent Background/Borders/Content ---
      // This renders the current node's own visual appearance first.
      await this._renderContent(ctx, x, y, width, height)

      // --- Step 2: Prepare Children for Stacking ---
      const positionedChildren: { node: BoxNode; zIndex: number; originalIndex: number }[] = []
      const inFlowChildren: BoxNode[] = []

      this.children.forEach((child, index) => {
        // Check if child participates in zIndex stacking
        if (child.props.positionType === Style.PositionType.Absolute && child.props.zIndex !== undefined) {
          positionedChildren.push({
            node: child,
            zIndex: child.props.zIndex,
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
      if (appliedOpacity && originalAlpha !== undefined) {
        ctx.globalAlpha = originalAlpha
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
          drawRoundedRectPath(ctx, x + subtractOffset / 2, y + subtractOffset / 2, width - subtractOffset, height - subtractOffset, radii)
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
          maxBlur = Math.max(maxBlur, currentBlur)
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
            drawRoundedRectPath(
              offCtx,
              shapeOffsetX + subtractOffset / 2,
              shapeOffsetY + subtractOffset / 2,
              width - subtractOffset,
              height - subtractOffset,
              radii,
            )
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
    // This logic uses standard context methods and remains unchanged.
    if (insetShadows.length > 0) {
      for (const shadow of insetShadows) {
        ctx.save()
        const color = shadow.color ?? 'black'
        const shadowOffsetX = shadow.offsetX ?? 0
        const shadowOffsetY = shadow.offsetY ?? 0
        const blur = shadow.blur ?? Math.max(shadowOffsetX, shadowOffsetY)
        drawRoundedRectPath(ctx, x, y, width, height, radii)
        ctx.clip()
        ctx.shadowColor = color
        ctx.shadowOffsetX = shadowOffsetX
        ctx.shadowOffsetY = shadowOffsetY
        ctx.shadowBlur = blur
        ctx.lineWidth = 1 // Minimal line width for the stroke.
        ctx.strokeStyle = 'transparent' // Stroke color doesn't matter; only the shadow does.
        // Draw a slightly offset path *inside* the clip to generate the inset shadow.
        drawRoundedRectPath(ctx, x - shadowOffsetX, y - shadowOffsetY, width, height, radii)
        ctx.stroke() // The stroke generates the shadow inside the clipped area.
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
 * @classdesc Node class for horizontal row layout.
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
