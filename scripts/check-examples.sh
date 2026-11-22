#!/usr/bin/env bash
# Runs every program in examples/ and compares its output with the golden file.
# Same conventions as tests/examples.rs (NAME.expected, NAME.stdin, NAME.args, NAME.cmd),
# for runs where `cargo test` cannot start the binary (cross-compiled targets under QEMU).
#
#   STACKEL_CMD="target/release/stackel" scripts/check-examples.sh
#   STACKEL_CMD="cross run --release --quiet --target riscv64gc-unknown-linux-gnu --" \
#     scripts/check-examples.sh
set -u

if [ -z "${STACKEL_CMD:-}" ]; then
  echo "set STACKEL_CMD to the command that starts stackel" >&2
  exit 2
fi
read -r -a runner <<< "$STACKEL_CMD"

cd "$(dirname "$0")/.." || exit 2
failed=0
checked=0

for path in examples/*.stk examples/*/; do
  path="${path%/}"
  # A directory is an example only if it is a package.
  if [ -d "$path" ] && [ ! -f "$path/stackel.toml" ]; then continue; fi
  [ -e "$path" ] || continue
  name="$(basename "$path" .stk)"
  expected="examples/$name.expected"
  if [ ! -f "$expected" ]; then
    echo "FAIL $path: no golden file $expected"
    failed=$((failed + 1))
    continue
  fi

  cmd="run"
  [ -f "examples/$name.cmd" ] && cmd="$(tr -d '[:space:]' < "examples/$name.cmd")"
  args=()
  if [ -f "examples/$name.args" ]; then
    while IFS= read -r line || [ -n "$line" ]; do
      [ -n "$line" ] && args+=("$line")
    done < "examples/$name.args"
  fi
  input=/dev/null
  [ -f "examples/$name.stdin" ] && input="examples/$name.stdin"

  actual="$("${runner[@]}" "$cmd" "$path" ${args[@]+"${args[@]}"} < "$input" 2>&1; echo "exit=$?")"
  want="$(cat "$expected"; echo "exit=0")"
  # `$(...)` strips trailing newlines, so both sides lose them the same way.
  if [ "$actual" = "$want" ]; then
    checked=$((checked + 1))
  else
    echo "FAIL $path"
    diff <(printf '%s\n' "$want") <(printf '%s\n' "$actual") | head -20
    failed=$((failed + 1))
  fi
done

echo "$checked examples passed, $failed failed"
[ "$failed" -eq 0 ]
