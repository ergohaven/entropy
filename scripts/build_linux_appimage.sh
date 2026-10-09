#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="${1:-${GITHUB_REF_NAME:-}}"
if [[ -z "$VERSION" ]]; then
  VERSION="v$(awk -F '"' '/^version = / { print $2; exit }' "$ROOT/Cargo.toml")"
fi

OUT="${2:-$ROOT/dist/release/entropy-${VERSION}-x86_64.AppImage}"
# Кэш инструментов общий с nfpm, который кладёт туда же scripts/prepare_env.sh,
# и переживает `cargo clean`.
APPIMAGETOOL="${APPIMAGETOOL:-$ROOT/.cache/tools/appimagetool-x86_64.AppImage}"
# mksquashfs пишет в образ mtime каждого файла, а AppDir собирается `install`ом
# «сейчас», поэтому две сборки одного дерева иначе дают разные суммы.
SOURCE_DATE_EPOCH="${SOURCE_DATE_EPOCH:-$(git -C "$ROOT" log -1 --pretty=%ct 2>/dev/null || echo 315532800)}"
export SOURCE_DATE_EPOCH

cd "$ROOT"
# shellcheck disable=SC1091
source scripts/appimagetool_pin.sh
APPIMAGETOOL_URL="${APPIMAGETOOL_URL:-$APPIMAGETOOL_PINNED_URL}"
APPIMAGETOOL_SHA256="${APPIMAGETOOL_SHA256:-$APPIMAGETOOL_PINNED_SHA256}"

# OUT и путь загрузки appimagetool приезжают снаружи: путь канонизируется
# (симлинки, `..`) и обязан лежать строго внутри сборочных каталогов репозитория.
# Корни фиксированные — ни TMPDIR, ни DIST сюда не попадают: иначе вызывающий
# сам выбирает, куда сборке можно писать (`DIST=.` — весь репозиторий).
BUILD_ROOTS=("$ROOT/target" "$ROOT/dist" "$ROOT/.cache")

build_path() {
  local name="$1" path="$2" resolved root root_resolved
  resolved="$(realpath -m -- "$path")"
  for root in "${BUILD_ROOTS[@]}"; do
    root_resolved="$(realpath -m -- "$root")"
    # Только строго внутри: сам корень (`target`, `dist`) — не место для файла.
    if [[ "$resolved" == "$root_resolved"/* ]]; then
      printf '%s\n' "$resolved"
      return 0
    fi
  done
  echo "$name='$path' resolves to '$resolved', outside ${BUILD_ROOTS[*]}" >&2
  return 1
}

OUT="$(build_path OUT "$OUT")"

cargo build --release --locked

# По пути, пришедшему снаружи, ничего не удаляем: между проверкой и удалением
# идёт долгий cargo build, и каталог-предок успевают подменить симлинком.
# Сборка идёт в собственном каталоге, а в OUT готовый образ попадает rename'ом.
mkdir -p "$ROOT/target/appimage"
WORK="$(mktemp -d "$ROOT/target/appimage/build.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT
APPDIR="$WORK/Entropy.AppDir"
mkdir -p \
  "$APPDIR/usr/bin" \
  "$APPDIR/usr/share/applications" \
  "$APPDIR/usr/share/metainfo" \
  "$APPDIR/usr/share/icons"

install -m 0755 "$ROOT/target/release/entropy" "$APPDIR/usr/bin/entropy"

# Ничего не бандлим: единственные ELF-зависимости бинарника — libc/libm/libgcc,
# весь GUI-стек (libGL, xkbcommon, X11/xcb, wayland) грузится через dlopen, а
# hidapi собран с чисто растовым hidraw-бэкендом и libudev не требует. Подсовывать
# хостовые библиотеки через LD_LIBRARY_PATH при этом опаснее, чем полезно: они
# перебивали бы системные у всего, что подгрузится позже.
cat > "$APPDIR/AppRun" <<'EOF'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
exec "$HERE/usr/bin/entropy" "$@"
EOF
chmod 0755 "$APPDIR/AppRun"

# Те же .desktop, metainfo и иконки, что уходят в deb/rpm/arch: описание
# приложения должно быть одним во всех форматах.
install -m 0644 "$ROOT/packaging/linux/entropy.desktop" "$APPDIR/usr/share/applications/entropy.desktop"
install -m 0644 "$ROOT/packaging/linux/com.ergohaven.entropy.metainfo.xml" \
  "$APPDIR/usr/share/metainfo/com.ergohaven.entropy.metainfo.xml"
cp -r "$ROOT/assets/icons/hicolor" "$APPDIR/usr/share/icons/hicolor"

# appimagetool ищет .desktop и иконку с именем из Icon= в корне AppDir.
install -m 0644 "$ROOT/packaging/linux/entropy.desktop" "$APPDIR/entropy.desktop"
printf 'X-AppImage-Version=%s\n' "${VERSION#v}" >> "$APPDIR/entropy.desktop"
install -m 0644 "$ROOT/assets/icons/hicolor/256x256/apps/entropy.png" "$APPDIR/entropy.png"
# .DirIcon создаём сами: appimagetool делает это уже после нормализации времён,
# и его симлинк со временем «сейчас» ломает воспроизводимость образа.
ln -s entropy.png "$APPDIR/.DirIcon"

find "$APPDIR" -exec touch -h -d "@$SOURCE_DATE_EPOCH" {} +

if [[ ! -x "$APPIMAGETOOL" ]]; then
  # Проверяется только путь загрузки: готовый инструмент из образа лежит в
  # /usr/local/bin и не переписывается, а сюда мы пишем.
  APPIMAGETOOL="$(build_path APPIMAGETOOL "$APPIMAGETOOL")"
  APPIMAGETOOL_DOWNLOAD="$WORK/appimagetool.download"
  curl -fsSL "$APPIMAGETOOL_URL" -o "$APPIMAGETOOL_DOWNLOAD"
  "$ROOT/scripts/verify_sha256.sh" "$APPIMAGETOOL_DOWNLOAD" "$APPIMAGETOOL_SHA256"
  chmod 0755 "$APPIMAGETOOL_DOWNLOAD"
  mkdir -p "$(dirname "$APPIMAGETOOL")"
  mv "$APPIMAGETOOL_DOWNLOAD" "$APPIMAGETOOL"
else
  "$ROOT/scripts/verify_sha256.sh" "$APPIMAGETOOL" "$APPIMAGETOOL_SHA256"
fi

# mksquashfs 4.3 внутри закреплённого appimagetool пакует хвосты файлов во
# фрагменты в порядке гонки своих потоков, и один и тот же AppDir даёт разные
# образы. Флаги в mksquashfs appimagetool не пробрасывает и берёт его по
# фиксированному пути рядом с собой, поэтому запускаем распакованную копию
# (сумма выше проверена у самого AppImage), где mksquashfs обёрнут в
# `-processors 1`.
TOOL_DIR="$WORK/tool"
mkdir "$TOOL_DIR"
(cd "$TOOL_DIR" && "$APPIMAGETOOL" --appimage-extract >/dev/null)
MKSQUASHFS="$TOOL_DIR/squashfs-root/usr/lib/appimagekit/mksquashfs"
mv "$MKSQUASHFS" "$MKSQUASHFS.real"
cat > "$MKSQUASHFS" <<'EOF'
#!/bin/sh
exec "$0.real" "$@" -processors 1
EOF
chmod 0755 "$MKSQUASHFS"

IMAGE="$WORK/entropy.AppImage"
ARCH=x86_64 "$TOOL_DIR/squashfs-root/AppRun" --comp xz "$APPDIR" "$IMAGE"
chmod 0755 "$IMAGE"
mkdir -p "$(dirname "$OUT")"
mv -fT "$IMAGE" "$OUT"
echo "Built $OUT"
