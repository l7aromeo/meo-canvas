# Security Policy

## Supported versions

Fixes land on the current major. Older majors are not patched — the upgrade path is the fix.

| What                             | Where            | Supported                    |
| -------------------------------- | ---------------- | ---------------------------- |
| `meo-canvas` 9.x                 | npm, `latest`    | yes                          |
| `meo-canvas` 10.x                | not yet released | n/a                          |
| `meo-canvas` < 9                 | npm              | no                           |
| `meo-canvas*` crates             | not published    | n/a                          |
| `meo-canvas-<platform>` packages | npm              | with the version they pin to |

10.x is being built on the `v10` branch and nothing from it is on npm: `npm view meo-canvas dist-tags`
answers `latest: 9.0.3` and no other tag. The Rust crates are not on crates.io. If you are reading
this on the `v10` branch, the published surface is still 9.x, and a report against something that
exists only here is a bug report rather than an advisory — say so and it will be treated as one.

## Reporting a vulnerability

Report privately through
[GitHub's advisory form](https://github.com/l7aromeo/meo-canvas/security/advisories/new). It opens a
channel visible only to you and the maintainer. If that is not available to you, email
<ukasyahrz@outlook.com> instead.

Please do not open a public issue for a vulnerability, and please do not include a working exploit
in one.

Useful in a report: what an attacker controls (a `src` URL, a font file, a colour string, path data,
the bytes of an encoded scene, the content of an image being decoded), what they get out of it, and
the smallest input that shows it.

Expect an acknowledgement within a week. If a report is valid, you will be told what the fix is and
when it ships, and credited in the advisory unless you would rather not be.

## What this project does about dependencies

Stated plainly, because a policy that only says "we take security seriously" tells you nothing you
can check.

`just audit` runs `cargo audit` over `Cargo.lock`, and CI runs it once per push. **Vulnerabilities
fail; unmaintained notices report.** `cargo audit` already draws that line, and failing on an
unmaintained transitive crate would be red for a condition nobody here can resolve, which is how a
gate stops being read.

**Ignored advisories are flags in the recipe, not entries in a config file**, so the reason and the
removal condition sit next to the identifier where a reader will meet them. Two are ignored today,
both denial-of-service findings in `quick-xml` 0.37.5 reached through
`little_exif <- meo-skia-canvas`; the fix is in 0.41.0 and no release of `little_exif` yet permits
it. The recipe records what was checked about reachability, that it is a reading of the call sites
rather than a proof, and the single condition under which both ignores are removed.

Nothing scanned this tree at all until 4 September 2026, when three advisories were found by
querying OSV by hand against the 385 crates in the lockfile. That is the reason the gate exists: a
finding that needs someone to think of looking is not a gate.

## What is in scope

This library takes untrusted input in more places than most, so it is worth being specific.

**In scope**

- The `f64` arena, which is how the JavaScript surface hands a scene to the addon. A crafted arena
  reaching an out-of-bounds read, a panic that crosses the boundary, or a length that is trusted
  rather than checked.
- The scene codec — the self-describing byte format `meo-canvas-cli` reads from a file or a pipe.
  Anything in it that a decoder should refuse and does not.
- Input reaching the renderer or layout engine in a way this library is responsible for: a `src`
  path escaping where it should be confined, a decoded image or a font file driving an
  out-of-bounds read through a value this library computes, SVG path data that is parsed here.
- `httpOptions` on `Root`: header injection, a redirect followed somewhere it should not be, a
  request sent with credentials it should not carry. Only URLs in the scene are fetched, and bytes
  cross to the renderer rather than URLs.
- The addon loader: `MEO_CANVAS_ADDON`, the in-tree path and the platform packages are three ways a
  binary is chosen, and anything that lets one be substituted for another unexpectedly.
- Denial of service that a caller cannot bound — an input that hangs a render rather than one that
  makes it slow.

**Not in scope**

- Vulnerabilities in `meo-skia-canvas`, Skia or `taffy` themselves. Report those upstream; if a
  version bump is needed here, this repository will follow.
- Rendering at attacker-chosen sizes and running out of memory. **Nothing bounds the canvas
  dimensions** and that is documented rather than accidental — sizes come from your code, and
  bounding them is yours to do. See "Sizing a service" in the package README.
- Fetching a URL a caller passed in. The renderer fetches what the scene names; deciding what may
  be named is the caller's job.
- A font registered by one render affecting a later one in the same process. This is real, it is
  documented on `FontRegistration`, and it is a property of the registry below this project rather
  than a defect here — but a service that registers per request should read that note.
