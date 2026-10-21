#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 3 ]]; then
  printf 'usage: %s APPIMAGE DEB RPM\n' "$0" >&2
  exit 2
fi

stage=inputs
checked_path=$1
work_dir=
failure_command=
failure_line=

diagnose_path() {
  local path=$1
  printf 'path=%q\n' "$path" >&2
  stat --printf='type=%F mode=%a (%A) size=%s name=%N\n' -- "$path" >&2 || true
  file -h -- "$path" >&2 || true
  if [[ -L "$path" ]]; then
    printf 'symlink target: ' >&2
    readlink -- "$path" >&2 || true
    printf 'resolved target: ' >&2
    realpath -e -- "$path" >&2 || true
    stat -L --printf='target type=%F mode=%a (%A) size=%s name=%N\n' -- "$path" >&2 || true
  fi
  stat --printf='parent type=%F mode=%a (%A) name=%N\n' -- "$(dirname -- "$path")" >&2 || true
}

finish() {
  local status=$1
  if (( status != 0 )); then
    printf 'Linux maintenance verification failed: stage=%s exit=%s line=%s command=%q\n' \
      "$stage" "$status" "$failure_line" "$failure_command" >&2
    diagnose_path "$checked_path"
    if [[ -n "$work_dir" && -f "$work_dir/rpm-payload.cpio" && "$stage" == rpm.* ]]; then
      printf 'rpm2cpio stdout capture:\n' >&2
      diagnose_path "$work_dir/rpm-payload.cpio"
      printf 'rpm2cpio stdout bytes (first 256, hexadecimal):\n' >&2
      od -An -tx1 -N256 -- "$work_dir/rpm-payload.cpio" >&2 || true
    fi
    if [[ -n "$work_dir" && -f "$work_dir/maintenance-info.stdout" ]]; then
      printf 'maintenance stdout bytes (first 256, hexadecimal):\n' >&2
      od -An -tx1 -N256 -- "$work_dir/maintenance-info.stdout" >&2 || true
    fi
  fi
  if [[ -n "$work_dir" ]]; then rm -rf -- "$work_dir"; fi
  exit "$status"
}
set -E
trap 'failure_command=$BASH_COMMAND failure_line=$LINENO' ERR
trap 'finish "$?"' EXIT

appimage=$(realpath "$1")
checked_path=$2
deb=$(realpath "$2")
checked_path=$3
rpm=$(realpath "$3")
work_dir=$(mktemp -d)

verify_maintenance() {
  checked_path=$1
  "$@" > "$work_dir/maintenance-info.stdout"
  printf '%s\n' 'Linea POS 0.2.1: mantenimiento local disponible' | cmp - "$work_dir/maintenance-info.stdout"
  rm -- "$work_dir/maintenance-info.stdout"
}

verify_binary() {
  local binary=$1
  checked_path=$binary
  [[ -f "$binary" && ! -L "$binary" && -x "$binary" ]]
  verify_maintenance "$binary" maintenance-info
}

stage=appimage.launcher
verify_maintenance "$appimage" --appimage-extract-and-run maintenance-info

stage=appimage.extract
(
  cd "$work_dir"
  "$appimage" --appimage-extract >/dev/null
)
stage=appimage.payload
verify_binary "$work_dir/squashfs-root/usr/bin/pos_tauri"

stage=deb.extract
checked_path=$deb
mkdir "$work_dir/deb"
dpkg-deb --extract "$deb" "$work_dir/deb"
stage=deb.payload
verify_binary "$work_dir/deb/usr/bin/pos_tauri"

stage=rpm.convert
checked_path=$rpm
rpm2cpio "$rpm" > "$work_dir/rpm-payload.cpio"
[[ -s "$work_dir/rpm-payload.cpio" ]]
stage=rpm.extract
mkdir "$work_dir/rpm"
(
  cd "$work_dir/rpm"
  cpio --extract --make-directories --quiet < "$work_dir/rpm-payload.cpio"
)
stage=rpm.payload
verify_binary "$work_dir/rpm/usr/bin/pos_tauri"
