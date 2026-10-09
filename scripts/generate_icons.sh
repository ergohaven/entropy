#!/usr/bin/env bash
# Раскладывает assets/entropy.ico в набор иконок hicolor для Linux-пакетов.
# Результат коммитится рядом с самим .ico, поэтому ImageMagick нужен только тому,
# кто меняет логотип, а не каждому, кто собирает пакет.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

SRC="${SRC:-assets/entropy.ico}"
OUT="${OUT:-assets/icons/hicolor}"
SIZES=(16 32 48 64 128 256)

# ImageMagick 7 ставит `magick`, ImageMagick 6 (Debian bookworm) — только `convert`.
im() {
  if command -v magick >/dev/null 2>&1; then
    magick "$@"
  elif command -v convert >/dev/null 2>&1; then
    convert "$@"
  else
    echo "need ImageMagick (imagemagick on Debian/Ubuntu and Arch, ImageMagick on openSUSE/Fedora)" >&2
    exit 1
  fi
}

im_identify() {
  if command -v magick >/dev/null 2>&1; then
    magick identify "$@"
  else
    identify "$@"
  fi
}

# Мелкие размеры в .ico нарисованы отдельно, а не уменьшены из 256: берём их как
# есть и досчитываем ресайзом только те, которых в файле нет.
declare -A frame_of=()
while read -r scene width; do
  frame_of["$width"]="$scene"
done < <(im_identify -format '%s %w\n' "$SRC")

largest=0
for width in "${!frame_of[@]}"; do
  ((width > largest)) && largest="$width"
done
((largest > 0)) || {
  echo "$SRC has no readable frames" >&2
  exit 1
}

for size in "${SIZES[@]}"; do
  dst="$OUT/${size}x${size}/apps/entropy.png"
  mkdir -p "$(dirname "$dst")"
  # -strip убирает дату создания: иначе каждая перегенерация даёт новый diff.
  if [[ -n "${frame_of[$size]:-}" ]]; then
    im "${SRC}[${frame_of[$size]}]" -strip "$dst"
  else
    im "${SRC}[${frame_of[$largest]}]" -filter Lanczos -resize "${size}x${size}" -strip "$dst"
  fi
  echo "Wrote $dst"
done
