//! Every checked-in Chrome table is read by a test, and every table a test
//! names exists: an unread answer is indistinguishable from never having asked.
//! Read means `include_str!` of the file -- a mention in prose reads nothing --
//! and whether the parsed values reach an assertion is left to review.

use std::{collections::BTreeSet, fs, path::Path};

/// Where the tables live, relative to this crate.
const TABLES: &str = "tests/assets/chrome";

/// The trees searched for readers, both crates: `chrome_border_rhythm.rs` lives
/// in `meo-canvas-core` and reads a table that lives here.
const SOURCES: &[&str] = &[
    "tests",
    "src",
    "../meo-canvas-core/tests",
    "../meo-canvas-core/src",
];

/// Tables not yet read, each with the reason and what is expected to read it: a
/// recorded exception, never a way to make the suite pass.
const KNOWN_UNREAD: &[(&str, &str)] = &[];

/// Every `.rs` file under `root`, recursively.
fn sources(root: &Path, into: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            sources(&path, into);
        } else if path.extension().is_some_and(|kind| kind == "rs")
            && let Ok(text) = fs::read_to_string(&path)
        {
            into.push(text);
        }
    }
}

#[test]
fn every_chrome_table_is_read_by_a_test() {
    let mut text = Vec::new();
    for root in SOURCES {
        sources(Path::new(root), &mut text);
    }
    assert!(
        !text.is_empty(),
        "no sources were searched; the paths are wrong"
    );

    let tables: BTreeSet<String> = fs::read_dir(TABLES)
        .unwrap_or_else(|error| {
            unreachable!("{TABLES} is not readable: {error}")
        })
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    assert!(
        !tables.is_empty(),
        "no tables were found; the path is wrong"
    );

    let mut unread = Vec::new();
    let mut stale_exemption = Vec::new();

    for table in &tables {
        // The `include_str!` form specifically, matched by the path's tail
        // since the other crate reaches these tables through
        // `../../meo-canvas/`. Whitespace is removed first, since rustfmt
        // splits the macro and its path across lines.
        let flat: Vec<String> = text
            .iter()
            .map(|source| {
                source.chars().filter(|c| !c.is_whitespace()).collect()
            })
            .collect();
        let tail = format!("assets/chrome/{table}\"");
        let read = flat.iter().any(|source| {
            source.match_indices("include_str!(\"").any(|(at, opener)| {
                let after = &source[at + opener.len()..];
                after
                    .find('"')
                    .is_some_and(|end| after[..=end].ends_with(&tail))
            })
        });
        let excused = KNOWN_UNREAD.iter().find(|(name, _)| name == table);

        match (read, excused) {
            (false, None) => unread.push(format!(
                "{table} is read by no test. Write a walker for it, or add it \
                 to KNOWN_UNREAD with the reason and the work that removes it"
            )),
            (true, Some(_)) => stale_exemption.push(format!(
                "{table} is now read. That is the exemption doing its job -- \
                 delete its row from KNOWN_UNREAD"
            )),
            _ => {}
        }
    }

    // The other direction: a reader naming a table that is not there. A
    // renamed file would otherwise leave an `include_str!` pointing at nothing
    // -- which the compiler does catch, but only for this exact spelling, and
    // this says so in the same place as everything else about the pairing.
    let mut missing = Vec::new();
    for source in &text {
        for line in source.lines().filter(|line| line.contains("include_str!"))
        {
            let Some(at) = line.find("assets/chrome/") else {
                continue;
            };
            let rest = &line[at + "assets/chrome/".len()..];
            if let Some(end) = rest.find('"') {
                let named = &rest[..end];
                if !tables.contains(named) {
                    missing.push(format!(
                        "a test reads assets/chrome/{named}, which does not exist"
                    ));
                }
            }
        }
    }

    let mut wrong = unread;
    wrong.extend(stale_exemption);
    wrong.extend(missing);
    assert!(
        wrong.is_empty(),
        "{} tables are not paired with a reader:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
    eprintln!(
        "chrome tables: {} present, {} excused",
        tables.len(),
        KNOWN_UNREAD.len()
    );
}
