/**
 * skia-canvas types import `sharp` for Canvas#toSharp(); this package does not ship sharp at runtime.
 */
declare module 'sharp' {
  export interface Sharp {
    toBuffer(): Promise<Buffer>
  }

  export default function sharp(input?: unknown): Sharp
}
