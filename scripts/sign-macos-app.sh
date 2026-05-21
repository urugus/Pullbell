#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -lt 1 ] || [ "$#" -gt 2 ]; then
  echo "usage: $0 <app-path> [codesign-identity]" >&2
  exit 2
fi

app_path="$1"
identity="${2:-${CODESIGN_IDENTITY:-}}"
info_plist="$app_path/Contents/Info.plist"

if [ ! -d "$app_path" ]; then
  echo "app bundle does not exist: $app_path" >&2
  exit 1
fi

if [ ! -f "$info_plist" ]; then
  echo "app bundle is missing Info.plist: $info_plist" >&2
  exit 1
fi

if [ -z "$identity" ]; then
  echo "codesign identity is required. Use '-' for local ad-hoc signing." >&2
  exit 1
fi

executable_name="$(/usr/libexec/PlistBuddy -c "Print :CFBundleExecutable" "$info_plist" 2>/dev/null || true)"
if [ -z "$executable_name" ]; then
  echo "CFBundleExecutable is missing from $info_plist" >&2
  exit 1
fi

executable_path="$app_path/Contents/MacOS/$executable_name"
if [ ! -x "$executable_path" ]; then
  echo "bundle executable is not executable: $executable_path" >&2
  exit 1
fi

codesign_args=(--force --options runtime --sign "$identity")
if [ "$identity" != "-" ]; then
  codesign_args+=(--timestamp)
fi

codesign "${codesign_args[@]}" "$executable_path"
codesign "${codesign_args[@]}" "$app_path"
codesign --verify --deep --strict --verbose=2 "$app_path"
