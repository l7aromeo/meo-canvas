//! The animation helpers against v9's own numbers, generated from its built
//! module at `v9.0.2` (`890eed2`). Printed at JavaScript's shortest round-trip
//! precision, so comparison is exact, except `spring.tsv` within [`EXP_ULP`].
//! These tables cover `easing` and `spring`; the other modules have unit tests.

#![expect(
    clippy::float_cmp,
    reason = "the exact comparison IS the assertion. Both sides parse the \
              same shortest-round-trip decimal, so equality is the strongest \
              claim available and an epsilon here would hide the disagreement \
              this file exists to find. Where an epsilon is unavoidable it is \
              `EXP_ULP`, which is measured rather than chosen."
)]

use meo_canvas_core::{
    animate::{
        color::{Rgba, mix},
        easing::{Easing, cubic_bezier, steps},
        group::{Member, Parallel},
        interpolate::{Animatable, keyframes, map_range},
        sampled::Sampled,
        sequence::{Sequence, Step},
        spring::{Shape, Spring},
        track::{Motion, Track},
    },
    color::parse_channels,
};

/// The rows of one vector file, comments dropped and fields split, checked
/// against the count the file declares: a bare `#808080` in the first field
/// reads as a comment and drops its row silently, and the count is what catches
/// it.
fn rows(text: &'static str) -> Vec<Vec<&'static str>> {
    let data: Vec<Vec<&str>> = text
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| line.split('\t').collect())
        .collect();
    let declared = text
        .lines()
        .find_map(|line| line.strip_prefix("# rows: "))
        .unwrap_or_else(|| {
            unreachable!("the table declares no `# rows:` count")
        })
        .trim()
        .parse::<usize>()
        .unwrap_or_else(|error| {
            unreachable!("unreadable `# rows:` count: {error}")
        });
    assert_eq!(
        data.len(),
        declared,
        "the table declares {declared} rows and {} survived parsing; a row is \
         being swallowed",
        data.len()
    );
    data
}

/// One field, unquoted where the table quoted it.
///
/// Quoting exists so a value beginning with `#` cannot be read as a comment.
fn field(value: &str) -> &str {
    value.trim_matches('"')
}

/// One field as the `f64` it round-trips to.
fn at(row: &[&str], index: usize) -> f64 {
    row[index]
        .parse()
        .unwrap_or_else(|error| unreachable!("{:?}: {error}", row[index]))
}

#[test]
fn every_curve_in_the_catalogue_matches_v9() {
    let table = include_str!("assets/animate/easing.tsv");
    let mut checked = 0;
    for row in rows(table) {
        let curve = Easing::ALL
            .into_iter()
            .find(|curve| curve.name() == row[0])
            .unwrap_or_else(|| {
                unreachable!("{} is not in the catalogue", row[0])
            });
        let (t, expected) = (at(&row, 1), at(&row, 2));
        let ours = curve.at(t);
        assert!(
            ours == expected,
            "{}({t}) is {ours} where v9 draws {expected}",
            row[0]
        );
        checked += 1;
    }
    // The catalogue and the table have to be the same size, or a curve we
    // never wrote would pass by not being asked about.
    assert_eq!(checked, 403, "the table changed size");
}

#[test]
fn the_catalogue_is_the_whole_catalogue() {
    let table = include_str!("assets/animate/easing.tsv");
    let named: std::collections::HashSet<&str> =
        rows(table).into_iter().map(|row| row[0]).collect();
    for curve in Easing::ALL {
        assert!(named.contains(curve.name()), "{} is untested", curve.name());
    }
    assert_eq!(
        named.len(),
        Easing::ALL.len(),
        "a curve is missing from one side"
    );
}

#[test]
fn a_bezier_solves_where_v9_solves() {
    let table = include_str!("assets/animate/bezier.tsv");
    for row in rows(table) {
        let (x1, y1) = (at(&row, 0), at(&row, 1));
        let (x2, y2) = (at(&row, 2), at(&row, 3));
        let (t, expected) = (at(&row, 4), at(&row, 5));
        let ours = cubic_bezier(x1, y1, x2, y2).at(t);
        assert!(
            ours == expected,
            "cubic-bezier({x1}, {y1}, {x2}, {y2}) at {t} is {ours} where v9 \
             gives {expected}"
        );
    }
}

#[test]
fn steps_land_on_the_side_v9_lands_on() {
    let table = include_str!("assets/animate/steps.tsv");
    for row in rows(table) {
        let count = row[0]
            .parse::<u32>()
            .unwrap_or_else(|error| unreachable!("{error}"));
        let (t, expected) = (at(&row, 1), at(&row, 2));
        let ours = steps(count)
            .unwrap_or_else(|error| unreachable!("{error}"))
            .at(t);
        assert!(
            ours == expected,
            "steps({count}) at {t} is {ours} where v9 gives {expected}"
        );
    }
}

#[test]
fn a_step_count_below_one_is_refused() {
    assert!(
        steps(0).is_err(),
        "steps(0) has no width to hold a value for"
    );
    assert!(steps(1).is_ok());
}

/// How many representable `f64` values lie between two numbers.
fn ulps_between(ours: f64, theirs: f64) -> i64 {
    #[expect(
        clippy::cast_possible_wrap,
        reason = "both values are small positive-exponent finites here, so \
                  the bit patterns are far from the sign boundary"
    )]
    let bits = |value: f64| value.to_bits() as i64;
    (bits(ours) - bits(theirs)).abs()
}

/// One ulp, measured: V8's `Math.exp` and Rust's libm differ in the last place
/// on some arguments (`stiffness 100, damping 10, t 0.35`), and only the spring
/// reaches `exp`. Two of 77 rows differ, by exactly one. The bound holds for
/// these samples only, so a new spring row re-verifies it.
const EXP_ULP: i64 = 1;

#[test]
fn a_spring_is_where_v9_puts_it() {
    let table = include_str!("assets/animate/spring.tsv");
    let mut regimes = (0, 0, 0);
    for row in rows(table) {
        let spring = Spring {
            from: at(&row, 0),
            to: at(&row, 1),
            stiffness: at(&row, 2),
            damping: at(&row, 3),
            mass: at(&row, 4),
            velocity: at(&row, 5),
        };
        let (t, expected) = (at(&row, 6), at(&row, 7));
        let ours = spring.at(t).unwrap_or_else(|error| unreachable!("{error}"));
        assert!(
            ulps_between(ours, expected) <= EXP_ULP,
            "a spring at {t}s is {ours} where v9 gives {expected}, which is \
             {} ulp apart (stiffness {}, damping {})",
            ulps_between(ours, expected),
            spring.stiffness,
            spring.damping
        );

        // Which regime each row exercised, so the table cannot quietly test
        // one solution three times.
        let zeta =
            spring.damping / (2.0 * (spring.stiffness * spring.mass).sqrt());
        if (zeta - 1.0).abs() < 1e-4 {
            regimes.1 += 1;
        } else if zeta < 1.0 {
            regimes.0 += 1;
        } else {
            regimes.2 += 1;
        }
    }
    // All three regimes are exercised, which a row count cannot show:
    // `stiffness 100, damping 20` has a damping ratio of exactly 1, the
    // critical branch.
    let (under, critical, over) = regimes;
    assert!(
        under > 0,
        "no underdamped row: the oscillating solution is untested"
    );
    assert!(
        critical > 0,
        "no critical row: the limiting solution is untested"
    );
    assert!(
        over > 0,
        "no overdamped row: the two-exponential solution is untested"
    );
}

#[test]
fn a_spring_without_an_equation_is_refused() {
    let none = Spring {
        stiffness: 0.0,
        ..Spring::default()
    };
    assert!(
        none.at(0.5).is_err(),
        "a spring with no stiffness has no equation"
    );
    assert!(
        Spring {
            damping: -1.0,
            ..Spring::default()
        }
        .at(0.5)
        .is_err()
    );
    assert!(
        Spring {
            mass: 0.0,
            ..Spring::default()
        }
        .at(0.5)
        .is_err()
    );
    assert!(Spring::default().at(0.5).is_ok());
}

/// One easing by the name the tables use, or `Linear` for the absent option.
///
/// `-` is the option omitted rather than `linear` named, and the two are
/// different calls on the JavaScript side even though they answer alike.
fn easing_named(name: &str) -> Easing {
    if name == "-" {
        return Easing::Linear;
    }
    Easing::ALL
        .into_iter()
        .find(|curve| curve.name() == name)
        .unwrap_or_else(|| unreachable!("{name} is not in the catalogue"))
}

#[test]
fn a_spring_settles_where_v9_settles_for_every_regime() {
    // `settles_after` had no direct test at all before this table: it was
    // reached only through `Track::duration` and `Sequence::plan`, so it could
    // have drifted with the suite staying green.
    let table = include_str!("assets/animate/spring-duration.tsv");
    let mut checked = 0;
    for row in rows(table) {
        let spring = Spring {
            from: at(&row, 0),
            to: at(&row, 1),
            stiffness: at(&row, 2),
            damping: at(&row, 3),
            mass: at(&row, 4),
            velocity: at(&row, 5),
        };
        let ours = spring
            .settles_after(at(&row, 6))
            .unwrap_or_else(|error| unreachable!("{error}"));
        let expected = at(&row, 7);
        assert!(
            ours == expected,
            "a spring {row:?} settles after {ours} where v9 says {expected}"
        );
        checked += 1;
    }
    assert_eq!(checked, 40);
}

#[test]
fn a_lerp_lands_where_v9_lands() {
    // `t` runs outside 0..1 here on purpose: the mix does not clamp, and a
    // table sampling only 0..1 could not tell an unclamped implementation from
    // a clamped one.
    let table = include_str!("assets/animate/lerp.tsv");
    for row in rows(table) {
        let (from, to, t) = (at(&row, 0), at(&row, 1), at(&row, 2));
        let ours = from.mix(to, t);
        let expected = at(&row, 3);
        assert!(
            ours == expected,
            "lerp({from}, {to}, {t}) is {ours} where v9 says {expected}"
        );
    }
}

#[test]
fn a_range_maps_where_v9_maps() {
    let table = include_str!("assets/animate/map-range.tsv");
    for row in rows(table) {
        let clamp = field(row[5]) == "true";
        let ours = map_range(
            at(&row, 0),
            (at(&row, 1), at(&row, 2)),
            (at(&row, 3), at(&row, 4)),
            clamp,
        );
        let expected = at(&row, 6);
        assert!(
            ours == expected,
            "map_range {row:?} is {ours} where v9 says {expected}"
        );
    }
}

#[test]
fn keyframes_land_where_v9_lands() {
    let table = include_str!("assets/animate/interpolate.tsv");
    for row in rows(table) {
        let t = at(&row, 0);
        let stops: Vec<f64> = field(row[1])
            .split(';')
            .map(|value| {
                value
                    .parse()
                    .unwrap_or_else(|error| unreachable!("{error}"))
            })
            .collect();
        let values: Vec<f64> = field(row[2])
            .split(';')
            .map(|value| {
                value
                    .parse()
                    .unwrap_or_else(|error| unreachable!("{error}"))
            })
            .collect();
        let ours = keyframes(t, &stops, &values, easing_named(field(row[3])))
            .unwrap_or_else(|error| unreachable!("{error}"));
        let expected = at(&row, 4);
        assert!(
            ours == expected,
            "keyframes {row:?} is {ours} where v9 says {expected}"
        );
    }
}

#[test]
fn a_mix_lands_where_v9_lands_for_the_kinds_rust_has() {
    // `kind = array` is skipped: `Animatable` covers `f64` and `Rgba` only, so
    // a Rust caller cannot mix arrays, and the table's `surface` column
    // records the gap. Colour rows are skipped too -- their values are v9's
    // CSS strings, and `mix-color.tsv` walks the colour arithmetic.
    let table = include_str!("assets/animate/mix.tsv");
    let (mut compared, mut skipped) = (0, 0);
    for row in rows(table) {
        if field(row[0]) != "number" || field(row[2]) != "value" {
            skipped += 1;
            continue;
        }
        let (from, to, t) = (at(&row, 3), at(&row, 4), at(&row, 5));
        let ours = from.mix(to, t);
        let expected = at(&row, 6);
        assert!(
            ours == expected,
            "mix({from}, {to}, {t}) is {ours} where v9 says {expected}"
        );
        compared += 1;
    }
    // A table read as all-skips would pass without asking anything.
    assert_eq!((compared, skipped), (5, 10));
}

/// A CSS colour as this crate reads it, or a panic naming the row.
fn parsed(css: &str) -> Rgba {
    let [r, g, b, a] = parse_channels(css)
        .unwrap_or_else(|| unreachable!("{css} did not parse"));
    Rgba { r, g, b, a }
}

#[test]
fn a_colour_parses_to_the_channels_the_author_wrote() {
    // The one table whose expected values are not v9's: v9 quantises alpha to
    // eight bits (`0.102` for `0.1`), so the browser's `0.1` is the
    // tiebreak. The `note` column names each surface's wrong answer, so a
    // regression says which it hit.
    let table = include_str!("assets/animate/parse-color.tsv");
    let mut checked = 0;
    for row in rows(table) {
        let css = field(row[0]);
        let ours = parsed(css);
        let want = Rgba {
            r: at(&row, 2),
            g: at(&row, 3),
            b: at(&row, 4),
            a: at(&row, 5),
        };
        assert!(
            ours.r == want.r && ours.g == want.g && ours.b == want.b,
            "{css} parses to ({}, {}, {}) where the rule says ({}, {}, {})",
            ours.r,
            ours.g,
            ours.b,
            want.r,
            want.g,
            want.b
        );
        assert!(
            ours.a == want.a,
            "{css} parses to alpha {} where the author wrote {}",
            ours.a,
            want.a
        );
        checked += 1;
    }
    assert_eq!(checked, 18);
}

#[test]
fn a_colour_mixes_where_v9_mixes() {
    // Asserted to byte precision, all a hex string holds: v9's `mixColor`
    // returns CSS strings and this crate returns `Rgba`, so `127.5` here
    // meets v9's `#808080`, which reads back as 128. The JavaScript walker
    // checks the full string; the two together cover `mixColor`.
    let table = include_str!("assets/animate/mix-color.tsv");
    let (mut compared, mut skipped) = (0, 0);
    for row in rows(table) {
        if field(row[3]) != "value" {
            skipped += 1;
            continue;
        }
        let ours =
            mix(parsed(field(row[0])), parsed(field(row[1])), at(&row, 2));
        let want = parsed(field(row[4]));
        let byte = |channel: f64| channel.round();
        assert!(
            byte(ours.r) == byte(want.r)
                && byte(ours.g) == byte(want.g)
                && byte(ours.b) == byte(want.b),
            "mixing {} and {} at {} gives ({}, {}, {}) where v9 draws {}",
            field(row[0]),
            field(row[1]),
            field(row[2]),
            ours.r,
            ours.g,
            ours.b,
            field(row[4])
        );
        // Alpha survives a string exactly where the channels do not: v9 writes
        // it as a decimal rather than a byte.
        assert!(
            ours.a == want.a,
            "mixing {} and {} at {} gives alpha {} where v9 draws {}",
            field(row[0]),
            field(row[1]),
            field(row[2]),
            ours.a,
            want.a
        );
        compared += 1;
    }
    // Mostly divergent by design; assert the agreeing rows were reached rather
    // than all skipped.
    assert_eq!((compared, skipped), (7, 8));
}

/// The `motion` column: an easing name, or a spring's four numbers.
fn motion_of(text: &str) -> Motion {
    text.strip_prefix("spring:").map_or_else(
        || Motion::Ease(easing_named(text)),
        |shape| {
            let n: Vec<f64> = shape
                .split(':')
                .map(|v| v.parse().unwrap_or_else(|e| unreachable!("{e}")))
                .collect();
            Motion::Spring(Shape {
                stiffness: n[0],
                damping: n[1],
                mass: n[2],
                velocity: n[3],
            })
        },
    )
}

/// A field that may be absent, spelled `-`.
fn maybe(row: &[&str], index: usize) -> Option<f64> {
    (row[index] != "-").then(|| at(row, index))
}

/// Asks one motion how long it lasts through [`Sampled`], so one walker over
/// all three types also asserts they share the trait. `at` is left out: its
/// `Value` is `f64` for a track and a plan and `Vec<f64>` for a group.
fn how_long<M: Sampled>(motion: &M, row: &[&str], kind: usize) -> f64 {
    match row[kind] {
        "duration" => motion
            .duration()
            .unwrap_or_else(|error| unreachable!("{error}")),
        "totalDuration" => motion
            .total_duration(at(row, kind + 3) as usize)
            .unwrap_or_else(|error| unreachable!("{error}")),
        other => unreachable!("{other} is not a question this helper answers"),
    }
}

/// Asks a single-valued motion any of the three questions.
fn answer<M: Sampled<Value = f64>>(
    motion: &M,
    row: &[&str],
    kind: usize,
) -> f64 {
    if row[kind] == "at" {
        return motion
            .at(at(row, kind + 1), at(row, kind + 2) as usize)
            .unwrap_or_else(|error| unreachable!("{error}"));
    }
    how_long(motion, row, kind)
}

#[test]
fn a_track_answers_where_v9_answers() {
    let table = include_str!("assets/animate/track.tsv");
    let (mut compared, mut skipped) = (0, 0);
    for row in rows(table) {
        // `js-only` is the fractional count: `total_duration` takes a `usize`
        // and cannot be asked. A recorded shape difference, not a defect.
        if row[6] != "both" {
            skipped += 1;
            continue;
        }
        let motion = Track {
            from: at(&row, 0),
            to: at(&row, 1),
            duration: maybe(&row, 2),
            delay: at(&row, 3),
            stagger: at(&row, 4),
            motion: motion_of(row[5]),
        };
        let ours = answer(&motion, &row, 7);
        let expected = at(&row, 11);
        assert!(
            ours == expected,
            "a track {row:?} answers {ours} where v9 says {expected}"
        );
        compared += 1;
    }
    assert_eq!((compared, skipped), (126, 6));
}

#[test]
fn a_sequence_answers_where_v9_answers() {
    let table = include_str!("assets/animate/sequence.tsv");
    let mut compared = 0;
    for row in rows(table) {
        let steps = row[3]
            .split(';')
            .map(|leg| {
                let part: Vec<&str> = leg.split(':').collect();
                Step {
                    to: part[0].parse().unwrap_or_else(|e| unreachable!("{e}")),
                    duration: (part[1] != "-").then(|| {
                        part[1].parse().unwrap_or_else(|e| unreachable!("{e}"))
                    }),
                    hold: part[2]
                        .parse()
                        .unwrap_or_else(|e| unreachable!("{e}")),
                    motion: motion_of(part[3]),
                }
            })
            .collect();
        let plan = Sequence {
            from: at(&row, 0),
            steps,
            delay: at(&row, 1),
            stagger: at(&row, 2),
        }
        .plan()
        .unwrap_or_else(|error| unreachable!("{error}"));
        let ours = answer(&plan, &row, 5);
        let expected = at(&row, 9);
        assert!(
            ours == expected,
            "a sequence {row:?} answers {ours} where v9 says {expected}"
        );
        compared += 1;
    }
    assert_eq!(compared, 80);
}

/// The member vocabulary `parallel.tsv` declares in its header.
fn member_named(letter: &str) -> Member<f64> {
    match letter {
        "A" => Member::Track(Track {
            from: 0.0,
            to: 100.0,
            duration: Some(1.0),
            delay: 0.0,
            stagger: 0.0,
            motion: Motion::Ease(Easing::OutCubic),
        }),
        "B" => Member::Sequence(
            Sequence {
                from: 0.0,
                steps: vec![Step {
                    to: 4.0,
                    duration: Some(2.0),
                    hold: 0.0,
                    motion: Motion::Ease(Easing::Linear),
                }],
                delay: 0.0,
                stagger: 0.0,
            }
            .plan()
            .unwrap_or_else(|error| unreachable!("{error}")),
        ),
        "C" => Member::Track(Track {
            from: 0.0,
            to: 5.0,
            duration: Some(1.0),
            delay: 0.0,
            stagger: 1.0,
            motion: Motion::Ease(Easing::Linear),
        }),
        other => unreachable!("{other} is not a member this table declares"),
    }
}

#[test]
fn a_group_answers_where_v9_answers() {
    // The rows include each member alone as well as in company, so a member
    // built differently here from the JavaScript side is caught directly
    // rather than as a group disagreement whose cause is a guess.
    let table = include_str!("assets/animate/parallel.tsv");
    let mut compared = 0;
    for row in rows(table) {
        let names: Vec<&str> = row[0].split(';').collect();
        let group = Parallel::new(
            names
                .iter()
                .map(|letter| ((*letter).to_owned(), member_named(letter)))
                .collect(),
        )
        .unwrap_or_else(|error| unreachable!("{error}"));
        let expected = at(&row, 7);
        let ours = if row[2] == "at" {
            // The group's value is a `Vec` in declaration order here and a
            // record in JavaScript, so the row names one member and the walker
            // finds its position rather than assuming either container.
            let values = group
                .at(at(&row, 3), at(&row, 4) as usize)
                .unwrap_or_else(|error| unreachable!("{error}"));
            let wanted = row[6];
            let position = names
                .iter()
                .position(|letter| *letter == wanted)
                .unwrap_or_else(|| {
                    unreachable!("{wanted} is not in this group")
                });
            values[position]
        } else {
            how_long(&group, &row, 2)
        };
        assert!(
            ours == expected,
            "a group {row:?} answers {ours} where v9 says {expected}"
        );
        compared += 1;
    }
    assert_eq!(compared, 122);
}
