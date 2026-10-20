#!/usr/bin/env bash
# Shared input contracts for every native golden validator.

golden_require_projects() {
  local directory="$1" marker="$2" project count=0
  if [ ! -d "$directory" ]; then
    echo "error: golden dir not found: $directory" >&2
    return 1
  fi
  for project in "$directory"/*/; do
    [ -d "$project" ] || continue
    if [ ! -f "$project/$marker" ]; then
      echo "error: missing $marker in $project" >&2
      return 1
    fi
    count=$((count + 1))
  done
  golden_require_selection "$count"
}

golden_require_selection() {
  if [ "$1" -eq 0 ]; then
    echo "error: no golden projects selected; no compilation was performed" >&2
    return 1
  fi
}
