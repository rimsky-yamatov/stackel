#!/usr/bin/env bash
# Packs a built stackel binary into a release archive and writes its SHA-256 checksum.
#
#   scripts/package.sh <target> [binary]
#
# <target>  the Rust target triple, used in the archive name (stackel-<version>-<target>)
# [binary]  the built binary (default: target/<target>/release/stackel[.exe], then
#           target/release/stackel[.exe])
#
# Writes dist/stackel-<version>-<target>.tar.gz (.zip for Windows) and a .sha256 file next to
# it, and prints the archive path. The archive holds one top-level directory with the binary,
# the README and the examples.
set -euo pipefail

if [ $# -lt 1 ] || [ $# -gt 2 ]; then
  echo "usage: $0 <target> [binary]" >&2
  exit 2
fi
target="$1"

cd "$(dirname "$0")/.."

version="$(sed -n 's/^version *= *"\(.*\)"$/\1/p' Cargo.toml | head -n 1)"
if [ -z "$version" ]; then
  echo "cannot read the version from Cargo.toml" >&2
  exit 1
fi

case "$target" in
  *windows*) exe="stackel.exe"; format="zip" ;;
  *) exe="stackel"; format="tar.gz" ;;
esac

if [ $# -eq 2 ]; then
  binary="$2"
elif [ -f "target/$target/release/$exe" ]; then
  binary="target/$target/release/$exe"
else
  binary="target/release/$exe"
fi
if [ ! -f "$binary" ]; then
  echo "binary not found: $binary" >&2
  exit 1
fi

name="stackel-$version-$target"
stage="dist/$name"
rm -rf "$stage" "dist/$name.tar.gz" "dist/$name.zip" "dist/$name.$format.sha256"
mkdir -p "$stage"

cp "$binary" "$stage/$exe"
cp README.md "$stage/"
for f in LICENSE LICENSE-MIT LICENSE-APACHE; do
  [ -f "$f" ] && cp "$f" "$stage/"
done
# The examples without their golden files: those belong to the test suite.
mkdir -p "$stage/examples"
cp -R examples/. "$stage/examples/"
find "$stage/examples" \( -name '*.expected' -o -name '*.stdin' -o -name '*.args' -o -name '*.cmd' \) -delete

archive="dist/$name.$format"
if [ "$format" = "zip" ]; then
  if command -v zip >/dev/null 2>&1; then
    (cd dist && zip -qr "$name.zip" "$name")
  elif command -v 7z >/dev/null 2>&1; then
    (cd dist && 7z a -tzip -bso0 -bsp0 "$name.zip" "$name")
  else
    # Windows runners always have PowerShell.
    (cd dist && powershell -NoProfile -Command \
      "Compress-Archive -Path '$name' -DestinationPath '$name.zip'")
  fi
else
  tar -C dist -czf "$archive" "$name"
fi
rm -rf "$stage"

if command -v sha256sum >/dev/null 2>&1; then
  (cd dist && sha256sum "$name.$format" > "$name.$format.sha256")
else
  (cd dist && shasum -a 256 "$name.$format" > "$name.$format.sha256")
fi

echo "$archive"
