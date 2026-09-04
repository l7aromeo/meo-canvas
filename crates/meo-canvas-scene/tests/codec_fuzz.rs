//! Malformed bytes through `decode`, in bulk.
//!
//! **A refusal is the expected outcome and a panic is the finding.** `decode`
//! reads bytes that, on the JavaScript surface, arrive from a caller, so every
//! shape of corruption has to come back as a `CodecError` rather than as an
//! unwind through the addon boundary.
//!
//! The generator is seeded and the seed is in the source: a failure here is
//! reproducible by running it again, rather than a story about a run nobody
//! else has.
use meo_canvas_scene::{Scene, Size, codec, node::Node};

/// The number of inputs. Enough to be worth the second it costs, and
/// overridable for a longer soak: twenty million have been run this way and
/// found nothing.
const ITERATIONS: usize = 100_000;

/// Fixed, so a failure is reproducible.
const SEED: u64 = 0x5EED_1234_ABCD_0001;

/// **The floor that stops this measuring the magic number.** A fuzz whose
/// inputs are all rejected in the first four bytes is green and worthless; the
/// acceptance rate is what says the mutations reach the decoder's interior.
/// Measured at about 7%, so 2% is a floor rather than a target.
const MIN_ACCEPTED: usize = ITERATIONS / 50;

/// xorshift64*, so the sequence is the seed's and nothing else's.
struct Rng(u64);

impl Rng {
    const fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, bound: usize) -> usize {
        if bound == 0 {
            return 0;
        }
        usize::try_from(self.next() % bound as u64).unwrap_or(0)
    }
}

/// Two real encodings: an empty scene, and one with text in it.
fn corpus() -> Vec<Vec<u8>> {
    let mut scene = Scene::new(Size::new(120.0, 80.0));
    let empty = codec::encode(&scene);

    let root = scene
        .root()
        .unwrap_or_else(|| unreachable!("a new scene has a root"));
    for _ in 0..8 {
        scene
            .push(root, Node::text("hello \u{1F600} world"))
            .unwrap_or_else(|error| unreachable!("{error}"));
    }
    vec![empty, codec::encode(&scene)]
}

#[test]
fn decode_refuses_rather_than_panics() {
    let corpus = corpus();
    let mut rng = Rng(SEED);
    let (mut accepted, mut refused) = (0_usize, 0_usize);

    for i in 0..ITERATIONS {
        let bytes = match i % 4 {
            // Mutations of a real encoding, which is what gets past the magic
            // and the version and into the interior.
            0 | 1 => {
                let mut bytes = corpus[rng.below(corpus.len())].clone();
                for _ in 0..=rng.below(6) {
                    let at = rng.below(bytes.len());
                    bytes[at] = u8::try_from(rng.next() & 0xFF).unwrap_or(0);
                }
                bytes
            }
            // Truncation, which is what a short read looks like.
            2 => {
                let whole = &corpus[rng.below(corpus.len())];
                whole[..rng.below(whole.len())].to_vec()
            }
            // A valid header followed by noise, or noise alone.
            _ => {
                let mut bytes = if rng.next().is_multiple_of(2) {
                    corpus[0][..corpus[0].len().min(30)].to_vec()
                } else {
                    Vec::new()
                };
                for _ in 0..rng.below(512) {
                    bytes.push(u8::try_from(rng.next() & 0xFF).unwrap_or(0));
                }
                bytes
            }
        };

        if let Ok(scene) = codec::decode(&bytes) {
            accepted += 1;
            // **A decoder that admits what the encoder cannot express is a
            // worse failure than a refusal**, so anything accepted has to
            // survive being written back out and read again.
            let again = codec::encode(&scene);
            assert!(
                codec::decode(&again).is_ok(),
                "a decoded scene did not survive a round trip"
            );
        } else {
            refused += 1;
        }
    }

    assert!(
        accepted >= MIN_ACCEPTED,
        "only {accepted} of {ITERATIONS} inputs were accepted, which is below \
         the {MIN_ACCEPTED} that says the mutations reach past the header; \
         {refused} refused"
    );
}
