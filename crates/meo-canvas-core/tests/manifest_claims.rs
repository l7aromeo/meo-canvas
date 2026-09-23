//! Claims the README makes, asserted against the manifest: which features are
//! on by default. The `runtime-free` recipe pins "no async runtime either way";
//! that text is shaped and broken here is architecture and stays prose.

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
    // An unresolved URL is an error unless a build asks for the network,
    // asserted on the manifest: `cfg!(feature = "net")` describes only how this
    // test was compiled, so `--all-features` would pass it.
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
