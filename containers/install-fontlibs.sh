#!/usr/bin/env bash
# freetype and fontconfig, built static with their optional dependencies off.
#
# The optional dependencies are the point. Ubuntu's own `libfontconfig.a` links
# perfectly well and drags `libexpat`, `libbz2`, `libpng16` and `libbrotlidec`
# onto the link line with it -- three of which are missing from every image the
# package claims. Built here, freetype takes none of them and fontconfig takes
# only expat, which is built static alongside it.
#
# Nothing is installed to a system directory and **no shared object is
# produced**, which is what makes the static link happen without patching
# rust-skia: with no `libfontconfig.so` to find, its unconditional
# `-lfontconfig` resolves to the archive.
set -euxo pipefail

# **Everything here is built position-independent, and it is not optional.**
#
# The addon is a shared object, so every object linked into it must be PIC.
# Autotools builds a static library non-PIC by default, and the failure is late
# and does not name the cause:
#
#   ld: libskia_bindings.rlib(ftinit.o): relocation R_X86_64_32 against hidden
#       symbol `tt_driver_class' can not be used when making a shared object
#
# `ftinit.o` is freetype's, reached through Skia, and the message says
# "relocation" rather than "this library is not PIC". It appears only at the
# final link of the whole addon, after Skia has compiled -- which is an hour in
# on a cold build. It did not happen while freetype was the distribution's
# shared library, because a shared library is PIC by construction; it starts the
# moment the static archives here are the ones being absorbed.
#
# All three need saying so explicitly. The two autotools builds take both
# halves -- `--with-pic` is libtool's switch, `-fPIC` covers anything compiled
# outside libtool -- and meson takes `-Db_staticpic=true`.
#
# meson's `b_staticpic` is passed explicitly rather than relied on, which costs
# nothing and removes a default from the reasoning.
export CFLAGS="${CFLAGS:-} -fPIC"

# Installed into `/usr`, not a private prefix, and Skia is the reason.
#
# `third_party/freetype2/BUILD.gn` **hard-codes `/usr/include/freetype2`** as
# its include path -- rust-skia's own comment says so
# (`build_support/skia/config.rs:206`). With the libraries in `/opt/fontlibs`,
# Skia could not find a system freetype and compiled its own, non-PIC, into
# `libskia.a`. The addon then failed at its final link on
# `libskia_bindings.rlib(ftinit.o): relocation R_X86_64_32 against hidden
# symbol tt_driver_class` -- a freetype object we never built, reached through
# a library we did not know was carrying one.
#
# `lib64` because that is where EL puts 64-bit libraries and what the linker
# and pkg-config search without being told. **Alpine uses `lib` for everything**,
# so the musl sibling of this image passes `LIBDIR=lib` and the rest of this
# script is shared between the two families unchanged.
PREFIX=/usr
LIBDIR="${LIBDIR:-lib64}"
mkdir -p "$PREFIX" /opt/src
cd /opt/src

# Fetched to a file and checksummed before anything is unpacked, with a second
# source. `meo-skia-canvas` had five truncated transfers across three machines
# and an outright HTTP error from one host during a single release: `curl`'s
# `--retry` only covers a request that fails before any data arrives, so a
# stall mid-stream leaves `tar` holding a partial archive with nothing to
# resume from. Downloading first makes `--retry` mean something and lets
# `--speed-time` trip on a stall.
fetch() {
  local sha="$1" out="$2"; shift 2
  local url
  for url in "$@"; do
    if curl -sfL --retry 5 --retry-delay 2 --speed-limit 1024 --speed-time 30 -o "$out" "$url"; then
      if echo "$sha  $out" | sha256sum -c - >/dev/null 2>&1; then return 0; fi
      echo "checksum mismatch from $url, trying the next source" >&2
    else
      echo "fetch failed from $url, trying the next source" >&2
    fi
  done
  echo "every source failed for a tarball with sha256 $sha" >&2
  return 1
}

# The checksums were taken by fetching, not from memory -- the first version of
# this script carried three remembered sums, two were wrong, and the guard
# caught it rather than the build.
#
# **All three are corroborated by a second, independent host.** expat took some
# finding: its release lives on GitHub, and SourceForge, Debian's pool, openSUSE
# and Gentoo's distfiles all refused the fetch or do not carry that exact
# archive. Buildroot's source cache does, and returns the same bytes -- which
# makes it two parties rather than one. The distributions were no help for a
# different reason worth knowing: they have moved to expat 2.8, and they publish
# SHA512 of a `.tar.gz` or BLAKE2 sums, so none of them states a SHA256 of this
# `.tar.xz` to compare against at all.

# expat, for fontconfig's XML backend. Static only.
EXPAT=expat-2.6.4
fetch a695629dae047055b37d50a0ff4776d1d45d0a4c842cf4ccee158441f55ff7ee "$EXPAT.tar.xz" \
  "https://github.com/libexpat/libexpat/releases/download/R_2_6_4/$EXPAT.tar.xz" \
  "http://sources.buildroot.net/expat/$EXPAT.tar.xz"
tar xJf "$EXPAT.tar.xz"
cd "$EXPAT"
./configure --prefix="$PREFIX" --libdir="$PREFIX/$LIBDIR" \
  --enable-static --disable-shared --with-pic \
  --without-docbook --without-examples --without-tests
make -j"$(nproc)" && make install
cd /opt/src

# freetype, with every optional dependency off. `--without-zlib` matters: it
# uses its own bundled copy rather than adding libz to the link line, and the
# whole point of building this is controlling what comes with it.
FREETYPE=freetype-2.13.3
fetch 0550350666d427c74daeb85d5ac7bb353acba5f76956395995311a9c6f063289 "$FREETYPE.tar.xz" \
  "https://downloads.sourceforge.net/project/freetype/freetype2/2.13.3/$FREETYPE.tar.xz" \
  "https://download.savannah.gnu.org/releases/freetype/$FREETYPE.tar.xz"
tar xJf "$FREETYPE.tar.xz"
cd "$FREETYPE"
./configure --prefix="$PREFIX" --libdir="$PREFIX/$LIBDIR" \
  --enable-static --disable-shared --with-pic \
  --with-zlib=no --with-bzip2=no --with-png=no --with-brotli=no --with-harfbuzz=no
make -j"$(nproc)" && make install
cd /opt/src

# fontconfig, static, no tools and no tests. `--wrap-mode=nofallback` refuses to
# silently download a dependency it cannot find, which is what makes the flags
# above meaningful rather than advisory.
#
# **No `-Dxml-backend=expat` here, and the version is the reason.** That option
# arrived after 2.15: `meo-skia-canvas` passes it because it builds 2.17.1,
# and on 2.15.0 meson refuses the whole setup with `Unknown option:
# "xml-backend"`. 2.15 has no choice to make -- expat is the only backend --
# so the flag would be stating a default that cannot be otherwise. Moving to
# 2.17 later means adding it back.
FONTCONFIG=fontconfig-2.15.0
# Corroborated: freedesktop.org and osuosl return the same bytes.
fetch 63a0658d0e06e0fa886106452b58ef04f21f58202ea02a94c39de0d3335d7c0e "$FONTCONFIG.tar.xz" \
  "https://www.freedesktop.org/software/fontconfig/release/$FONTCONFIG.tar.xz" \
  "https://ftp.osuosl.org/pub/blfs/conglomeration/fontconfig/$FONTCONFIG.tar.xz"
tar xJf "$FONTCONFIG.tar.xz"
cd "$FONTCONFIG"
# `--libdir=lib` on purpose. meson follows the distribution's convention and
# EL puts 64-bit libraries in `lib64`, while the autotools builds above use
# `lib` -- so without this the three archives land in two directories and
# `PKG_CONFIG_PATH` has to name both. Alpine, where the musl sibling of this
# image is built, uses `lib` for everything, so normalising here is also what
# keeps one `PKG_CONFIG_PATH` correct for both families.
PKG_CONFIG_PATH="$PREFIX/$LIBDIR/pkgconfig" meson setup build \
  --prefix="$PREFIX" --libdir="$LIBDIR" --default-library=static --wrap-mode=nofallback \
  -Db_staticpic=true \
  -Dtests=disabled -Dtools=disabled -Ddoc=disabled -Dcache-build=disabled
meson compile -C build
meson install -C build

# No shared object anywhere in the prefix. This is load-bearing rather than
# tidiness: rust-skia's `-lfontconfig` cannot be removed without patching its
# build script, so what it resolves to is decided by what exists on disk.
# The base image ships expat, and its `-devel` symlink would win.
#
# `-lexpat` resolves `libexpat.so` before `libexpat.a`, so the system's own
# development symlink is enough to put `libexpat.so.1` back in the addon's
# NEEDED list -- and measured, libexpat is absent from every image this package
# claims. Only the **linker name** is removed: `libexpat.so.1` stays, because
# dnf itself links against it and this image still has to work.
rm -f "$PREFIX/$LIBDIR/libexpat.so" /usr/lib/libexpat.so

# No linker name for any of the three, so `-lfoo` can only reach the archive.
# `.so.N` files are deliberately not matched: they are what a *running* program
# resolves, they do not participate in a link, and one of them belongs to the
# image's own package manager.
find "$PREFIX/$LIBDIR" /usr/lib -maxdepth 1 \
  \( -name 'libfreetype.so' -o -name 'libfontconfig.so' -o -name 'libexpat.so' \) \
  2>/dev/null | tee /tmp/shared.txt
test ! -s /tmp/shared.txt || { echo "a linker name survived; -lfoo would find a shared object and the static link would silently not happen" >&2; exit 1; }

# Every archive proved usable in a shared object, by linking one.
#
# **Counting relocations does not answer this, and got it exactly backwards.**
# The first version of this check counted `R_X86_64_32` with `readelf` and
# reported freetype and expat clean and fontconfig broken. The truth was the
# other way round: meson builds with debug info by default and `.rela.debug_*`
# is full of absolute 32-bit relocations that are perfectly legal in a shared
# object, while the two archives that really could not be used had far fewer
# and were passed.
#
# Asking the linker removes the inference. If `ld -shared` accepts the archive
# then it can go into the addon, which is the only property anyone cares about.
# Unresolved symbols are ignored because each library is being linked alone,
# and their absence is not what is being tested.
for archive in "$PREFIX/$LIBDIR"/libexpat.a "$PREFIX/$LIBDIR"/libfreetype.a "$PREFIX/$LIBDIR"/libfontconfig.a; do
  ld -shared --unresolved-symbols=ignore-all --whole-archive "$archive" -o /tmp/pic-check.so 2>/tmp/pic-check.err || {
    echo "$archive cannot be linked into a shared object:" >&2
    grep -m2 'can not be used\|relocation' /tmp/pic-check.err >&2
    echo "the addon is a shared object, so this would fail at its final link -- an hour into a cold build, naming a relocation and an object file rather than a library or a flag" >&2
    exit 1
  }
done
rm -f /tmp/pic-check.so /tmp/pic-check.err

# And every archive in one directory, which is what lets PKG_CONFIG_PATH name
# one path. An empty result here would mean the builds above installed
# somewhere this image does not look.
ls "$PREFIX/$LIBDIR/libexpat.a" "$PREFIX/$LIBDIR/libfreetype.a" "$PREFIX/$LIBDIR/libfontconfig.a"
ls "$PREFIX/$LIBDIR/pkgconfig/fontconfig.pc" "$PREFIX/$LIBDIR/pkgconfig/freetype2.pc"
# The header Skia hard-codes. If this moves, Skia silently builds its own
# freetype again and the failure arrives an hour later at the final link.
ls "$PREFIX/include/freetype2/ft2build.h"

rm -rf /opt/src
