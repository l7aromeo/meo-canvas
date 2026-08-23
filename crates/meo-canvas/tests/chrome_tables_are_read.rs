//! Every checked-in Chrome table is read by a test, and every table a test
//! names exists.
//!
//! # The failure this exists to catch
//!
//! **An unread answer is indistinguishable from never having asked.** A table
//! is measured in a browser, checked in, and then nothing consults it — and
//! nothing fails, because a file that is never read cannot disagree with
//! anything. The suite reports green while a browser's answers sit unused.
//!
//! The pinned `KNOWN` lists guard the opposite direction: a row that starts
//! agreeing fails and says to delete it, which has caught four fixes in a day.
//! **This is the other direction, and nothing was watching it.**
//!
//! # Why a mention is not a read
//!
//! Three ways a table can be unread, and only the first is obvious:
//!
//! 1. **nothing refers to it at all** — `gradient-truth.tsv` and
//!    `object-fit.tsv` were measured and never consulted
//! 2. **a doc comment names it** — `blend-modes.tsv` and `dotted-rhythm.tsv`
//!    are cited in prose, which satisfies a `grep` and reads nothing
//! 3. **its numbers are transcribed into a constant** — `border-rhythm.tsv` had
//!    five rows copied into a `const CHROME`, so the test asserts against **a
//!    copy that can drift from the table in silence**, and a regeneration that
//!    changed an answer would leave the two disagreeing with nothing to say so
//!
//! # How far this reaches, exactly
//!
//! **It proves a table is opened. It does not prove its numbers are used.** A
//! file may `include_str!` a table and still assert against a hand-written
//! struct three lines below — case 3 surviving inside a file that now passes
//! case 1. `border-rhythm.tsv` was caught only because it had no `include_str!`
//! at all; one with both would go unnoticed here.
//!
//! Checking that a parsed value reaches an assertion is a different and much
//! harder thing, and this does not attempt it. **An unnamed limit gets
//! mistaken for coverage**, so it is named: the guard is a floor, and review is
//! what covers the rest.
//!
//! So this looks for `include_str!` of the file, which is the one form that
//! makes the committed bytes reach an assertion. A file that is read and never
//! asserted on is still the same failure with a witness — that part cannot be
//! checked mechanically, and is what review is for.

use std::{collections::BTreeSet, fs, path::Path};

/// Where the tables live, relative to this crate.
const TABLES: &str = "tests/assets/chrome";

/// The trees searched for readers.
///
/// Both crates, tests and sources alike: `chrome_border_rhythm.rs` lives in
/// `meo-canvas-core` and reads a table that lives here, so a search of this
/// crate alone would report a false absence.
const SOURCES: &[&str] = &[
    "tests",
    "src",
    "../meo-canvas-core/tests",
    "../meo-canvas-core/src",
];

/// Tables not yet read, each with the reason and the work that will remove it.
///
/// **A recorded exception a future reader can delete, not a silent omission.**
/// Adding a name here to make the suite pass is the thing this file exists to
/// prevent, so each entry carries what is expected to read it.
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
        // The `include_str!` form specifically, on the line that names the
        // file. A doc comment naming it matches a plain substring search and
        // reads nothing. The path is matched by its tail rather than in full,
        // because `chrome_border_rhythm.rs` lives in the other crate and
        // reaches these tables through `../../meo-canvas/...`.
        let tail = format!("assets/chrome/{table}\"");
        let read = text.iter().any(|source| {
            source.lines().any(|line| {
                line.contains("include_str!") && line.contains(&tail)
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
