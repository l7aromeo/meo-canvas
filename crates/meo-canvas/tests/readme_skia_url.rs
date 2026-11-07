//! The README tells a crate user the `SKIA_BINARIES_URL` line to add, and the
//! justfile is where this repository builds with it; a release of
//! meo-skia-canvas that changes its patches changes the line, so the two copies
//! have to move together.

const JUSTFILE: &str = include_str!("../../../justfile");
const README: &str = include_str!("../README.md");

/// The quoted value of `name = "..."` on the line starting with `prefix`.
fn quoted<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let line = text.lines().find(|line| line.starts_with(prefix))?;
    line.split('"').nth(1)
}

#[test]
fn the_readme_names_the_url_this_repository_builds_with() {
    let built = quoted(JUSTFILE, "export SKIA_BINARIES_URL := ")
        .unwrap_or_else(|| unreachable!("the justfile exports no URL"));
    let told = quoted(README, "SKIA_BINARIES_URL = ")
        .unwrap_or_else(|| unreachable!("the README gives no URL line"));
    assert!(built.contains("{key}"), "not an archive template: {built}");
    assert_eq!(told, built, "the README's line differs from the justfile's");
}
