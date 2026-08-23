//! Claims the README makes, asserted against the manifest rather than read.
//!
//! # Why prose needs a test at all
//!
//! `crates/meo-canvas-core/README.md` said **"Fetch-free and runtime-free"**
//! for as long as it was true, and went on saying it after the `net` feature
//! landed. **Nothing failed**: no test covers prose, the claim was still
//! syntactically fine, and the only signal was a person reading it against the
//! code. That is the worst kind of stale -- an outward-facing statement that is
//! wrong in the one file a user actually reads.
//!
//! **A claim with a test behind it is not prose any more.** This file pins the
//! part of that sentence a machine can check: which features are on by default.
//! The rest of it -- *no async runtime either way* -- is pinned by the
//! `runtime-free` recipe, which reads the dependency tree.
//!
//! What cannot be pinned here stays prose and is honest about it: whether text
//! is *shaped by Skia and broken into lines here* is architecture, and only a
//! reader can check it.

/// The manifest this crate is built from, read at compile time.
const MANIFEST: &str = include_str!("../Cargo.toml");

/// The line naming the default feature set.
fn default_features() -> &'static str {
    MANIFEST
        .lines()
        .find(|line| line.starts_with("default = "))
        .unwrap_or_else(|| {
            unreachable!("the manifest names no default features")
        })
}

#[test]
fn fetching_is_not_on_by_default() {
    // **The claim: an unresolved URL is an error unless a build asks for the
    // network.** Asserted against the manifest rather than against
    // `cfg!(feature = "net")`, which would only describe however *this* test
    // was compiled -- `--all-features` would make it pass while saying
    // nothing, and that is the failure mode the whole file is about.
    let default = default_features();
    assert!(
        !default.contains("net"),
        "the README says fetch-free by default and the manifest says \
         {default:?}"
    );
}

#[test]
fn the_gpu_backends_are_not_on_by_default_either() {
    // The same sentence's neighbour: a build with no backend named renders on
    // the CPU, which is what a portable `cargo check` needs. This is the claim
    // that the two example surfaces silently disagreed over, so it is worth a
    // line rather than a comment.
    let default = default_features();
    assert!(
        !default.contains("metal"),
        "metal is on by default: {default:?}"
    );
    assert!(
        !default.contains("vulkan"),
        "vulkan is on by default: {default:?}"
    );
}

#[test]
fn the_features_the_claims_are_about_still_exist() {
    // The control. Both assertions above pass if the features are renamed or
    // deleted, because they are checking for absence from one line -- so the
    // presence of each name somewhere in the manifest is what keeps them
    // meaningful.
    for feature in ["net = ", "metal = ", "vulkan = "] {
        assert!(
            MANIFEST.contains(feature),
            "{feature:?} is gone from the manifest, so the test above now \
             asserts nothing"
        );
    }
}
