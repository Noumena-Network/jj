#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
bin="${1:-dist/jj}"
test -x "$bin"
bin_abs="$(cd "$(dirname "$bin")" && pwd -P)/$(basename "$bin")"
"$bin_abs" --version
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
(
  cd "$work"
  "$bin_abs" git init repo >/dev/null
  cd repo
  "$bin_abs" status >/dev/null
)
