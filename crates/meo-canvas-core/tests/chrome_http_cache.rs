//! The image cache against Chrome's HTTP cache (`http-cache.tsv`): every row's
//! responses replayed through `ImageCache`, one render per load, comparing what
//! the origin counted and what each request carried with what Chrome sent.
#![cfg(feature = "net")]

use std::{
    fmt::Write as _,
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    num::NonZeroU64,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use meo_canvas_core::{
    Renderer,
    image_cache::{ImageCache, MemoryStore},
};
use meo_canvas_scene::{
    Length, Scene, Size,
    node::{ImageSource, Node, NodeKind},
    style::paint::ObjectFit,
};

const TABLE: &str =
    include_str!("../../meo-canvas/tests/assets/chrome/http-cache.tsv");

/// The rows this cache answers differently from Chrome. None: the dates are
/// read as Chrome reads them and a 304's headers merge as Chrome merges them.
/// Date forms beyond the table are not measured, and a difference there
/// would show as a row added to `http-cache.tsv` failing here.
const APART: &[&str] = &[];

/// Each zone's offset from GMT in minutes, as `httpcache.mjs` writes it.
const ZONE_MINUTES: &[(&str, i64)] = &[
    ("EST", -300),
    ("PST", -480),
    ("EDT", -240),
    ("JST", 540),
    ("MET", 60),
    ("BST", 60),
    ("+0130", 90),
    ("-0800", -480),
    ("GMT+0300", 180),
];

/// A 1x1 PNG.
const DOT: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D,
    0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,
    0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00,
    0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0xF8, 0xCF, 0xC0, 0x00,
    0x00, 0x03, 0x01, 0x01, 0x00, 0x18, 0xDD, 0x8D, 0xB0, 0x00, 0x00, 0x00,
    0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

/// One row: its name, its responses, how many loads, and what Chrome did.
struct Row<'a> {
    name: &'a str,
    responses: Vec<&'a str>,
    loads: usize,
    requests: usize,
    sent: &'a str,
}

fn rows() -> Vec<Row<'static>> {
    TABLE
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .map(|line| {
            let c: Vec<&str> = line.split('\t').collect();
            assert_eq!(c.len(), 6, "{line}");
            Row {
                name: c[0],
                responses: c[1].split(" || ").collect(),
                loads: c[2]
                    .parse()
                    .unwrap_or_else(|_| unreachable!("loads: {line}")),
                requests: c[3]
                    .parse()
                    .unwrap_or_else(|_| unreachable!("requests: {line}")),
                sent: c[5],
            }
        })
        .collect()
}

/// `seconds` since the epoch as an IMF-fixdate.
fn http_date(seconds: i64) -> String {
    const DAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct",
        "Nov", "Dec",
    ];
    let (days, rest) = (seconds.div_euclid(86_400), seconds.rem_euclid(86_400));
    // Howard Hinnant's `civil_from_days`.
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524
        - day_of_era / 146_096)
        / 365;
    let day_of_year =
        day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!(
        "{}, {day:02} {} {year} {:02}:{:02}:{:02} GMT",
        DAYS[days.rem_euclid(7) as usize],
        MONTHS[(month - 1) as usize],
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

/// `value` with each `@-90` resolved against `now`, and `@+7200~EST` written
/// as that instant's wall clock in the zone, as `httpcache.mjs` writes them.
fn resolved(value: &str, now: i64) -> String {
    let Some((before, after)) = value.split_once('@') else {
        return value.to_owned();
    };
    let end = after
        .char_indices()
        .find(|&(at, c)| !(c.is_ascii_digit() || (at == 0 && "+-".contains(c))))
        .map_or(after.len(), |(at, _)| at);
    let offset: i64 = after[..end].parse().unwrap_or(0);
    let (zone, rest) =
        after[end..]
            .strip_prefix('~')
            .map_or((None, &after[end..]), |zoned| {
                let stop = zoned.find(' ').unwrap_or(zoned.len());
                (Some(&zoned[..stop]), &zoned[stop..])
            });
    let written = zone.map_or_else(
        || http_date(now + offset),
        |zone| {
            if zone == "none" {
                return http_date(now + offset).replace(" GMT", "");
            }
            let minutes = ZONE_MINUTES
                .iter()
                .find(|(name, _)| *name == zone)
                .map_or(0, |(_, minutes)| *minutes);
            http_date(now + offset + minutes * 60).replace("GMT", zone)
        },
    );
    format!("{before}{written}{rest}")
}

/// A response's head: dates resolved, and a `Date` of now added unless the
/// script names one or says `nodate`, as Node's server added it for Chrome.
fn head(spec: &str, now: i64, body: usize) -> String {
    let mut parts = spec.split(" | ");
    let mut first = parts.next().unwrap_or("404").split(' ');
    let status = first.next().unwrap_or("404");
    let mut dated = first.next() != Some("nodate");
    let reason = if status == "304" {
        "Not Modified"
    } else {
        "OK"
    };
    let mut head = format!("HTTP/1.1 {status} {reason}\r\n");
    for header in parts {
        let (name, value) = header.split_once(": ").unwrap_or((header, ""));
        if name.eq_ignore_ascii_case("date") {
            dated = false;
        }
        let _ = write!(head, "{name}: {}\r\n", resolved(value, now));
    }
    if dated {
        let _ = write!(head, "date: {}\r\n", http_date(now));
    }
    let _ = write!(
        head,
        "content-length: {body}\r\ncontent-type: image/png\r\nconnection: \
         close\r\n\r\n"
    );
    head
}

/// An IMF-fixdate in seconds since the epoch, the only form this cache sends
/// back as `If-Modified-Since` in these rows.
fn imf_seconds(value: &str) -> Option<i64> {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct",
        "Nov", "Dec",
    ];
    let words: Vec<&str> = value.split_ascii_whitespace().collect();
    let [_, day, month, year, time, _] = words.as_slice() else {
        return None;
    };
    let month =
        i64::try_from(MONTHS.iter().position(|name| name == month)?).ok()? + 1;
    let (day, year): (i64, i64) = (day.parse().ok()?, year.parse().ok()?);
    let clock: Vec<i64> = time
        .split(':')
        .filter_map(|part| part.parse().ok())
        .collect();
    let [hour, minute, second] = clock.as_slice() else {
        return None;
    };
    // Howard Hinnant's `days_from_civil`.
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let days = era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468;
    Some(days * 86_400 + hour * 3600 + minute * 60 + second)
}

/// Serves `responses` in order, recording what each request carried.
fn origin(responses: Vec<String>) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .unwrap_or_else(|error| unreachable!("{error}"));
    let port = listener
        .local_addr()
        .unwrap_or_else(|error| unreachable!("{error}"))
        .port();
    let sent = Arc::new(Mutex::new(Vec::new()));
    let recording = Arc::clone(&sent);
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            answer(stream, &responses, &recording);
        }
    });
    (format!("http://127.0.0.1:{port}/image.png"), sent)
}

fn answer(
    mut stream: TcpStream,
    responses: &[String],
    sent: &Mutex<Vec<String>>,
) {
    let Ok(clone) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(clone);
    let (mut inm, mut ims) = (None, None);
    let mut line = String::new();
    while reader.read_line(&mut line).unwrap_or(0) > 2 {
        let lower = line.to_ascii_lowercase();
        if let Some(value) = lower.strip_prefix("if-none-match:") {
            let at = line.len() - value.len();
            inm = Some(line[at..].trim().to_owned());
        }
        if lower.starts_with("if-modified-since:") {
            ims = imf_seconds(line["if-modified-since:".len()..].trim());
        }
        line.clear();
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| i64::try_from(since.as_secs()).unwrap_or(0));
    // As an offset from now to the nearest 100 s, as `httpcache.mjs` records.
    let ims = ims.map(|at| {
        let offset = ((at - now) as f64 / 100.0).round() as i64 * 100;
        format!("if-modified-since @{offset}")
    });
    let Ok(mut log) = sent.lock() else { return };
    let n = log.len();
    log.push(match (inm, ims) {
        (None, None) => "plain".to_owned(),
        (Some(tag), None) => format!("if-none-match {tag}"),
        (Some(tag), Some(ims)) => format!("if-none-match {tag} + {ims}"),
        (None, Some(ims)) => ims,
    });
    drop(log);
    let spec = &responses[n.min(responses.len() - 1)];
    let body: &[u8] = if spec.starts_with("200") { DOT } else { &[] };
    let _ = stream.write_all(head(spec, now, body.len()).as_bytes());
    let _ = stream.write_all(body);
}

/// What this cache's origin counted and was sent, replaying `row`.
fn replay(row: &Row<'_>) -> (usize, String) {
    let responses = row.responses.iter().map(|&spec| spec.to_owned()).collect();
    let (url, sent) = origin(responses);
    let cache = ImageCache::new(MemoryStore::new(
        NonZeroU64::new(1 << 20).unwrap_or(NonZeroU64::MIN),
    ));
    for _ in 0..row.loads {
        let mut scene = Scene::new(Size::new(4.0, 4.0));
        let root = scene
            .root()
            .unwrap_or_else(|| unreachable!("a new scene has a root"));
        scene
            .push(
                root,
                Node::new(NodeKind::Image {
                    source: ImageSource::url(url.clone()),
                    frame: None,
                    fit: ObjectFit::Fill,
                    position: (Length::ZERO, Length::ZERO),
                }),
            )
            .unwrap_or_else(|error| unreachable!("{error}"));
        let canvas = Renderer::new()
            .render_with_cache(&scene, &cache)
            .unwrap_or_else(|error| unreachable!("{}: {error}", row.name));
        assert!(canvas.warnings().is_empty(), "{}", row.name);
    }
    sent.lock()
        .map(|log| (log.len(), log.join(" ")))
        .unwrap_or_default()
}

#[test]
fn every_row_agrees_with_chrome_or_is_listed_apart() {
    let rows = rows();
    assert_eq!(rows.len(), 132, "every row was read");
    let mut apart = Vec::new();
    for row in &rows {
        let (requests, sent) = replay(row);
        if (requests, sent.as_str()) != (row.requests, row.sent) {
            apart.push(row.name);
            assert!(
                APART.contains(&row.name),
                "{}: Chrome made {} requests ({}), this cache {requests} \
                 ({sent})",
                row.name,
                row.requests,
                row.sent
            );
        }
    }
    assert_eq!(apart, APART, "the rows apart are exactly these");
}

#[test]
fn the_formatter_writes_imf_fixdate() {
    assert_eq!(http_date(784_111_777), "Sun, 06 Nov 1994 08:49:37 GMT");
    assert_eq!(http_date(0), "Thu, 01 Jan 1970 00:00:00 GMT");
}
