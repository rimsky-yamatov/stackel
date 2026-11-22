#!/usr/bin/env bash
# Tests scripts/release-notes.sh against a throw-away git repository.
set -euo pipefail

notes="$(cd "$(dirname "$0")" && pwd)/release-notes.sh"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
cd "$work"

export GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@example.com
export GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@example.com
git init -q -b main .

n=0
commit() {
  n=$((n + 1))
  echo "$n" > file
  git add file
  git commit -q -m "$1"
}

failures=0
contains() { # <name> <text> <needle>
  if ! grep -Fq -- "$3" <<< "$2"; then
    echo "FAIL $1: missing: $3"
    while IFS= read -r line; do echo "    | $line"; done <<< "$2"
    failures=$((failures + 1))
  fi
}
lacks() { # <name> <text> <needle>
  if grep -Fq -- "$3" <<< "$2"; then
    echo "FAIL $1: unexpected: $3"
    failures=$((failures + 1))
  fi
}

commit "feat: first feature"
commit "fix(parser): first fix"
git tag v0.1.0

commit "feat(cli)!: drop the old flag"
commit "fix: another fix"
commit "perf(eval): faster calls"
commit "chore: tidy"
commit "something without a type"
commit "wip: unknown type"
n=$((n + 1)); echo "$n" > file; git add file
git commit -q -m "refactor: rename things" -m "BREAKING CHANGE: the module layout changed"
git checkout -q -b side
commit "fix: from a branch"
git checkout -q main
git merge -q --no-ff side -m "Merge branch 'side'"

# A tag compared with the nearest earlier tag.
git tag v0.2.0
out="$(NOTES_REPO_URL=https://example.com/o/r "$notes" v0.2.0)"
contains "grouped" "$out" "### Breaking changes"
contains "breaking bang" "$out" "- **cli:** drop the old flag"
contains "breaking footer" "$out" "- rename things"
contains "features" "$out" "## What's changed"
contains "fix section" "$out" "### Bug fixes"
contains "fix text" "$out" "- another fix"
contains "scope" "$out" "- **eval:** faster calls"
contains "chore section" "$out" "### Chores"
contains "no type" "$out" "- something without a type"
contains "unknown type" "$out" "- wip: unknown type"
contains "other section" "$out" "### Other changes"
contains "branch commit" "$out" "- from a branch"
contains "link" "$out" "(https://example.com/o/r/commit/"
contains "compare" "$out" "**Full changelog:** https://example.com/o/r/compare/v0.1.0...v0.2.0"
lacks "old commits" "$out" "first feature"
lacks "merge commits" "$out" "Merge branch"
# Sections appear in a fixed order: breaking changes first, then bug fixes, then chores.
order="$(grep -n '^### ' <<< "$out" | sed 's/^[0-9]*://' | tr '\n' ',')"
if [ "$order" != "### Breaking changes,### Bug fixes,### Performance,### Chores,### Other changes," ]; then
  echo "FAIL order: $order"
  failures=$((failures + 1))
fi

# The first release: no previous tag, so every commit is listed and there is no comparison.
out="$("$notes" v0.1.0)"
contains "first release" "$out" "- first feature"
contains "first release scope" "$out" "- **parser:** first fix"
lacks "first release compare" "$out" "Full changelog"

# An explicit previous tag.
out="$("$notes" v0.2.0 v0.1.0)"
contains "explicit previous" "$out" "- another fix"

# An untagged ref (a dry run on HEAD) is compared with the latest tag below it.
commit "feat: after the release"
out="$("$notes" HEAD)"
contains "head" "$out" "- after the release"
lacks "head old" "$out" "another fix"

# Nothing new since the previous tag.
out="$("$notes" v0.2.0 v0.2.0)"
contains "empty" "$out" "No changes since v0.2.0."

if [ "$failures" -ne 0 ]; then
  echo "$failures check(s) failed"
  exit 1
fi
echo "release-notes: all checks passed"
