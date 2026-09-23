//! Every box a conformance walker builds names its display: `Box::new` is
//! `Flex` and Chrome's `div` is `block`, a difference each scene's explicit
//! size hides today. It reads the sources, so a new walker is covered, and
//! accepts any stated display, since saying nothing is the defect.

use std::{fs, path::Path};

/// How far below a `Box::new()` a display may be stated: builders chain one
/// call per line and reach ten, and the shortest gap between two constructions
/// is larger than twelve.
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
            // Stop at the next construction as well as at the window, or an
            // unstated box is credited with its neighbour's display.
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
