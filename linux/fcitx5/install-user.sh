#!/usr/bin/env sh
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
build_dir="${XDG_CACHE_HOME:-$HOME/.cache}/entropy/fcitx5-build"
prefix="${1:-$HOME/.local}"

cmake -S "$script_dir" -B "$build_dir" -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$prefix"
cmake --build "$build_dir"
cmake --install "$build_dir"

data_home="${XDG_DATA_HOME:-$HOME/.local/share}"
mkdir -p "$data_home/entropy/fcitx5"
printf '%s\n' 2 >"$data_home/entropy/fcitx5/host-text-protocol-version"

printf '%s\n' "Installed Entropy Fcitx5 backend under $prefix"
if command -v fcitx5 >/dev/null 2>&1; then
    fcitx5 -r >/dev/null 2>&1 || true
    printf '%s\n' "Restarted Fcitx5 if it was running."
fi
