//! Reads a date in a response header as Chrome reads `Date`, `Expires` and
//! `Last-Modified`: a month, a day and a year in any order and nearly any
//! spelling, with an optional time and zone. `image-cache.ts`'s `httpDate`
//! reads alike, and `dates.tsv` holds both to the same answers.

/// The most pieces a string may have and still be read: Chrome reads 1001 and
/// refuses 1002 (`dates.tsv`: "1001 pieces are read", "1002 pieces are not").
const MAX_PIECES: usize = 1001;

/// The largest year that is a date: Chrome reads 32767 and refuses 32768
/// (`dates.tsv`: "year 32767", "year 32768").
const MAX_YEAR: i64 = 32767;

/// A one- or two-digit year below this is in the 2000s and from it up in the
/// 1900s (`dates.tsv`: "two-digit 69 is 2069", "two-digit 70 is 1970").
const CENTURY_PIVOT: i64 = 70;

/// The highest month and day a numeric date may name before it is no date.
const LAST_MONTH: i64 = 12;
/// See [`LAST_MONTH`]; also the highest two-digit number read as a day.
const LAST_DAY: i64 = 31;

/// The hour AM and PM count from, and the one they treat specially.
const NOON: i64 = 12;

/// How many fractional digits count, as microseconds.
const FRACTION_DIGITS: usize = 6;

/// Days from 0000-03-01 to 1970-01-01 in the proleptic Gregorian calendar.
const EPOCH_DAYS: i64 = 719_468;
/// Days in a 400-year Gregorian cycle.
const CYCLE_DAYS: i64 = 146_097;

/// The zone words and their offsets from GMT in minutes, by their first three
/// letters. GMT is apart, since an offset may follow it.
const ZONES: [(&[u8; 3], i64); 14] = [
    (b"ast", -4 * 60),
    (b"bst", 60),
    (b"cdt", -5 * 60),
    (b"cst", -6 * 60),
    (b"edt", -4 * 60),
    (b"eet", 2 * 60),
    (b"est", -5 * 60),
    (b"jst", 9 * 60),
    (b"mdt", -6 * 60),
    (b"met", 60),
    (b"mst", -7 * 60),
    (b"nst", -(3 * 60 + 30)),
    (b"pdt", -7 * 60),
    (b"pst", -8 * 60),
];

/// The month words, January first, by their first three letters.
const MONTHS: [&[u8; 3]; 12] = [
    b"jan", b"feb", b"mar", b"apr", b"may", b"jun", b"jul", b"aug", b"sep",
    b"oct", b"nov", b"dec",
];

/// A date in a response header, in milliseconds since the epoch, or `None`
/// when it names no month, day or year, names a year past 32767, or runs to
/// more than 1001 pieces. A missing time is midnight and a missing zone GMT.
pub(crate) fn http_date(value: &str) -> Option<i64> {
    let text = value.as_bytes();
    let mut found = Found::default();
    let mut at = 0;
    let mut pieces = 0;
    while let Some(start) = next_piece(text, at, found.year.is_some()) {
        pieces += 1;
        if pieces > MAX_PIECES {
            return None;
        }
        at = found.piece(text, start);
    }
    found.instant()
}

/// The zone a string is read in.
#[derive(Clone, Copy)]
struct Zone {
    /// Its offset from GMT, in minutes.
    minutes: i64,
    /// Whether a numeric offset may still follow: only after GMT, UT or `Z`.
    open: bool,
}

/// GMT as a zone word, `UT` or a trailing `Z` names it.
const GMT: Zone = Zone {
    minutes: 0,
    open: true,
};

/// A time of day as written, before AM or PM applies.
#[derive(Clone, Copy)]
struct Clock {
    hour: i64,
    minute: i64,
    second: i64,
    micros: i64,
}

/// What the pieces read so far have given. Each field is taken from the first
/// piece that gives it, except that a numeric date replaces a day and a year.
#[derive(Default)]
struct Found {
    month: Option<i64>,
    day: Option<i64>,
    year: Option<i64>,
    clock: Option<Clock>,
    zone: Option<Zone>,
    /// A month word was read, which stops a later numeric date.
    month_word: bool,
    /// A numeric offset was taken, which no later offset or `Z` replaces.
    offset_taken: bool,
}

impl Found {
    /// Reads the piece at `start` and returns where the next search begins.
    fn piece(&mut self, text: &[u8], start: usize) -> usize {
        match text[start] {
            byte if byte.is_ascii_alphabetic() => self.word(text, start),
            byte if byte.is_ascii_digit() => self.number(text, start),
            b'+' | b'-' => self.offset(text, start),
            _ => rest(text, start),
        }
    }

    /// A word, known by its first three letters, or `UT` by exactly two. A `T`
    /// before a digit is a letter here: Chrome reads `PST08` as PST
    /// (`http-cache.tsv`: "reading T and a digit inside a word").
    fn word(&mut self, text: &[u8], start: usize) -> usize {
        let mut end = start;
        while end < text.len() && text[end].is_ascii_alphabetic() {
            end += 1;
        }
        let letters = &text[start..end];
        if letters.len() >= 3 {
            let key = [
                letters[0].to_ascii_lowercase(),
                letters[1].to_ascii_lowercase(),
                letters[2].to_ascii_lowercase(),
            ];
            let month = (1..).zip(MONTHS).find(|(_, name)| **name == key);
            if let Some((number, _)) = month {
                self.month.get_or_insert(number);
                self.month_word = true;
            } else if &key == b"gmt" {
                self.zone.get_or_insert(GMT);
            } else if let Some((_, minutes)) =
                ZONES.iter().find(|(name, _)| **name == key)
            {
                self.zone.get_or_insert(Zone {
                    minutes: *minutes,
                    open: false,
                });
            }
        } else if letters.eq_ignore_ascii_case(b"ut") {
            self.zone.get_or_insert(GMT);
        }
        rest(text, end)
    }

    /// A piece starting with digits: a time, a numeric date, or one number.
    fn number(&mut self, text: &[u8], start: usize) -> usize {
        let end = digits(text, start);
        match text.get(end) {
            Some(b':') => return self.clock(text, start, end),
            Some(b'/' | b'-') if is_digit(text, end + 1) => {
                return self.numeric_date(text, start, end);
            }
            Some(byte) if byte.is_ascii_alphabetic() && !divides(text, end) => {
                return rest(text, end);
            }
            _ => {}
        }
        let count = end - start;
        let value = number(&text[start..end]);
        match count {
            4 | 5 => {
                self.year.get_or_insert(value);
            }
            2 if self.day.is_none() && value <= LAST_DAY => {
                self.day = Some(value);
            }
            2 => {
                self.year.get_or_insert_with(|| short_year(value));
            }
            1 => {
                self.day.get_or_insert(value);
            }
            _ => {}
        }
        rest(text, end)
    }

    /// A time from its hour digits at `start..hour_end`, a colon behind them.
    fn clock(&mut self, text: &[u8], start: usize, hour_end: usize) -> usize {
        let ignored = rest(text, hour_end);
        if hour_end - start > 2 {
            return ignored;
        }
        let minute_end = digits(text, hour_end + 1);
        if !(1..=2).contains(&(minute_end - hour_end - 1)) {
            return ignored;
        }
        let mut at = minute_end;
        let mut second = 0;
        let mut micros = 0;
        if text.get(at) == Some(&b':') {
            let second_end = digits(text, at + 1);
            match second_end - at - 1 {
                0 => at += 1,
                1 | 2 => {
                    second = number(&text[at + 1..second_end]);
                    at = second_end;
                    if text.get(at) == Some(&b'.') {
                        let fraction_end = digits(text, at + 1);
                        micros = fraction(&text[at + 1..fraction_end]);
                        at = fraction_end;
                    }
                }
                _ => return ignored,
            }
        }
        let zulu = text.get(at) == Some(&b'Z');
        if zulu {
            at += 1;
        }
        let mut hour = number(&text[start..hour_end]);
        if hour <= NOON && !zulu {
            let mut marker = at;
            while matches!(text.get(marker), Some(b' ' | b'\t')) {
                marker += 1;
            }
            let pair =
                text.get(marker..marker + 2).map(<[u8]>::to_ascii_lowercase);
            let read = match pair.as_deref() {
                Some(b"am") => Some(hour % NOON),
                Some(b"pm") => Some(hour % NOON + NOON),
                _ => None,
            };
            if let Some(read) = read {
                hour = read;
                at = marker + 2;
            }
        }
        if self.clock.is_none() {
            self.clock = Some(Clock {
                hour,
                minute: number(&text[hour_end + 1..minute_end]),
                second,
                micros,
            });
            if zulu && !self.offset_taken {
                self.zone = Some(GMT);
            }
        }
        rest(text, at)
    }

    /// Three numbers joined by `/` or `-`, the first at `start..first_end`.
    /// One that is no numeric date gives up only its first number.
    fn numeric_date(
        &mut self,
        text: &[u8],
        start: usize,
        first_end: usize,
    ) -> usize {
        let second_end = digits(text, first_end + 1);
        let shaped = matches!(text.get(second_end), Some(b'/' | b'-'))
            && is_digit(text, second_end + 1);
        if !shaped || self.month_word {
            return first_end;
        }
        let third_end = digits(text, second_end + 1);
        let widths = (
            first_end - start,
            second_end - first_end - 1,
            third_end - second_end - 1,
        );
        let ends = third_end == text.len()
            || is_separator(text[third_end])
            || divides(text, third_end);
        if !matches!(widths, (1 | 2 | 4, 1 | 2, 1 | 2 | 4 | 5)) || !ends {
            return first_end;
        }
        let first = number(&text[start..first_end]);
        let second = number(&text[first_end + 1..second_end]);
        let third = number(&text[second_end + 1..third_end]);
        let (year, month, day) = if first > LAST_DAY || first == 0 {
            if second > LAST_MONTH || third > LAST_DAY {
                return first_end;
            }
            (year_of(first, widths.0), second, third)
        } else if first > LAST_MONTH && second > LAST_MONTH {
            return third_end;
        } else if first > LAST_MONTH {
            (year_of(third, widths.2), second, first)
        } else {
            (year_of(third, widths.2), first, second)
        };
        self.year = Some(year);
        self.month = Some(month);
        self.day = Some(day);
        third_end
    }

    /// A sign and its digits: hours, or hours and minutes, from GMT.
    fn offset(&mut self, text: &[u8], start: usize) -> usize {
        let end = digits(text, start + 1);
        let value = number(&text[start + 1..end]);
        let minutes = match end - start - 1 {
            1 | 2 => value * 60,
            4 => value / 100 * 60 + value % 100,
            _ => return rest(text, start + 1),
        };
        let open = self.zone.is_none_or(|zone| zone.open);
        if !self.offset_taken && open {
            let sign = if text[start] == b'-' { -1 } else { 1 };
            self.zone = Some(Zone {
                minutes: sign * minutes,
                open: false,
            });
            self.offset_taken = true;
        }
        rest(text, start + 1)
    }

    /// The instant the fields name, carrying any value past its range into the
    /// next unit up, as a calendar does.
    fn instant(&self) -> Option<i64> {
        let (Some(month), Some(day), Some(year)) =
            (self.month, self.day, self.year)
        else {
            return None;
        };
        if year > MAX_YEAR {
            return None;
        }
        let year = year + (month - 1).div_euclid(12);
        let month = (month - 1).rem_euclid(12) + 1;
        let days = first_of_month(year, month) + day - 1;
        let clock = self.clock.unwrap_or(Clock {
            hour: 0,
            minute: 0,
            second: 0,
            micros: 0,
        });
        let minutes = (days * 24 + clock.hour) * 60 + clock.minute
            - self.zone.map_or(0, |zone| zone.minutes);
        Some((minutes * 60 + clock.second) * 1000 + clock.micros / 1000)
    }
}

/// Where the first piece at or after `at` starts, skipping separators: the
/// listed ones, a `T` before a digit, and a dash that joins nothing.
fn next_piece(text: &[u8], mut at: usize, has_year: bool) -> Option<usize> {
    while at < text.len() {
        let byte = text[at];
        let skipped = is_separator(byte)
            || divides(text, at)
            || byte == b'-'
                && (!is_digit(text, at + 1)
                    || at > 0
                        && text[at - 1].is_ascii_alphabetic()
                        && !has_year);
        if !skipped {
            return Some(at);
        }
        at += 1;
    }
    None
}

/// Where the piece containing `at` ends: at a separator, a sign, or a `T`
/// before a digit. What lies between is ignored.
fn rest(text: &[u8], mut at: usize) -> usize {
    while at < text.len()
        && !is_separator(text[at])
        && !matches!(text[at], b'+' | b'-')
        && !divides(text, at)
    {
        at += 1;
    }
    at
}

/// The characters that only separate pieces.
const fn is_separator(byte: u8) -> bool {
    matches!(
        byte,
        b' ' | b'\t' | b',' | b';' | b'/' | b'(' | b')' | b'[' | b']'
    )
}

/// Whether `at` is an upper-case `T` directly before a digit, which ends a
/// piece outside a word.
fn divides(text: &[u8], at: usize) -> bool {
    text.get(at) == Some(&b'T') && is_digit(text, at + 1)
}

/// Whether the byte at `at` is an ASCII digit, `false` past the end.
fn is_digit(text: &[u8], at: usize) -> bool {
    text.get(at).is_some_and(u8::is_ascii_digit)
}

/// Where the run of digits from `at` ends.
fn digits(text: &[u8], mut at: usize) -> usize {
    while is_digit(text, at) {
        at += 1;
    }
    at
}

/// The value of a run of digits. Saturates, since a run too long for any field
/// is still read before its length rules it out.
fn number(run: &[u8]) -> i64 {
    run.iter().fold(0_i64, |value, digit| {
        value
            .saturating_mul(10)
            .saturating_add(i64::from(digit - b'0'))
    })
}

/// A fraction of a second as microseconds, from its first six digits.
fn fraction(run: &[u8]) -> i64 {
    let kept = &run[..run.len().min(FRACTION_DIGITS)];
    (kept.len()..FRACTION_DIGITS).fold(number(kept), |value, _| value * 10)
}

/// A one- or two-digit year in its century.
const fn short_year(value: i64) -> i64 {
    if value < CENTURY_PIVOT {
        2000 + value
    } else {
        1900 + value
    }
}

/// A year as written with `width` digits.
const fn year_of(value: i64, width: usize) -> i64 {
    if width <= 2 { short_year(value) } else { value }
}

/// Days from the epoch to the first of `month` in `year`, proleptic
/// Gregorian, counting years from 1 March so the leap day falls last.
const fn first_of_month(year: i64, month: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let cycle = year.div_euclid(400);
    let year_of_cycle = year - cycle * 400;
    let month_from_march = (month + 9) % 12;
    let day_of_year = (153 * month_from_march + 2) / 5;
    let day_of_cycle = year_of_cycle * 365 + year_of_cycle / 4
        - year_of_cycle / 100
        + day_of_year;
    cycle * CYCLE_DAYS + day_of_cycle - EPOCH_DAYS
}
