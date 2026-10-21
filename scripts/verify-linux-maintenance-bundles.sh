#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 3 ]]; then
  printf 'usage: %s APPIMAGE DEB RPM\n' "$0" >&2
  exit 2
fi

appimage=$(realpath "$1")
deb=$(realpath "$2")
rpm=$(realpath "$3")
work_dir=$(mktemp -d)
trap 'rm -rf -- "$work_dir"' EXIT

verify_maintenance() {
  "$@" > "$work_dir/maintenance-info.stdout"
  printf '%s\n' 'Linea POS 0.2.1: mantenimiento local disponible' | cmp - "$work_dir/maintenance-info.stdout"
}

verify_binary() {
  local binary=$1
  [[ -f "$binary" && ! -L "$binary" && -x "$binary" ]]
  verify_maintenance "$binary" maintenance-info
}

verify_maintenance "$appimage" --appimage-extract-and-run maintenance-info

(
  cd "$work_dir"
  "$appimage" --appimage-extract >/dev/null
)
verify_binary "$work_dir/squashfs-root/usr/bin/pos_tauri"

mkdir "$work_dir/deb"
dpkg-deb --extract "$deb" "$work_dir/deb"
verify_binary "$work_dir/deb/usr/bin/pos_tauri"

mkdir "$work_dir/rpm"
(
  cd "$work_dir/rpm"
  rpm2cpio "$rpm" | cpio --extract --quiet
)
verify_binary "$work_dir/rpm/usr/bin/pos_tauri"
