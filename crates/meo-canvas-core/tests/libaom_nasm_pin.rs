//! Pins the libaom that refuses every nasm 3, which is why the Windows
//! workflows install NASM 2.16.03 (`ci.yml`, `fixtures.yml`,
//! `encode-hashes.yml`). b:448994065 is fixed in libaom 3.13.2; the day
//! libaom-sys bundles it, the first test fails and the pin can go.

use std::process::Command;

/// libaom's first release that detects a working nasm 3 (its CHANGELOG,
/// b:448994065), as `(major, minor, patch)`.
const READS_NASM_3: (u32, u32, u32) = (3, 13, 2);

/// The workspace lockfile, which a mutated copy of cannot be run against: cargo
/// refuses a locked version the registry lacks.
const LOCK: &str = include_str!("../../../Cargo.lock");

/// The libaom version libaom-sys bundles, from the `+libaom.X.Y.Z` build
/// metadata on its locked version, so a 3.12 bump is compared rather than
/// string-matched.
fn bundled_libaom(lock: &str) -> (u32, u32, u32) {
    let versions: Vec<&str> = lock
        .split("[[package]]")
        .filter(|block| block.contains("\nname = \"libaom-sys\"\n"))
        .filter_map(|block| {
            block
                .lines()
                .find_map(|line| line.strip_prefix("version = \""))
                .and_then(|rest| rest.strip_suffix('"'))
        })
        .collect();
    let [version] = versions.as_slice() else {
        unreachable!("Cargo.lock holds {versions:?} for libaom-sys, not one")
    };
    let libaom = version.split_once("+libaom.").map_or_else(
        || unreachable!("libaom-sys {version} names no bundled libaom"),
        |(_, libaom)| libaom,
    );
    let parts: Vec<u32> = libaom
        .split('.')
        .map(|part| {
            part.parse().unwrap_or_else(|_| {
                unreachable!("libaom {libaom} is not X.Y.Z")
            })
        })
        .collect();
    let [major, minor, patch] = parts.as_slice() else {
        unreachable!("libaom {libaom} is not X.Y.Z")
    };
    (*major, *minor, *patch)
}

#[test]
fn the_bundled_libaom_still_refuses_nasm_3() {
    let bundled = bundled_libaom(LOCK);
    assert!(
        bundled < READS_NASM_3,
        "libaom-sys now bundles libaom {}.{}.{}, which detects nasm 3: unpin \
         `choco install nasm --version=2.16.3` in .github/workflows/ci.yml, \
         .github/workflows/fixtures.yml and .github/workflows/encode-hashes.yml, \
         drop their [WORKAROUND] comments, and delete this probe",
        bundled.0,
        bundled.1,
        bundled.2
    );
}

/// The comparison above, over the lockfile with its libaom rewritten: it fires
/// at the fix and after it, and not before, compared as numbers.
#[test]
fn the_comparison_fires_at_the_fix_and_not_before() {
    let (major, minor, patch) = bundled_libaom(LOCK);
    let current = format!("+libaom.{major}.{minor}.{patch}\"");
    assert_eq!(
        LOCK.matches(&current).count(),
        1,
        "{current} is not in Cargo.lock once"
    );
    // `3.9.0` is the row a string comparison gets wrong, as it does `4.0.0`.
    let rows = [
        ("3.13.2", true),
        ("3.14.0", true),
        ("4.0.0", true),
        ("3.12.0", false),
        ("3.13.1", false),
        ("3.9.0", false),
    ];
    for (libaom, fires) in rows {
        let rewritten = LOCK.replace(&current, &format!("+libaom.{libaom}\""));
        assert_eq!(
            bundled_libaom(&rewritten) >= READS_NASM_3,
            fires,
            "libaom {libaom} should {}fire the probe",
            if fires { "" } else { "not " }
        );
    }
}

/// The host triple, which `cargo metadata` is filtered to: unfiltered it reads
/// every platform's manifests, and `--offline` then fails on a runner whose
/// cache holds only its own platform's crates.
fn host() -> String {
    let output = Command::new("rustc")
        .arg("-vV")
        .output()
        .unwrap_or_else(|error| unreachable!("rustc -vV: {error}"));
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .map_or_else(
            || unreachable!("rustc -vV printed no `host:` line"),
            str::to_owned,
        )
}

/// The vendored libaom source cargo resolved for libaom-sys, read from
/// `cargo metadata` rather than guessed from `CARGO_HOME`. Every way of not
/// finding it fails, naming what was run and searched.
fn libaom_source() -> std::path::PathBuf {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let host = host();
    let args = [
        "metadata",
        "--format-version",
        "1",
        "--offline",
        "--locked",
        "--filter-platform",
        host.as_str(),
    ];
    let output = Command::new(&cargo)
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap_or_else(|error| unreachable!("{cargo} {args:?}: {error}"));
    assert!(
        output.status.success(),
        "{cargo} {args:?} exited {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let json = String::from_utf8_lossy(&output.stdout);
    // A package entry, not a dependency entry naming it: only the package has
    // `version` straight after its name.
    let marker = "\"name\":\"libaom-sys\",\"version\":\"";
    let entries = json.matches(marker).count();
    assert_eq!(
        entries, 1,
        "searched `cargo metadata` for {host} for `{marker}` and found {entries} \
         package entries, not one"
    );
    let at = json.find(marker).unwrap_or_else(|| {
        unreachable!("{marker} counted once, found nowhere")
    });
    let key = "\"manifest_path\":\"";
    let start = json[at..].find(key).map_or_else(
        || unreachable!("the libaom-sys package entry has no manifest_path"),
        |offset| at + offset + key.len(),
    );
    let end = json[start..].find('"').map_or_else(
        || unreachable!("the manifest_path is unterminated"),
        |offset| start + offset,
    );
    let manifest = json[start..end].replace("\\\\", "\\");
    std::path::Path::new(&manifest)
        .parent()
        .unwrap_or_else(|| unreachable!("{manifest} has no directory"))
        .join("vendor/build/cmake/aom_optimization.cmake")
}

// [FOUNDATION] the pin rests on libaom gating nasm by `-Ox` in `nasm -hf`,
// which NASM 2.16.03 prints; that it does, on choco's build, is not testable
// here. A different gate could refuse 2.16.03 too, and this row would say so.
#[test]
fn libaom_gates_nasm_on_its_help_text() {
    let path = libaom_source();
    let cmake = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| unreachable!("{}: {error}", path.display()));
    let gate = cmake
        .split("function(test_nasm)")
        .nth(1)
        .and_then(|body| body.split("endfunction()").next())
        .unwrap_or_else(|| unreachable!("{} has no test_nasm", path.display()));
    assert!(
        gate.contains("-hf") && gate.contains("MATCHES \"-Ox\""),
        "libaom's test_nasm no longer reads `-Ox` from `nasm -hf`, so pinning \
         NASM 2.16.03 is no longer known to pass it:\n{gate}"
    );
}
