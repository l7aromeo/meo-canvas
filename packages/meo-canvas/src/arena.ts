/**
 * The `f64` arena: how a scene crosses into the addon.
 *
 * One `Float64Array` carries the whole tree. The wire specification lives in
 * `crates/meo-canvas-node/src/arena.rs` — this is the writing half of it, and
 * the reader on the other side is the definition. Where the two could disagree
 * the numbers are generated rather than copied: {@link MAGIC}, {@link VERSION}
 * and the property tables come from `./generated/arena-tables.js`.
 *
 * It is shaped this way because **reading a value out of V8 is what costs**,
 * not the crossing. A `lineTo` in `meo-skia-canvas` costs 82 nanoseconds, of
 * which 17 is the crossing and 39 is reading two floats out of the arguments.
 * Decoding from a `&[f64]` skips V8 entirely, and a store into a
 * `Float64Array` is one operation where writing varint bytes from JavaScript
 * is several.
 *
 * Strings and buffers cannot live in a `Float64Array`, so they go into a side
 * array and the arena stores an index into it.
 *
 * @packageDocumentation
 */

import { MAGIC, MASK_BITS, VERSION } from './generated/arena-tables.js'

/** A value the arena cannot carry itself, held beside it. */
export type SideValue = string | Uint8Array

/** A written scene: the slots, and the values they index. */
export interface Arena {
  /** Every slot, in the order the reader consumes them. */
  readonly slots: Float64Array
  /** The strings and buffers the slots index into. */
  readonly values: readonly SideValue[]
}

/**
 * The tag an absent optional is written as.
 *
 * Named because `0` and `1` appear as three different things in this format —
 * a presence flag, a boolean, and an enum discriminant — and a reader of this
 * file should not have to work out which from context.
 */
const ABSENT = 0

/** The tag a present optional is written as. */
const PRESENT = 1

/** The largest value one mask slot holds: 53 bits all set. */
const LARGEST_MASK_SLOT = 2 ** MASK_BITS - 1

/**
 * Accumulates the slots of one scene.
 *
 * A cursor rather than an index calculation, because slot counts are not
 * constant: a list writes its length and then its items, an optional writes a
 * flag and perhaps a value. Every method appends and returns nothing, so a
 * caller cannot write a value at the wrong offset by arithmetic.
 */
export class ArenaWriter {
  /** The slots written so far. */
  readonly #slots: number[] = []

  /** The strings and buffers the slots index. */
  readonly #values: SideValue[] = []

  /**
   * Where each string already written sits in {@link ArenaWriter.#values}.
   *
   * Strings repeat heavily — a font family appears on every text node of a
   * document — and each duplicate is one more value the addon has to read out
   * of V8, which is the cost this format exists to avoid. Buffers are not
   * deduplicated: comparing their contents would cost more than the copy saves,
   * and the same buffer written twice is a caller's own doing.
   */
  readonly #strings = new Map<string, number>()

  /** The scene, ready to hand across. */
  finish(): Arena {
    return { slots: Float64Array.from(this.#slots), values: this.#values }
  }

  /** How many slots have been written. */
  get length(): number {
    return this.#slots.length
  }

  /** Writes one slot exactly as given. */
  slot(value: number): void {
    this.#slots.push(value)
  }

  /**
   * Writes a number the reader narrows to `f32`.
   *
   * Nothing is rounded here. The arena is `f64` because JavaScript has no other
   * number and every geometric quantity in a scene is `f32`, so the narrowing
   * happens once, on the reading side, rather than twice.
   */
  f32(value: number): void {
    this.#slots.push(value)
  }

  /** Writes a boolean as `0` or `1`, which is the only thing the reader takes. */
  bool(value: boolean): void {
    this.#slots.push(value ? 1 : 0)
  }

  /** Writes an integer, which must already be one. */
  integer(value: number): void {
    this.#slots.push(value)
  }

  /** Writes an enum's discriminant, the same byte the codec writes. */
  enum(discriminant: number): void {
    this.#slots.push(discriminant)
  }

  /** Writes a count, then leaves the caller to write that many items. */
  count(items: number): void {
    this.#slots.push(items)
  }

  /** Writes an absent optional: one slot, and nothing follows. */
  absent(): void {
    this.#slots.push(ABSENT)
  }

  /** Writes the flag of a present optional. The value follows. */
  present(): void {
    this.#slots.push(PRESENT)
  }

  /** Writes an optional, calling `write` only when there is something to write. */
  optional<T>(value: T | undefined, write: (value: T) => void): void {
    if (value === undefined) {
      this.absent()
      return
    }
    this.present()
    write(value)
  }

  /** Writes a string as an index into the side values. */
  text(value: string): void {
    const seen = this.#strings.get(value)
    if (seen !== undefined) {
      this.#slots.push(seen)
      return
    }
    const index = this.#values.length
    this.#values.push(value)
    this.#strings.set(value, index)
    this.#slots.push(index)
  }

  /** Writes a buffer as an index into the side values. */
  bytes(value: Uint8Array): void {
    const index = this.#values.length
    this.#values.push(value)
    this.#slots.push(index)
  }

  /**
   * Reserves a group's mask slots and returns where they sit.
   *
   * The mask says which of a group's properties follow it, and that is not
   * known until they have been written. So the slots are reserved, the values
   * are written, and {@link ArenaWriter.patchMask} fills the reservation in —
   * one pass over the style rather than one to decide and another to write.
   */
  reserveMask(slots: number): number {
    const at = this.#slots.length
    for (let index = 0; index < slots; index += 1) this.#slots.push(0)
    return at
  }

  /**
   * Fills in a reservation with the bits of the properties that were written.
   *
   * `bits` is one accumulator per slot, each carrying at most
   * {@link MASK_BITS} bits — 53, because a double is exact on integers only to
   * 2^53 and the 54th bit of a mask packed into one slot is lost with no
   * rounding a reader could detect.
   */
  patchMask(at: number, bits: readonly number[]): void {
    bits.forEach((slot, offset) => {
      if (slot < 0 || slot > LARGEST_MASK_SLOT || !Number.isInteger(slot)) {
        throw new RangeError(`a mask slot holds ${MASK_BITS} bits; ${slot} is not one of them`)
      }
      this.#slots[at + offset] = slot
    })
  }
}

/** Writes the four slots every arena opens with, and the page count. */
export function writeHeader(out: ArenaWriter, width: number, height: number, scale: number, pages: number): void {
  out.slot(MAGIC)
  out.slot(VERSION)
  out.f32(width)
  out.f32(height)
  out.f32(scale)
  out.count(pages)
}
