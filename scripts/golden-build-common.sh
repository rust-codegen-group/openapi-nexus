#!/usr/bin/env bash
# Shared inventory preflight for the native golden compilers.

golden_select_projects() (
  local workspace="$1" generator="$2" directory="$3" marker="$4" requested="${5:-native}"
  local catalog="$workspace/tests/golden_cases.tsv" prefix="${directory#"$workspace/tests/golden/"}/"
  local selectors mode name path actual entry
  local -a expected=() fragments=() selected=()

  if [[ ! -d "$directory" || -L "$directory" ]]; then
    echo "error: expected a golden directory: $directory" >&2
    return 1
  fi
  if [[ ! -f "$catalog" ]]; then
    echo "error: golden case catalog not found: $catalog" >&2
    return 1
  fi

  while IFS=$'\t' read -r selectors mode name _ || [[ -n "$selectors" ]]; do
    case "$selectors" in ''|\#*) continue ;; esac
    case ",$selectors," in
      *,"$generator",*) ;;
      ,all,) ;;
      *) continue ;;
    esac
    if [[ "$mode" == fragment ]]; then
      # Selected files are compared as snapshots, not compiled as projects.
      # Recognize the exact ones that share this backend's immediate root.
      if [[ "$name" == "$prefix"* ]]; then
        name="${name#"$prefix"}"
        if [[ "$name" == *.golden && "$name" != */* ]]; then fragments+=("$name"); fi
      fi
      continue
    fi
    if [[ ! "$name" =~ ^[a-zA-Z0-9_-]+$ ]]; then
      echo "error: invalid golden case name for $generator: $name" >&2
      return 1
    fi
    case "$generator:$mode" in
      typescript-fetch:native|typescript-fetch:tsc) mode=tsc ;;
      typescript-fetch:vp|*:native) ;;
      *) echo "error: invalid golden build mode for $generator/$name: $mode" >&2; return 1 ;;
    esac
    if golden_contains "$name" ${expected[@]+"${expected[@]}"}; then
      echo "error: duplicate golden case for $generator: $name" >&2
      return 1
    fi
    expected+=("$name")

    path="$directory/$name"
    if [[ ! -d "$path" || -L "$path" ]]; then
      echo "error: missing registered golden project: $path" >&2
      return 1
    fi
    if [[ ! -f "$path/$marker" || -L "$path/$marker" ]]; then
      echo "error: missing $marker in $path" >&2
      return 1
    fi
    if [[ "$generator" == typescript-fetch ]]; then
      actual=tsc
      if [[ -e "$path/vite.config.ts.golden" || -L "$path/vite.config.ts.golden" ]]; then
        if [[ ! -f "$path/vite.config.ts.golden" || -L "$path/vite.config.ts.golden" ]]; then
          echo "error: invalid Vite+ build marker: $path/vite.config.ts.golden" >&2
          return 1
        fi
        actual=vp
      fi
      if [[ "$mode" != "$actual" ]]; then
        echo "error: $path is registered for $mode but its build markers select $actual" >&2
        return 1
      fi
      if [[ "$mode" == vp && ( ! -f "$path/package.json.golden" || -L "$path/package.json.golden" ) ]]; then
        echo "error: missing package.json.golden in $path" >&2
        return 1
      fi
    fi
    if [[ "$mode" == "$requested" ]]; then selected+=("$name"); fi
  done < "$catalog"

  shopt -s dotglob nullglob
  for entry in "$directory"/*; do
    name="${entry##*/}"
    if [[ -d "$entry" && ! -L "$entry" ]] && golden_contains "$name" ${expected[@]+"${expected[@]}"}; then
      continue
    fi
    if [[ -f "$entry" && ! -L "$entry" ]] && golden_contains "$name" ${fragments[@]+"${fragments[@]}"}; then
      continue
    fi
    echo "error: unregistered golden entry: $entry" >&2
    return 1
  done
  if [[ -z "${selected[0]:-}" ]]; then
    echo "error: no golden projects selected for $generator ($requested); no compilation was performed" >&2
    return 1
  fi
  printf '%s\n' "${selected[@]}"
)

golden_contains() {
  local needle="$1" item
  shift
  for item in "$@"; do
    if [[ "$needle" == "$item" ]]; then return 0; fi
  done
  return 1
}
