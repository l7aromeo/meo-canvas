// src/worker/comlink.setup.ts
import * as Comlink from 'comlink'
import { MessageChannel } from 'node:worker_threads'

// Use deep import path — Comlink has no `exports` field (issue #508)
// @ts-expect-error — Comlink's node-adapter has no type declarations at this path
import nodeEndpoint from 'comlink/dist/esm/node-adapter.mjs'

/**
 * Fix Comlink.proxy() for Node.js worker_threads (issue #313).
 * The built-in proxy transfer handler uses browser MessageChannel.
 * This override uses Node's MessageChannel instead.
 *
 * Must be called on BOTH main thread and worker before any Comlink usage.
 */
function installNodeProxyHandler() {
  Comlink.transferHandlers.set('proxy', {
    canHandle: (obj: unknown): obj is { [Comlink.proxyMarker]: true } =>
      (typeof obj === 'object' || typeof obj === 'function') && obj !== null && Comlink.proxyMarker in obj,
    serialize: (obj: unknown) => {
      const { port1, port2 } = new MessageChannel()
      Comlink.expose(obj, nodeEndpoint(port1))
      return [port2, [port2]]
    },
    deserialize: (port: MessagePort) => {
      ;(port as any).start?.()
      return Comlink.wrap(nodeEndpoint(port))
    },
  })
}

// Install immediately on import
installNodeProxyHandler()

export { Comlink, nodeEndpoint }
