import { hashBuffer } from '@/util/disk.cache.js'

/**
 * Produces a short, deterministic, filesystem-safe hash of a `RequestInit`
 * (the `httpOptions` passed to an `Image`), used to key the image cache so that
 * the same URL fetched with different headers/method/body does not collide on a
 * stale cached image.
 *
 * The whole options object is folded into the hash, but non-plain fetch fields
 * are normalized so the result stays stable and serialization never throws:
 *
 *  - `Headers`         → sorted `[key, value]` entries (keys are already lowercased by `Headers`)
 *  - `URLSearchParams` → sorted `[key, value]` entries
 *  - `FormData`        → sorted entries; `Blob`/`File` values reduced to `{ name, size, type }`
 *  - `Blob` / `File`   → `{ __type, name?, size, type }` marker (content is not read)
 *  - `ReadableStream`  → `{ __type }` marker (streams are not consumed)
 *  - `AbortSignal`     → dropped (a cancellation control, irrelevant to cache identity)
 *  - plain objects     → keys sorted so insertion order does not affect the hash
 *  - circular refs     → replaced with a `'[Circular]'` marker
 *
 * Returns `''` for `undefined`/empty options, and also `''` if serialization
 * fails for any reason (the caller then simply omits options from the cache key
 * rather than crashing the render).
 */
export function hashHttpOptions(httpOptions?: RequestInit): string {
  if (!httpOptions) return ''

  try {
    // Canonicalize `headers` so the three accepted forms (Headers instance,
    // plain object, array of pairs) collapse to one representation — otherwise
    // equivalent headers expressed differently would miss each other's cache.
    let canonical: RequestInit = httpOptions
    if (httpOptions.headers !== undefined) {
      try {
        canonical = { ...httpOptions, headers: new Headers(httpOptions.headers) }
      } catch {
        // Leave headers untouched if they can't be coerced into a Headers instance.
      }
    }

    const serialized = stableStringify(canonical)
    // `{}` (no meaningful options) contributes nothing to the cache key.
    if (serialized === '' || serialized === '{}') return ''
    return hashBuffer(Buffer.from(serialized))
  } catch (error) {
    console.warn('[http.options] Failed to hash httpOptions; excluding it from the cache key:', (error as Error).message)
    return ''
  }
}

const CIRCULAR_MARKER = '[Circular]'

function isBlob(value: unknown): value is Blob {
  return typeof Blob !== 'undefined' && value instanceof Blob
}

/**
 * Deterministic `JSON.stringify`: sorts object keys, normalizes fetch-specific
 * types into plain serializable shapes, and guards against circular references.
 */
function stableStringify(value: unknown): string {
  const seen = new WeakSet<object>()

  const normalize = (val: unknown): unknown => {
    if (val === null || typeof val !== 'object') return val

    if (val instanceof Headers) {
      return { __type: 'Headers', entries: [...val.entries()].sort(compareEntries) }
    }
    if (typeof URLSearchParams !== 'undefined' && val instanceof URLSearchParams) {
      return { __type: 'URLSearchParams', entries: [...val.entries()].sort(compareEntries) }
    }
    if (typeof FormData !== 'undefined' && val instanceof FormData) {
      const entries = [...val.entries()].map(([k, v]) => [k, isBlob(v) ? describeBlob(v) : v] as const)
      return { __type: 'FormData', entries: entries.sort(compareEntries) }
    }
    if (typeof ReadableStream !== 'undefined' && val instanceof ReadableStream) {
      return { __type: 'ReadableStream' }
    }
    if (isBlob(val)) {
      return describeBlob(val)
    }
    if (typeof AbortSignal !== 'undefined' && val instanceof AbortSignal) {
      // Cancellation control — not part of the resource's identity.
      return undefined
    }
    if (ArrayBuffer.isView(val) || val instanceof ArrayBuffer) {
      const bytes = val instanceof ArrayBuffer ? new Uint8Array(val) : new Uint8Array(val.buffer, val.byteOffset, val.byteLength)
      return { __type: 'Bytes', hash: hashBuffer(Buffer.from(bytes)) }
    }

    if (seen.has(val)) return CIRCULAR_MARKER
    seen.add(val)

    if (Array.isArray(val)) {
      return val.map(normalize)
    }

    const out: Record<string, unknown> = {}
    for (const key of Object.keys(val).sort()) {
      const normalized = normalize((val as Record<string, unknown>)[key])
      if (normalized !== undefined) out[key] = normalized
    }
    return out
  }

  const normalized = normalize(value)
  return normalized === undefined ? '' : JSON.stringify(normalized)
}

function describeBlob(blob: Blob): Record<string, unknown> {
  const marker: Record<string, unknown> = { __type: 'Blob', size: blob.size, type: blob.type }
  if (typeof (blob as File).name === 'string') marker.name = (blob as File).name
  return marker
}

/** Stable ordering for `[key, value]` entry pairs. */
function compareEntries(a: readonly [string, unknown], b: readonly [string, unknown]): number {
  if (a[0] !== b[0]) return a[0] < b[0] ? -1 : 1
  return String(a[1]) < String(b[1]) ? -1 : String(a[1]) > String(b[1]) ? 1 : 0
}
