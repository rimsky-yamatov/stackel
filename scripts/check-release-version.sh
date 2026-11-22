#!/usr/bin/env bash
# Checks that a release tag matches the version in Cargo.toml and prints, as key=value lines
# (the format of $GITHUB_OUTPUT):
#
#   version=1.2.3          the version without the leading "v"
#   prerelease=true|false  true when the version has a pre-release part (1.2.3-rc.1)
#
#   scripts/check-release-version.sh v1.2.3
set -euo pipefail

if [ $# -ne 1 ]; then
  echo "usage: $0 <tag>" >&2
  exit 2
fi
tag="$1"

cd "$(dirname "$0")/.." || exit 2

semver_re='^v([0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?)$'
if [[ ! "$tag" =~ $semver_re ]]; then
  echo "error: the tag '$tag' is not of the form vMAJOR.MINOR.PATCH[-PRERELEASE]" >&2
  exit 1
fi
version="${BASH_REMATCH[1]}"

crate="$(sed -n 's/^version *= *"\(.*\)"$/\1/p' Cargo.toml | head -n 1)"
if [ "$crate" != "$version" ]; then
  echo "error: the tag is $tag but Cargo.toml says version $crate; bump the version and tag again" >&2
  exit 1
fi

echo "version=$version"
if [[ "$version" == *-* ]]; then
  echo "prerelease=true"
else
  echo "prerelease=false"
fi
