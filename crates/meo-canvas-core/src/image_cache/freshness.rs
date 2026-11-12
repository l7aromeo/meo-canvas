//! When a cached response may be used without asking the origin again: RFC
//! 9111's freshness, as Chrome applies it in `http-cache.tsv`, plus RFC 5861's
//! `stale-if-error`, which Chrome does not implement. The npm surface
//! has the same rules in `image-cache.ts`, and `freshness.tsv` holds both to
//! the same answers.
//!
//! Every time here is milliseconds since the Unix epoch, signed, so an age
//! larger than a lifetime gives an expiry before the response rather than an
//! overflow.

use super::http_date::http_date;

/// The longest lifetime a response can claim, in seconds: RFC 9111 §1.2.2
/// tells a cache to read any larger `delta-seconds` as 2^31.
const MAX_DELTA_SECONDS: u64 = 1 << 31;

/// The share of a response's age since `Last-Modified` it stays fresh for when
/// nothing else says: RFC 9111 §4.2.2's "10%", as Chrome uses it
/// (`http-cache.tsv`: "heuristic, age 90 of 100" and "age 110 of 100").
const HEURISTIC_DIVISOR: i64 = 10;

/// Milliseconds in a second.
const MS_PER_SECOND: i64 = 1000;

/// What one response decided about its own caching, at the time it arrived.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Freshness {
    /// Whether it may be stored at all: not `no-store`.
    pub(crate) storable: bool,
    /// Fresh until this instant; at it and past it, revalidate.
    pub(crate) expires_at: i64,
    /// A failed revalidation may serve the entry until this instant.
    pub(crate) stale_if_error_until: i64,
    /// `no-cache`: revalidate on every use, however fresh.
    pub(crate) revalidate_always: bool,
}

/// The cache settings the rules read.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Settings {
    /// The lifetime of a response that names none, replacing the heuristic.
    pub(crate) fallback_ttl: Option<i64>,
    /// How long past expiry a failed revalidation may serve the entry, when
    /// the response names no `stale-if-error` of its own.
    pub(crate) stale_if_error: i64,
}

/// Response headers by lower-cased name. A name sent twice reads as one
/// comma-joined value, which is what HTTP defines it to mean.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Headers<'a>(pub(crate) &'a [(String, String)]);

impl Headers<'_> {
    /// Every value sent under `name`, joined, or `None` if it was not sent.
    pub(crate) fn get(&self, name: &str) -> Option<String> {
        let mut found: Option<String> = None;
        for (held, value) in self.0 {
            if held.eq_ignore_ascii_case(name) {
                match found.as_mut() {
                    Some(joined) => {
                        joined.push_str(", ");
                        joined.push_str(value);
                    }
                    None => found = Some(value.clone()),
                }
            }
        }
        found
    }
}

/// One `Cache-Control` or `Pragma` directive: a lower-cased name, and its
/// value as written if it had one.
fn directives(value: Option<&str>) -> Vec<(String, Option<String>)> {
    value
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(|item| match item.split_once('=') {
            Some((name, value)) => (
                name.trim().to_ascii_lowercase(),
                Some(value.trim().to_owned()),
            ),
            None => (item.to_ascii_lowercase(), None),
        })
        .collect()
}

/// A `delta-seconds` value: digits only, capped at [`MAX_DELTA_SECONDS`].
/// Anything else is not one, and the directive carrying it is ignored.
fn delta_seconds(value: &str) -> Option<i64> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let seconds = value
        .bytes()
        .try_fold(0_u64, |total, digit| {
            total
                .checked_mul(10)
                .and_then(|total| total.checked_add(u64::from(digit - b'0')))
        })
        .unwrap_or(u64::MAX)
        .min(MAX_DELTA_SECONDS);
    // Exact: `MAX_DELTA_SECONDS` times a thousand is far inside `i64`.
    Some(i64::try_from(seconds).ok()? * MS_PER_SECOND)
}

/// The first directive named `name` that carries a valid `delta-seconds`.
fn seconds_of(found: &[(String, Option<String>)], name: &str) -> Option<i64> {
    found
        .iter()
        .filter(|(held, _)| held == name)
        .find_map(|(_, value)| value.as_deref().and_then(delta_seconds))
}

/// Whether a bare directive `name` is present. `no-cache="set-cookie"` is not
/// `no-cache`, as in Chrome (`http-cache.tsv`: `no-cache="set-cookie" with
/// max-age`).
fn has(found: &[(String, Option<String>)], name: &str) -> bool {
    found
        .iter()
        .any(|(held, value)| held == name && value.is_none())
}

/// Freshness for a response requested at `request` and received at
/// `response`, per RFC 9111 §4.2 as Chrome applies it:
///
/// - lifetime: `max-age`, else `Expires` less `Date`, else the caller's
///   fallback, else a tenth of `Date` less `Last-Modified` when that is not
///   negative, else zero; `no-cache` (or `Pragma: no-cache`) makes it zero and
///   revalidates always, and `must-revalidate` turns the heuristic off;
/// - age at arrival: the larger of `response` less `Date` and `Age` plus the
///   round trip, so an `Age` past `max-age` arrives already expired;
/// - stale-if-error: none under `must-revalidate` or `no-cache`, else the
///   response's own `stale-if-error`, else the caller's.
///
/// `Expires: 0` is already expired, and any other `Expires` that is no date,
/// by [`http_date`], falls through to the heuristic, where RFC 9111 reads
/// every invalid one as expired.
///
/// Where RFC 9111 leaves a choice, each is Chrome's, in `http-cache.tsv`:
/// `Pragma: no-cache, no Cache-Control`, `heuristic under must-revalidate`,
/// `heuristic with Last-Modified after Date`, `Expires: 0 with Last-Modified`
/// and `Expires: soon with Last-Modified`.
pub(crate) fn freshness(
    headers: Headers<'_>,
    request: i64,
    response: i64,
    settings: Settings,
) -> Freshness {
    let control = directives(headers.get("cache-control").as_deref());
    let pragma = directives(headers.get("pragma").as_deref());
    let no_cache = has(&control, "no-cache") || has(&pragma, "no-cache");
    let must_revalidate = has(&control, "must-revalidate");
    let date = headers
        .get("date")
        .as_deref()
        .and_then(http_date)
        .unwrap_or(response);

    let lifetime = if no_cache {
        0
    } else if let Some(max_age) = seconds_of(&control, "max-age") {
        max_age
    } else if headers
        .get("expires")
        .is_some_and(|value| value.trim() == "0")
    {
        0
    } else if let Some(expires) =
        headers.get("expires").as_deref().and_then(http_date)
    {
        (expires - date).max(0)
    } else if let Some(fallback) = settings.fallback_ttl {
        fallback
    } else {
        headers
            .get("last-modified")
            .as_deref()
            .and_then(http_date)
            .filter(|modified| !must_revalidate && *modified <= date)
            .map_or(0, |modified| (date - modified) / HEURISTIC_DIVISOR)
    };

    let age = headers
        .get("age")
        .as_deref()
        .and_then(|value| delta_seconds(value.trim()))
        .unwrap_or(0);
    let apparent = (response - date).max(0);
    let initial = apparent.max(age + (response - request));
    let expires_at = response + lifetime - initial;

    let stale_if_error_until = if no_cache || must_revalidate {
        expires_at
    } else {
        expires_at
            + seconds_of(&control, "stale-if-error")
                .unwrap_or(settings.stale_if_error)
    };

    Freshness {
        storable: !has(&control, "no-store"),
        expires_at,
        stale_if_error_until,
        revalidate_always: no_cache,
    }
}

/// Whether an entry stored at `stored_at` may be served at `now` with no
/// request. One stored in the future means the clock moved back, and it is
/// expired rather than fresh for longer.
pub(crate) const fn is_fresh(
    stored_at: i64,
    expires_at: i64,
    revalidate_always: bool,
    now: i64,
) -> bool {
    !revalidate_always && stored_at <= now && now < expires_at
}

/// Whether a failed revalidation at `now` may serve the entry. A cache whose
/// own `stale_if_error` is zero never does, whatever the entry recorded.
pub(crate) const fn stale_allowed(
    stale_if_error_until: i64,
    setting: i64,
    now: i64,
) -> bool {
    setting != 0 && now < stale_if_error_until
}

/// The request headers that make a revalidation conditional.
pub(crate) fn conditional(
    etag: Option<&str>,
    last_modified: Option<&str>,
) -> Vec<(String, String)> {
    let mut headers = Vec::new();
    if let Some(etag) = etag {
        headers.push(("if-none-match".to_owned(), etag.to_owned()));
    }
    if let Some(modified) = last_modified {
        headers.push(("if-modified-since".to_owned(), modified.to_owned()));
    }
    headers
}

#[cfg(test)]
mod tests {
    use super::{
        Headers, Settings, conditional, freshness, http_date, is_fresh,
        stale_allowed,
    };

    /// The table's zero: Sun, 01 Nov 2026 00:00:00 GMT.
    const EPOCH: i64 = 1_793_491_200_000;

    /// The stale-if-error a cache uses when its caller names none.
    const DEFAULT_STALE: std::time::Duration =
        crate::image_cache::DEFAULT_STALE_IF_ERROR;

    const TABLE: &str =
        include_str!("../../tests/assets/image-cache/freshness.tsv");

    /// Every row of `freshness.tsv`, through the functions the cache calls.
    #[test]
    fn every_row_of_the_shared_table() {
        let cell = |value: &'static str| (value != "-").then_some(value);
        let yes = |value: &str| {
            assert!(value == "yes" || value == "no", "{value:?}");
            value == "yes"
        };
        let ms = |value: &str| -> i64 {
            value
                .parse()
                .unwrap_or_else(|_| unreachable!("not an integer: {value}"))
        };
        let mut rows = TABLE.lines().filter(|line| !line.starts_with('#'));
        let header = rows.next().unwrap_or_default();
        assert!(header.starts_with("case\trequest"));
        let mut count = 0;
        for line in rows {
            let c: Vec<&str> = line.split('\t').collect();
            assert_eq!(c.len(), 15, "{line}");
            let headers: Vec<(String, String)> = cell(c[6])
                .map(|items| {
                    items
                        .split(" | ")
                        .map(|item| {
                            let (name, value) =
                                item.split_once(": ").unwrap_or_else(|| {
                                    unreachable!("not `name: value`: {item}")
                                });
                            (name.to_owned(), value.to_owned())
                        })
                        .collect()
                })
                .unwrap_or_default();
            let settings = Settings {
                fallback_ttl: cell(c[4]).map(ms),
                stale_if_error: cell(c[5]).map_or_else(
                    || crate::image_cache::duration_ms(DEFAULT_STALE),
                    ms,
                ),
            };
            let (request, response, now) =
                (EPOCH + ms(c[1]), EPOCH + ms(c[2]), EPOCH + ms(c[3]));
            let found =
                freshness(Headers(&headers), request, response, settings);
            assert_eq!(found.storable, yes(c[7]), "{}: storable", c[0]);
            count += 1;
            if !found.storable {
                continue;
            }
            assert_eq!(found.expires_at - EPOCH, ms(c[8]), "{}: expires", c[0]);
            assert_eq!(
                found.stale_if_error_until - EPOCH,
                ms(c[9]),
                "{}: stale-if-error until",
                c[0]
            );
            assert_eq!(found.revalidate_always, yes(c[10]), "{}", c[0]);
            assert_eq!(
                is_fresh(
                    response,
                    found.expires_at,
                    found.revalidate_always,
                    now
                ),
                yes(c[11]),
                "{}: fresh",
                c[0]
            );
            assert_eq!(
                stale_allowed(
                    found.stale_if_error_until,
                    settings.stale_if_error,
                    now
                ),
                yes(c[12]),
                "{}: stale allowed",
                c[0]
            );
            let headers = Headers(&headers);
            let sent = conditional(
                headers.get("etag").as_deref(),
                headers.get("last-modified").as_deref(),
            );
            let named = |name: &str| {
                sent.iter()
                    .find(|(held, _)| held == name)
                    .map(|(_, value)| value.as_str())
            };
            assert_eq!(named("if-none-match"), cell(c[13]), "{}", c[0]);
            assert_eq!(named("if-modified-since"), cell(c[14]), "{}", c[0]);
        }
        assert_eq!(count, 44, "every row was read");
    }

    /// Every row of `dates.tsv`, the dates both surfaces read alike.
    #[test]
    fn every_row_of_the_shared_date_table() {
        let table = include_str!("../../tests/assets/image-cache/dates.tsv");
        let mut count = 0;
        for line in table.lines().filter(|line| !line.starts_with('#')).skip(1)
        {
            let c: Vec<&str> = line.split('\t').collect();
            assert_eq!(c.len(), 3, "{line}");
            let want = (c[2] != "-").then(|| {
                c[2].parse::<i64>().unwrap_or_else(|_| {
                    unreachable!("not milliseconds: {line}")
                })
            });
            assert_eq!(http_date(c[1]), want, "{}: {:?}", c[0], c[1]);
            count += 1;
        }
        assert_eq!(count, 141, "every row was read");
    }
}
