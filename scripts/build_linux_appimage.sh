#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="${1:-${GITHUB_REF_NAME:-}}"
if [[ -z "$VERSION" ]]; then
  VERSION="v$(awk -F '"' '/^version = / { print $2; exit }' "$ROOT/Cargo.toml")"
fi

OUT="${2:-$ROOT/dist/release/entropy-${VERSION}-x86_64.AppImage}"
APPDIR="${APPDIR:-$ROOT/target/appimage/Entropy.AppDir}"
# Кэш инструментов общий с nfpm, который кладёт туда же scripts/prepare_env.sh,
# и переживает `cargo clean`.
APPIMAGETOOL="${APPIMAGETOOL:-$ROOT/.cache/tools/appimagetool-x86_64.AppImage}"

cd "$ROOT"
# shellcheck disable=SC1091
source scripts/appimagetool_pin.sh
APPIMAGETOOL_URL="${APPIMAGETOOL_URL:-$APPIMAGETOOL_PINNED_URL}"
APPIMAGETOOL_SHA256="${APPIMAGETOOL_SHA256:-$APPIMAGETOOL_PINNED_SHA256}"

cargo build --release --locked

rm -rf "$APPDIR" "$OUT"
mkdir -p \
  "$APPDIR/usr/bin" \
  "$APPDIR/usr/share/applications" \
  "$APPDIR/usr/share/metainfo" \
  "$APPDIR/usr/share/icons" \
  "$(dirname "$OUT")" \
  "$(dirname "$APPIMAGETOOL")"

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

if [[ ! -x "$APPIMAGETOOL" ]]; then
  APPIMAGETOOL_DOWNLOAD="$(mktemp "${APPIMAGETOOL}.download.XXXXXX")"
  trap 'rm -f "$APPIMAGETOOL_DOWNLOAD"' EXIT
  curl -fsSL "$APPIMAGETOOL_URL" -o "$APPIMAGETOOL_DOWNLOAD"
  "$ROOT/scripts/verify_sha256.sh" "$APPIMAGETOOL_DOWNLOAD" "$APPIMAGETOOL_SHA256"
  chmod 0755 "$APPIMAGETOOL_DOWNLOAD"
  mv "$APPIMAGETOOL_DOWNLOAD" "$APPIMAGETOOL"
  trap - EXIT
else
  "$ROOT/scripts/verify_sha256.sh" "$APPIMAGETOOL" "$APPIMAGETOOL_SHA256"
fi

ARCH=x86_64 APPIMAGE_EXTRACT_AND_RUN=1 "$APPIMAGETOOL" "$APPDIR" "$OUT"
chmod 0755 "$OUT"
echo "Built $OUT"
