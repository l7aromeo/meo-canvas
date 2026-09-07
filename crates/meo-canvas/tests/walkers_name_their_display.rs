//! Every box a conformance walker builds names its display.
//!
//! # What this is for
//!
//! `Box::new` names `Display::Flex` and Chrome's `div` is `block`, so a walker
//! that builds a box for a plain `div` and states nothing is measuring a flex
//! container against a block one. Twelve boxes across four walkers did that
//! until they were made to say `Block`, and **the survey behind that change
//! showed the difference is currently invisible**: 1345 renders, byte
//! identical either way, because every one of those scenes has an explicit
//! size.
//!
//! **That is exactly why this exists.** The rule was worth applying for a
//! reason that has not bitten yet -- the next fixture written against one of
//! these scenes inherits the coincidence -- and a rule whose violation costs
//! nothing today is a rule that decays. Nothing in the suite held it: the
//! twelve were correct and the thirteenth would have been whatever
//! `Box::new` defaults to.
//!
//! # Why a test rather than a lint or an assertion in each walker
//!
//! **An assertion inside a walker cannot catch a new walker.** The file that
//! forgets to state a display is the file that forgets to assert it, so the
//! obvious cheap shape protects only the files that already comply. This
//! reads the sources instead, so a file added tomorrow is covered by a test
//! written today.
//!
//! **And it asks for a display rather than for `Block`.** Three of these boxes
//! are flex containers because Chrome's markup says `display:flex`, and the
//! grid container says `Display::Grid` against a `display:grid` scene. A
//! walker that states the property where Chrome states it is not the defect;
//! **saying nothing is.**

use std::{fs, path::Path};

/// How far below a `Box::new()` a display may be stated.
///
/// Builders here are a chain of one call per line, and the longest of them
/// reaches ten. Twelve leaves room without spanning two constructions: the
/// shortest gap between two `Box::new()` calls in these files is larger.
const WITHIN: usize = 12;

/// The walkers this reads, by their own naming rule: a conformance walker is
/// `chrome_*.rs` beside a Chrome table.
fn walkers() -> Vec<std::path::PathBuf> {
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dirs = [here.join("tests"), here.join("../meo-canvas-core/tests")];
    let mut found: Vec<std::path::PathBuf> = dirs
        .iter()
        .filter_map(|dir| fs::read_dir(dir).ok())
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            // `extension` rather than `ends_with(".rs")`, which clippy refuses
            // as a case-sensitive comparison and is right to: `.RS` is the
            // same file to the compiler and a different string here.
            let rust = path.extension().is_some_and(|kind| kind == "rs");
            let named = path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("chrome_"));
            rust && named
        })
        .collect();
    found.sort();
    found
}

#[test]
fn every_box_in_a_conformance_walker_states_its_display() {
    let files = walkers();
    // The denominator, so a rule that stopped reading anything says so rather
    // than passing. `chrome_tables.rs` alone holds eighteen of these.
    assert!(
        files.len() >= 5,
        "found {} conformance walkers, which is fewer than exist",
        files.len()
    );

    let mut silent = Vec::new();
    let mut boxes = 0_usize;
    for path in &files {
        let source = fs::read_to_string(path)
            .unwrap_or_else(|error| unreachable!("{path:?}: {error}"));
        let lines: Vec<&str> = source.lines().collect();
        for (index, line) in lines.iter().enumerate() {
            if !line.contains("Box::new()") {
                continue;
            }
            boxes += 1;
            // **Stop at the next construction as well as at the window.**
            // Written with the window alone first, and the mutation that adds
            // an unstated box passed: the box it was inserted before stated a
            // display four lines later, and the scan credited it to both. An
            // unstated box hiding behind a stated neighbour is exactly the
            // case this rule is for.
            let end = (index + WITHIN).min(lines.len());
            let end = lines[index + 1..end]
                .iter()
                .position(|following| following.contains("Box::new()"))
                .map_or(end, |offset| index + 1 + offset);
            let states = lines[index..end]
                .iter()
                .any(|following| following.contains(".display("));
            if !states {
                let name = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("?");
                silent.push(format!("{name}:{}", index + 1));
            }
        }
    }

    assert!(
        boxes >= 20,
        "read {boxes} boxes across {} walkers, which is fewer than exist: \
         the rule is reading the wrong files",
        files.len()
    );
    assert!(
        silent.is_empty(),
        "{} boxes state no display, so they are flex containers standing in \
         for whatever Chrome's markup says:\n{}\n\nState it -- `Block` for a \
         plain div, `Flex` or `Grid` where Chrome's own markup does.",
        silent.len(),
        silent.join("\n")
    );
}
