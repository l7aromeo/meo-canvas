/**
 * Comlink ships node-adapter only as an ESM deep import without declarations.
 * @see https://github.com/GoogleChromeLabs/comlink/issues/508
 */
declare module 'comlink/dist/esm/node-adapter.mjs' {
  import type { Endpoint } from 'comlink'

  export interface NodeEndpoint {
    postMessage(message: any, transfer?: any[]): void
    on(type: string, listener: EventListenerOrEventListenerObject, options?: Record<string, unknown>): void
    off(type: string, listener: EventListenerOrEventListenerObject, options?: Record<string, unknown>): void
    start?: () => void
  }

  export default function nodeEndpoint(nep: NodeEndpoint): Endpoint
}
