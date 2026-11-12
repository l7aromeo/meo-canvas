/**
 * Reads a date in a response header as Chrome reads `Date`, `Expires` and
 * `Last-Modified`: a month, a day and a year in any order and nearly any
 * spelling, with an optional time and zone. `meo-canvas-core`'s `http_date`
 * reads alike, and `dates.tsv` holds both to the same answers.
 * @packageDocumentation
 */

/**
 * The most pieces a string may have and still be read: Chrome reads 1001 and
 * refuses 1002 (`dates.tsv`: "1001 pieces are read", "1002 pieces are not").
 */
const MAX_PIECES = 1001

/**
 * The largest year that is a date: Chrome reads 32767 and refuses 32768
 * (`dates.tsv`: "year 32767", "year 32768").
 */
const MAX_YEAR = 32767

/**
 * A one- or two-digit year below this is in the 2000s and from it up in the
 * 1900s (`dates.tsv`: "two-digit 69 is 2069", "two-digit 70 is 1970").
 */
const CENTURY_PIVOT = 70

/** The highest month a numeric date may name before it is no date. */
const LAST_MONTH = 12
/** The highest day a numeric date may name; also the highest two-digit day. */
const LAST_DAY = 31

/** The hour AM and PM count from, and the one they treat specially. */
const NOON = 12

/** How many fractional digits count, as microseconds. */
const FRACTION_DIGITS = 6

/** Days from 0000-03-01 to 1970-01-01 in the proleptic Gregorian calendar. */
const EPOCH_DAYS = 719_468
/** Days in a 400-year Gregorian cycle. */
const CYCLE_DAYS = 146_097

/**
 * The zone words and their offsets from GMT in minutes, by their first three
 * letters. GMT is apart, since an offset may follow it.
 */
const ZONES: ReadonlyMap<string, number> = new Map([
  ['ast', -4 * 60],
  ['bst', 60],
  ['cdt', -5 * 60],
  ['cst', -6 * 60],
  ['edt', -4 * 60],
  ['eet', 2 * 60],
  ['est', -5 * 60],
  ['jst', 9 * 60],
  ['mdt', -6 * 60],
  ['met', 60],
  ['mst', -7 * 60],
  ['nst', -(3 * 60 + 30)],
  ['pdt', -7 * 60],
  ['pst', -8 * 60],
])

/** The month words, January first, by their first three letters. */
const MONTHS = ['jan', 'feb', 'mar', 'apr', 'may', 'jun', 'jul', 'aug', 'sep', 'oct', 'nov', 'dec']

/** The character codes the reader branches on. */
const CODE = {
  space: 0x20,
  tab: 0x09,
  comma: 0x2c,
  semicolon: 0x3b,
  slash: 0x2f,
  openParen: 0x28,
  closeParen: 0x29,
  openBracket: 0x5b,
  closeBracket: 0x5d,
  plus: 0x2b,
  minus: 0x2d,
  colon: 0x3a,
  dot: 0x2e,
  zero: 0x30,
  nine: 0x39,
  upperT: 0x54,
  upperZ: 0x5a,
} as const

/** The zone a string is read in. */
interface Zone {
  /** Its offset from GMT, in minutes. */
  readonly minutes: number
  /** Whether a numeric offset may still follow: only after GMT, UT or `Z`. */
  readonly open: boolean
}

/** GMT as a zone word, `UT` or a trailing `Z` names it. */
const GMT: Zone = { minutes: 0, open: true }

/** A time of day, after AM or PM applies. */
interface Clock {
  readonly hour: number
  readonly minute: number
  readonly second: number
  readonly micros: number
}

/**
 * A date in a response header, in milliseconds since the epoch, or `undefined`
 * when there is no header or it names no month, day or year, names a year past
 * 32767, or runs to more than 1001 pieces. A missing time is midnight and a
 * missing zone GMT.
 * @internal
 */
export function httpDate(value: string | null): number | undefined {
  if (value === null) return undefined
  const found = new Found(value)
  let at = 0
  let pieces = 0
  for (let start = nextPiece(value, at, false); start !== undefined; start = nextPiece(value, at, found.year !== undefined)) {
    pieces += 1
    if (pieces > MAX_PIECES) return undefined
    at = found.piece(start)
  }
  return found.instant()
}

/**
 * What the pieces read so far have given. Each field is taken from the first
 * piece that gives it, except that a numeric date replaces a day and a year.
 */
class Found {
  month: number | undefined = undefined
  day: number | undefined = undefined
  year: number | undefined = undefined
  clock: Clock | undefined = undefined
  zone: Zone | undefined = undefined
  /** A month word was read, which stops a later numeric date. */
  monthWord = false
  /** A numeric offset was taken, which no later offset or `Z` replaces. */
  offsetTaken = false
  /** The string being read. */
  readonly text: string

  constructor(text: string) {
    this.text = text
  }

  /** Reads the piece at `start` and returns where the next search begins. */
  piece(start: number): number {
    const code = this.text.charCodeAt(start)
    if (isLetter(code)) return this.word(start)
    if (isDigitCode(code)) return this.number(start)
    if (code === CODE.plus || code === CODE.minus) return this.offset(start)
    return rest(this.text, start)
  }

  /**
   * A word, known by its first three letters, or `UT` by exactly two. A `T`
   * before a digit is a letter here: Chrome reads `PST08` as PST
   * (`http-cache.tsv`: "reading T and a digit inside a word").
   */
  word(start: number): number {
    const text = this.text
    let end = start
    while (end < text.length && isLetter(text.charCodeAt(end))) end += 1
    const letters = text.slice(start, end).toLowerCase()
    if (letters.length >= 3) {
      const key = letters.slice(0, 3)
      const month = MONTHS.indexOf(key)
      if (month >= 0) {
        this.month ??= month + 1
        this.monthWord = true
      } else if (key === 'gmt') {
        this.zone ??= GMT
      } else {
        const minutes = ZONES.get(key)
        if (minutes !== undefined) this.zone ??= { minutes, open: false }
      }
    } else if (letters === 'ut') {
      this.zone ??= GMT
    }
    return rest(text, end)
  }

  /** A piece starting with digits: a time, a numeric date, or one number. */
  number(start: number): number {
    const text = this.text
    const end = digits(text, start)
    const next = text.charCodeAt(end)
    if (next === CODE.colon) return this.time(start, end)
    if ((next === CODE.slash || next === CODE.minus) && isDigit(text, end + 1)) return this.numericDate(start, end)
    if (isLetter(next) && !divides(text, end)) return rest(text, end)

    const count = end - start
    const value = number(text, start, end)
    if (count === 4 || count === 5) {
      this.year ??= value
    } else if (count === 2 && this.day === undefined && value <= LAST_DAY) {
      this.day = value
    } else if (count === 2) {
      this.year ??= shortYear(value)
    } else if (count === 1) {
      this.day ??= value
    }
    return rest(text, end)
  }

  /** A time from its hour digits at `start..hourEnd`, a colon behind them. */
  time(start: number, hourEnd: number): number {
    const text = this.text
    const ignored = rest(text, hourEnd)
    if (hourEnd - start > 2) return ignored
    const minuteEnd = digits(text, hourEnd + 1)
    const minuteDigits = minuteEnd - hourEnd - 1
    if (minuteDigits < 1 || minuteDigits > 2) return ignored

    let at = minuteEnd
    let second = 0
    let micros = 0
    if (text.charCodeAt(at) === CODE.colon) {
      const secondEnd = digits(text, at + 1)
      const secondDigits = secondEnd - at - 1
      if (secondDigits === 0) {
        at += 1
      } else if (secondDigits <= 2) {
        second = number(text, at + 1, secondEnd)
        at = secondEnd
        if (text.charCodeAt(at) === CODE.dot) {
          const fractionEnd = digits(text, at + 1)
          micros = fraction(text, at + 1, fractionEnd)
          at = fractionEnd
        }
      } else {
        return ignored
      }
    }

    const zulu = text.charCodeAt(at) === CODE.upperZ
    if (zulu) at += 1
    let hour = number(text, start, hourEnd)
    if (hour <= NOON && !zulu) {
      let marker = at
      while (text.charCodeAt(marker) === CODE.space || text.charCodeAt(marker) === CODE.tab) marker += 1
      const pair = text.slice(marker, marker + 2).toLowerCase()
      if (pair === 'am' || pair === 'pm') {
        hour = pair === 'am' ? hour % NOON : (hour % NOON) + NOON
        at = marker + 2
      }
    }

    if (this.clock === undefined) {
      this.clock = { hour, minute: number(text, hourEnd + 1, minuteEnd), second, micros }
      if (zulu && !this.offsetTaken) this.zone = GMT
    }
    return rest(text, at)
  }

  /**
   * Three numbers joined by `/` or `-`, the first at `start..firstEnd`. One that
   * is no numeric date gives up only its first number.
   */
  numericDate(start: number, firstEnd: number): number {
    const text = this.text
    const secondEnd = digits(text, firstEnd + 1)
    const joiner = text.charCodeAt(secondEnd)
    const shaped = (joiner === CODE.slash || joiner === CODE.minus) && isDigit(text, secondEnd + 1)
    if (!shaped || this.monthWord) return firstEnd

    const thirdEnd = digits(text, secondEnd + 1)
    const widths = [firstEnd - start, secondEnd - firstEnd - 1, thirdEnd - secondEnd - 1] as const
    const ends = thirdEnd === text.length || isSeparator(text.charCodeAt(thirdEnd)) || divides(text, thirdEnd)
    const fits = [1, 2, 4].includes(widths[0]) && [1, 2].includes(widths[1]) && [1, 2, 4, 5].includes(widths[2])
    if (!fits || !ends) return firstEnd

    const first = number(text, start, firstEnd)
    const second = number(text, firstEnd + 1, secondEnd)
    const third = number(text, secondEnd + 1, thirdEnd)
    let date: readonly [number, number, number]
    if (first > LAST_DAY || first === 0) {
      if (second > LAST_MONTH || third > LAST_DAY) return firstEnd
      date = [yearOf(first, widths[0]), second, third]
    } else if (first > LAST_MONTH && second > LAST_MONTH) {
      return thirdEnd
    } else if (first > LAST_MONTH) {
      date = [yearOf(third, widths[2]), second, first]
    } else {
      date = [yearOf(third, widths[2]), first, second]
    }
    ;[this.year, this.month, this.day] = date
    return thirdEnd
  }

  /** A sign and its digits: hours, or hours and minutes, from GMT. */
  offset(start: number): number {
    const text = this.text
    const end = digits(text, start + 1)
    const count = end - start - 1
    const value = number(text, start + 1, end)
    let minutes: number
    if (count === 1 || count === 2) minutes = value * 60
    else if (count === 4) minutes = Math.trunc(value / 100) * 60 + (value % 100)
    else return rest(text, start + 1)

    const open = this.zone?.open ?? true
    if (!this.offsetTaken && open) {
      const sign = text.charCodeAt(start) === CODE.minus ? -1 : 1
      this.zone = { minutes: sign * minutes, open: false }
      this.offsetTaken = true
    }
    return rest(text, start + 1)
  }

  /**
   * The instant the fields name, carrying any value past its range into the
   * next unit up, as a calendar does.
   */
  instant(): number | undefined {
    const { month, day, year } = this
    if (month === undefined || day === undefined || year === undefined) return undefined
    if (year > MAX_YEAR) return undefined
    const carried = year + Math.floor((month - 1) / 12)
    const days = firstOfMonth(carried, mod(month - 1, 12) + 1) + day - 1
    const clock = this.clock ?? { hour: 0, minute: 0, second: 0, micros: 0 }
    const minutes = (days * 24 + clock.hour) * 60 + clock.minute - (this.zone?.minutes ?? 0)
    return (minutes * 60 + clock.second) * 1000 + Math.floor(clock.micros / 1000)
  }
}

/**
 * Where the first piece at or after `at` starts, skipping separators: the
 * listed ones, a `T` before a digit, and a dash that joins nothing.
 */
function nextPiece(text: string, at: number, hasYear: boolean): number | undefined {
  for (; at < text.length; at += 1) {
    const code = text.charCodeAt(at)
    const joinsNothing = code === CODE.minus && (!isDigit(text, at + 1) || (at > 0 && isLetter(text.charCodeAt(at - 1)) && !hasYear))
    if (!isSeparator(code) && !divides(text, at) && !joinsNothing) return at
  }
  return undefined
}

/**
 * Where the piece containing `at` ends: at a separator, a sign, or a `T`
 * before a digit. What lies between is ignored.
 */
function rest(text: string, at: number): number {
  while (at < text.length) {
    const code = text.charCodeAt(at)
    if (isSeparator(code) || code === CODE.plus || code === CODE.minus || divides(text, at)) break
    at += 1
  }
  return at
}

/** The characters that only separate pieces. */
function isSeparator(code: number): boolean {
  return (
    code === CODE.space ||
    code === CODE.tab ||
    code === CODE.comma ||
    code === CODE.semicolon ||
    code === CODE.slash ||
    code === CODE.openParen ||
    code === CODE.closeParen ||
    code === CODE.openBracket ||
    code === CODE.closeBracket
  )
}

/** Whether `at` is an upper-case `T` directly before a digit, which ends a piece outside a word. */
function divides(text: string, at: number): boolean {
  return text.charCodeAt(at) === CODE.upperT && isDigit(text, at + 1)
}

/** An ASCII letter; no other letter counts. */
function isLetter(code: number): boolean {
  return (code >= 0x41 && code <= 0x5a) || (code >= 0x61 && code <= 0x7a)
}

/** An ASCII digit; no other digit counts. */
function isDigitCode(code: number): boolean {
  return code >= CODE.zero && code <= CODE.nine
}

/** Whether the character at `at` is an ASCII digit, `false` past the end. */
function isDigit(text: string, at: number): boolean {
  return isDigitCode(text.charCodeAt(at))
}

/** Where the run of digits from `at` ends. */
function digits(text: string, at: number): number {
  while (isDigit(text, at)) at += 1
  return at
}

/** The value of the digits at `start..end`. */
function number(text: string, start: number, end: number): number {
  let value = 0
  for (let at = start; at < end; at += 1) value = value * 10 + (text.charCodeAt(at) - CODE.zero)
  return value
}

/** A fraction of a second as microseconds, from its first six digits. */
function fraction(text: string, start: number, end: number): number {
  const kept = Math.min(end - start, FRACTION_DIGITS)
  return number(text, start, start + kept) * 10 ** (FRACTION_DIGITS - kept)
}

/** A one- or two-digit year in its century. */
function shortYear(value: number): number {
  return value < CENTURY_PIVOT ? 2000 + value : 1900 + value
}

/** A year as written with `width` digits. */
function yearOf(value: number, width: number): number {
  return width <= 2 ? shortYear(value) : value
}

/** The remainder that is never negative. */
function mod(value: number, by: number): number {
  return ((value % by) + by) % by
}

/**
 * Days from the epoch to the first of `month` in `year`, proleptic Gregorian,
 * counting years from 1 March so the leap day falls last.
 */
function firstOfMonth(year: number, month: number): number {
  const shifted = month <= 2 ? year - 1 : year
  const cycle = Math.floor(shifted / 400)
  const yearOfCycle = shifted - cycle * 400
  const dayOfYear = Math.trunc((153 * ((month + 9) % 12) + 2) / 5)
  const dayOfCycle = yearOfCycle * 365 + Math.trunc(yearOfCycle / 4) - Math.trunc(yearOfCycle / 100) + dayOfYear
  return cycle * CYCLE_DAYS + dayOfCycle - EPOCH_DAYS
}
