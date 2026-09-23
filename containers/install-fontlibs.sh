#!/usr/bin/env bash
# freetype and fontconfig, static with optional dependencies off: the
# distribution's `libfontconfig.a` drags `libexpat`, `libbz2`, `libpng16` and
# `libbrotlidec` onto the link. Only expat comes, static too. With no shared
# object, rust-skia's unconditional `-lfontconfig` resolves to the archive.
set -euxo pipefail

# Everything is position-independent: the addon is a shared object, and
# autotools builds static archives non-PIC, failing the final link an hour in on
# `relocation R_X86_64_32 against hidden symbol tt_driver_class`. Autotools gets
# `--with-pic` and `-fPIC`, meson `-Db_staticpic=true`.
export CFLAGS="${CFLAGS:-} -fPIC"

# Installed into `/usr` because Skia's `BUILD.gn` hard-codes
# `/usr/include/freetype2`; anywhere else Skia compiles its own non-PIC freetype
# and the link fails as above. `LIBDIR` defaults to `lib64`, where EL's linker
# and pkg-config look; the musl image passes `lib`.
PREFIX=/usr
LIBDIR="${LIBDIR:-lib64}"
mkdir -p "$PREFIX" /opt/src
cd /opt/src

# Fetched to a file and checksummed before unpacking, with a second source:
# `curl --retry` covers only a request failing before data arrives, so streaming
# into `tar` turns a stall into a partial archive. A file lets `--retry` and
# `--speed-time` work.
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

# Each checksum was taken by fetching and corroborated by a second, independent
# host; for expat that is Buildroot's source cache, since the distributions
# publish no SHA256 of this `.tar.xz`.

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

# fontconfig, static, no tools and no tests; `--wrap-mode=nofallback` refuses to
# download a missing dependency. No `-Dxml-backend=expat`: 2.15.0 rejects that
# option, and expat is its only backend.
FONTCONFIG=fontconfig-2.15.0
# Corroborated: freedesktop.org and osuosl return the same bytes.
fetch 63a0658d0e06e0fa886106452b58ef04f21f58202ea02a94c39de0d3335d7c0e "$FONTCONFIG.tar.xz" \
  "https://www.freedesktop.org/software/fontconfig/release/$FONTCONFIG.tar.xz" \
  "https://ftp.osuosl.org/pub/blfs/conglomeration/fontconfig/$FONTCONFIG.tar.xz"
tar xJf "$FONTCONFIG.tar.xz"
cd "$FONTCONFIG"
# `--libdir` so meson installs where the autotools builds above do,
# `$PREFIX/$LIBDIR`, and one `PKG_CONFIG_PATH` names all three archives.
PKG_CONFIG_PATH="$PREFIX/$LIBDIR/pkgconfig" meson setup build \
  --prefix="$PREFIX" --libdir="$LIBDIR" --default-library=static --wrap-mode=nofallback \
  -Db_staticpic=true \
  -Dtests=disabled -Dtools=disabled -Ddoc=disabled -Dcache-build=disabled
meson compile -C build
meson install -C build

# No linker name for expat: the base image's `-devel` symlink would win over the
# archive and put `libexpat.so.1`, absent from every target image, in the
# addon's NEEDED list. `libexpat.so.1` stays, because dnf links against it.
rm -f "$PREFIX/$LIBDIR/libexpat.so" /usr/lib/libexpat.so

# No linker name for any of the three, so `-lfoo` can only reach the archive.
# `.so.N` files are deliberately not matched: they are what a *running* program
# resolves, they do not participate in a link, and one of them belongs to the
# image's own package manager.
find "$PREFIX/$LIBDIR" /usr/lib -maxdepth 1 \
  \( -name 'libfreetype.so' -o -name 'libfontconfig.so' -o -name 'libexpat.so' \) \
  2>/dev/null | tee /tmp/shared.txt
test ! -s /tmp/shared.txt || { echo "a linker name survived; -lfoo would find a shared object and the static link would silently not happen" >&2; exit 1; }

# Every archive proved usable in a shared object by linking one, not by counting
# `R_X86_64_32`: debug sections hold legal absolute relocations, so a count
# flags the wrong archives. Unresolved symbols are ignored, since each library
# links alone.
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
