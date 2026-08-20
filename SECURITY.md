# Security Policy

## Supported versions

Fixes land on the current major, released from `main`. Older majors are not patched — the upgrade
path is the fix.

| Version | Supported |
| ------- | --------- |
| 8.x     | yes       |
| < 8     | no        |

## Reporting a vulnerability

Report privately through
[GitHub's advisory form](https://github.com/l7aromeo/meo-canvas/security/advisories/new). It opens a
channel visible only to you and the maintainer. If that is not available to you, email
<ukasyahrz@outlook.com> instead.

Please do not open a public issue for a vulnerability, and please do not include a working exploit
in one.

Useful in a report: what an attacker controls (a `src` URL, a font file, a colour string, the
content of an image being decoded), what they get out of it, and the smallest input that shows it.

Expect an acknowledgement within a week. If a report is valid, you will be told what the fix is and
when it ships, and credited in the advisory unless you would rather not be.

## What is in scope

This library takes untrusted input in more places than most, so it is worth being specific.

**In scope**

- Input reaching the renderer or layout engine in a way this library is responsible for — a `src`
  path escaping where it should be confined, a decoded image driving an out-of-bounds read through
  a prop this library computes.
- `httpOptions` on `Image`: header injection, a redirect followed somewhere it should not be, a
  request sent with credentials it should not carry.
- The disk cache: a cache key that lets one source overwrite another's entry, or a path derived
  from a URL escaping the cache directory.
- The worker pool: anything crossing the Comlink boundary that lets a rendered document reach the
  host beyond the drawing API.
- Denial of service that a caller cannot bound — an input that hangs a render rather than one that
  makes it slow.

**Not in scope**

- Vulnerabilities in `meo-skia-canvas` or `yoga-layout` themselves. Report those upstream; if a
  version bump is needed here, this repository will follow.
- Rendering attacker-supplied text or images at attacker-chosen sizes and running out of memory.
  Sizes come from your code, and bounding them is yours to do.
- Fetching a URL a caller passed in. `Image` fetches what it is given; deciding what may be given
  to it is the caller's job.
