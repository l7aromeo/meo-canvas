/**
 * The two DOM names comlink's declarations reference, defined for a Node-only build.
 *
 * comlink is written browser-first, so its `.d.ts` reaches for `Transferable` and
 * `EventListenerOrEventListenerObject`. Pulling in the whole DOM lib to satisfy two names would
 * also put `document`, `window` and the rest in scope for a library that has none of them --
 * code referencing them would typecheck and then fail at runtime.
 *
 * These are the Node-accurate definitions: over `worker_threads`, the only transferables are
 * `ArrayBuffer` and `MessagePort`, which is narrower and more correct than the DOM's list.
 */
declare global {
  type Transferable = ArrayBuffer | MessagePort
  type EventListenerOrEventListenerObject = ((evt: Event) => void) | { handleEvent(evt: Event): void }
}

export {}
