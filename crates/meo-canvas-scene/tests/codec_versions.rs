//! The format revisions `codec::decode` reads: the one it writes, and 10, which
//! 0.2.x wrote before a scene carried fetched image bodies.

use meo_canvas_scene::codec::{self, CodecError, OLDEST_VERSION, VERSION};

/// `fixtures/flex-alignment/scene.mcs` as 0.2.1 committed it, at version 10.
const V10: &[u8] = include_bytes!("assets/codec/flex-alignment.v10.mcs");

/// The same scene as this revision writes it.
const CURRENT: &[u8] =
    include_bytes!("../../../fixtures/flex-alignment/scene.mcs");

/// Byte offset of the `u16` version, behind the four-byte magic.
const VERSION_AT: usize = 4;

const fn version_of(bytes: &[u8]) -> u16 {
    u16::from_le_bytes([bytes[VERSION_AT], bytes[VERSION_AT + 1]])
}

#[test]
fn a_version_10_scene_decodes_as_its_current_counterpart() {
    assert_eq!(version_of(V10), 10);
    assert_eq!(version_of(CURRENT), VERSION);

    let old =
        codec::decode(V10).unwrap_or_else(|error| unreachable!("{error}"));
    let current =
        codec::decode(CURRENT).unwrap_or_else(|error| unreachable!("{error}"));
    assert!(old.fetched_images.is_empty());
    assert_eq!(old, current);
    // Written back, it is the current revision: nothing writes 10.
    assert_eq!(codec::encode(&old), CURRENT);
}

#[test]
fn a_revision_outside_the_ones_read_is_refused_and_named() {
    assert_eq!((OLDEST_VERSION, VERSION), (10, 11));
    for found in [OLDEST_VERSION - 1, VERSION + 1] {
        let mut bytes = CURRENT.to_vec();
        bytes[VERSION_AT..VERSION_AT + 2].copy_from_slice(&found.to_le_bytes());
        let refused = codec::decode(&bytes);
        assert!(
            matches!(
                refused,
                Err(CodecError::UnsupportedVersion { found: f, .. }) if f == found
            ),
            "{found}: {refused:?}"
        );
        if let Err(error) = refused {
            assert_eq!(
                error.to_string(),
                format!("scene format version {found}, expected 10 or 11")
            );
        }
    }
}
