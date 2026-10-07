#!/usr/bin/env bash

set -euo pipefail

die() {
  printf 'sanitize-appimage: %s\n' "$*" >&2
  exit 1
}

if [[ $# -ne 1 ]]; then
  die "expected exactly one AppImage argument"
fi

appimage=$1
[[ -f "$appimage" && ! -L "$appimage" ]] || die "input must be a regular file, not a symlink: $appimage"
[[ -x "$appimage" ]] || die "input AppImage must be executable: $appimage"

appimage_dir=$(cd -- "$(dirname -- "$appimage")" && pwd -P)
appimage_name=$(basename -- "$appimage")
appimage="$appimage_dir/$appimage_name"
original_mode=$(stat -c '%a' -- "$appimage")

work_dir=
output_dir=
cleanup() {
  [[ -z "$work_dir" ]] || rm -rf -- "$work_dir"
  [[ -z "$output_dir" ]] || rm -rf -- "$output_dir"
}
work_dir=$(mktemp -d "${TMPDIR:-/tmp}/sanitize-appimage.XXXXXXXX")
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

plugin=${SANITIZE_APPIMAGE_PLUGIN:-}
if [[ -z "$plugin" ]]; then
  search_roots=()
  if [[ -n "${SANITIZE_APPIMAGE_PLUGIN_ROOT:-}" ]]; then
    search_roots+=("$SANITIZE_APPIMAGE_PLUGIN_ROOT")
  else
    search_roots+=("$PWD/src-tauri/target")
    if [[ -n "${XDG_CACHE_HOME:-}" ]]; then
      search_roots+=("$XDG_CACHE_HOME/tauri")
    elif [[ -n "${HOME:-}" ]]; then
      search_roots+=("$HOME/.cache/tauri")
    fi
  fi

  existing_roots=()
  for root in "${search_roots[@]}"; do
    [[ -d "$root" ]] && existing_roots+=("$root")
  done

  plugins=()
  if (( ${#existing_roots[@]} > 0 )); then
    plugin_manifest="$work_dir/plugins.nul"
    if ! find "${existing_roots[@]}" -type f -name 'linuxdeploy-plugin-appimage*.AppImage' -print0 >"$plugin_manifest"; then
      die "plugin discovery failed"
    fi
    while IFS= read -r -d '' candidate; do
      plugins+=("$candidate")
    done <"$plugin_manifest"
  fi
  (( ${#plugins[@]} == 1 )) || die "expected exactly one linuxdeploy-plugin-appimage.AppImage, found ${#plugins[@]}"
  plugin=${plugins[0]}
fi

[[ -f "$plugin" && ! -L "$plugin" ]] || die "plugin must be a regular file, not a symlink: $plugin"
[[ -x "$plugin" ]] || die "plugin must be executable: $plugin"

output_dir=$(mktemp -d "$appimage_dir/.${appimage_name}.sanitize.XXXXXXXX")
chmod 700 -- "$output_dir"
staged_output="$output_dir/repacked.AppImage"

extract_appimage() {
  local image=$1 destination=$2
  mkdir -- "$destination"
  (
    cd -- "$destination"
    "$image" --appimage-extract >/dev/null
  )
  [[ -d "$destination/squashfs-root" ]] || die "AppImage extraction did not create squashfs-root: $image"
}

write_forbidden_manifest() {
  local appdir=$1 manifest=$2
  find "$appdir" \( -type f -o -type l \) \
    \( -name 'libwayland-client.so*' -o -name 'libwayland-cursor.so*' \
       -o -name 'libwayland-egl.so*' -o -name 'libwayland-server.so*' \) -print0 >"$manifest"
}

extract_appimage "$appimage" "$work_dir/original"
appdir="$work_dir/original/squashfs-root"

client_manifest="$work_dir/client-libraries.nul"
if ! find "$appdir" \( -type f -o -type l \) -name 'libwayland-client.so*' -print0 >"$client_manifest"; then
  die "failed to inspect original AppImage for libwayland-client"
fi
client_count=0
while IFS= read -r -d '' library; do
  ((client_count += 1))
done <"$client_manifest"
(( client_count > 0 )) || die "original AppImage contains no libwayland-client.so*"

forbidden_manifest="$work_dir/forbidden-libraries.nul"
if ! write_forbidden_manifest "$appdir" "$forbidden_manifest"; then
  die "failed to inspect original AppImage for forbidden libraries"
fi
while IFS= read -r -d '' library; do
  rm -f -- "$library"
done <"$forbidden_manifest"

if ! write_forbidden_manifest "$appdir" "$forbidden_manifest"; then
  die "failed to verify removal of forbidden libraries"
fi
if IFS= read -r -d '' remaining <"$forbidden_manifest"; then
  die "failed to remove forbidden library: $remaining"
fi

env -u LDAI_SIGN -u LDAI_SIGN_KEY -u SIGN \
  ARCH=x86_64 \
  LDAI_OUTPUT="$staged_output" \
  "$plugin" --appimage-extract-and-run --appdir "$appdir"

[[ -f "$staged_output" && ! -L "$staged_output" ]] || die "plugin did not create a regular AppImage at the explicit output path"
chmod "$original_mode" -- "$staged_output"
[[ -x "$staged_output" ]] || die "rebuilt AppImage is not executable"

extract_appimage "$staged_output" "$work_dir/rebuilt"
if ! write_forbidden_manifest "$work_dir/rebuilt/squashfs-root" "$forbidden_manifest"; then
  die "failed to inspect rebuilt AppImage for forbidden libraries"
fi
if IFS= read -r -d '' remaining <"$forbidden_manifest"; then
  die "rebuilt AppImage still contains forbidden library: $remaining"
fi

mv -fT -- "$staged_output" "$appimage"
printf 'Sanitized AppImage: %s\n' "$appimage"
