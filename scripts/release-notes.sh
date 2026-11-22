#!/usr/bin/env bash
# Writes release notes in Markdown from the commit messages between two tags.
#
#   scripts/release-notes.sh <tag-or-ref> [previous-tag]
#
# Commits are grouped by their Conventional Commits type (feat, fix, ...). The previous tag
# defaults to the nearest earlier tag that looks like v1.2.3; with none, every commit up to
# <tag-or-ref> is listed. <tag-or-ref> may also be HEAD (used by dry runs of the release
# workflow). Merge commits are left out.
#
# Environment: NOTES_REPO_URL (https://github.com/owner/repo) turns commit hashes and the
# comparison into links; it defaults to $GITHUB_SERVER_URL/$GITHUB_REPOSITORY when both are set.
set -euo pipefail

if [ $# -lt 1 ] || [ $# -gt 2 ]; then
  echo "usage: $0 <tag-or-ref> [previous-tag]" >&2
  exit 2
fi
to="$1"
prev="${2:-}"

repo_url="${NOTES_REPO_URL:-}"
if [ -z "$repo_url" ] && [ -n "${GITHUB_SERVER_URL:-}" ] && [ -n "${GITHUB_REPOSITORY:-}" ]; then
  repo_url="$GITHUB_SERVER_URL/$GITHUB_REPOSITORY"
fi

if [ -z "$prev" ]; then
  # A tagged commit is compared with what came before it; an untagged ref (HEAD) with the
  # latest tag below it.
  if git describe --tags --exact-match --match 'v[0-9]*' "$to" >/dev/null 2>&1; then
    base="$to^"
  else
    base="$to"
  fi
  prev="$(git describe --tags --abbrev=0 --match 'v[0-9]*' "$base" 2>/dev/null || true)"
fi

if [ -n "$prev" ]; then range="$prev..$to"; else range="$to"; fi

# type -> section title, in the order the sections are written.
sections=(
  "breaking|Breaking changes"
  "feat|Features"
  "fix|Bug fixes"
  "perf|Performance"
  "refactor|Refactoring"
  "examples|Examples"
  "docs|Documentation"
  "test|Tests"
  "build|Build"
  "ci|CI and releases"
  "style|Style"
  "chore|Chores"
  "other|Other changes"
)

declare -A body
count=0
header_re='^([a-z]+)(\(([^)]*)\))?(!)?: (.+)$'

while IFS=$'\t' read -r hash subject; do
  [ -n "$hash" ] || continue
  type="other"
  scope=""
  text="$subject"
  breaking=0
  if [[ "$subject" =~ $header_re ]]; then
    type="${BASH_REMATCH[1]}"
    scope="${BASH_REMATCH[3]}"
    [ -n "${BASH_REMATCH[4]}" ] && breaking=1
    text="${BASH_REMATCH[5]}"
  fi
  if git show -s --format=%B "$hash" | grep -Eq '^BREAKING[ -]CHANGE'; then
    breaking=1
  fi
  known=0
  for s in "${sections[@]}"; do
    [ "${s%%|*}" = "$type" ] && known=1
  done
  [ "$known" -eq 1 ] || { type="other"; text="$subject"; scope=""; }

  line="- "
  [ -n "$scope" ] && line+="**$scope:** "
  line+="$text"
  if [ -n "$repo_url" ]; then
    line+=" ([\`$hash\`]($repo_url/commit/$hash))"
  else
    line+=" (\`$hash\`)"
  fi
  if [ "$breaking" -eq 1 ]; then
    body[breaking]+="$line"$'\n'
  else
    body[$type]+="$line"$'\n'
  fi
  count=$((count + 1))
done < <(git log --no-merges --reverse --format='%h%x09%s' "$range")

echo "## What's changed"
echo
if [ "$count" -eq 0 ]; then
  echo "No changes since ${prev:-the beginning}."
else
  for s in "${sections[@]}"; do
    key="${s%%|*}"
    title="${s#*|}"
    if [ -n "${body[$key]:-}" ]; then
      echo "### $title"
      echo
      printf '%s\n' "${body[$key]}"
    fi
  done
fi

if [ -n "$repo_url" ] && [ -n "$prev" ]; then
  echo "**Full changelog:** $repo_url/compare/$prev...$to"
fi
