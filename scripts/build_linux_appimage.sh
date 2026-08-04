#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="${1:-${GITHUB_REF_NAME:-}}"
if [[ -z "$VERSION" ]]; then
  VERSION="v$(awk -F '"' '/^version = / { print $2; exit }' "$ROOT/Cargo.toml")"
fi

OUT="${2:-$ROOT/dist/release/entropy-${VERSION}-x86_64.AppImage}"
APPDIR="${APPDIR:-$ROOT/target/appimage/Entropy.AppDir}"
APPIMAGETOOL="${APPIMAGETOOL:-$ROOT/.cache/tools/appimagetool-x86_64.AppImage}"
APPIMAGETOOL_VERSION="${APPIMAGETOOL_VERSION:-1.9.1}"
APPIMAGETOOL_URL="${APPIMAGETOOL_URL:-https://github.com/AppImage/appimagetool/releases/download/${APPIMAGETOOL_VERSION}/appimagetool-x86_64.AppImage}"

cd "$ROOT"
cargo build --release --locked

rm -rf "$APPDIR" "$OUT"
mkdir -p \
  "$APPDIR/usr/bin" \
  "$APPDIR/usr/share/applications" \
  "$APPDIR/usr/share/icons/hicolor/scalable/apps" \
  "$(dirname "$OUT")" \
  "$(dirname "$APPIMAGETOOL")"

install -m 0755 "$ROOT/target/release/entropy" "$APPDIR/usr/bin/entropy"

# The binary link-loads its GUI stack (libGL, xkbcommon, X11) via dlopen and uses
# the pure-Rust hidraw backend, so nothing has to be bundled — host libraries are
# resolved at runtime, which keeps the AppImage small and portable.
cat > "$APPDIR/AppRun" <<'EOF'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
exec "$HERE/usr/bin/entropy" "$@"
EOF
chmod 0755 "$APPDIR/AppRun"

install -m 0644 "$ROOT/packaging/linux/entropy.desktop" "$APPDIR/entropy.desktop"
install -m 0644 "$ROOT/packaging/linux/entropy.desktop" "$APPDIR/usr/share/applications/entropy.desktop"

install -m 0644 "$ROOT/assets/entropy.svg" "$APPDIR/entropy.svg"
install -m 0644 "$ROOT/assets/entropy.svg" "$APPDIR/usr/share/icons/hicolor/scalable/apps/entropy.svg"
if [[ -d "$ROOT/assets/icons/hicolor" ]]; then
  cp -r "$ROOT/assets/icons/hicolor/." "$APPDIR/usr/share/icons/hicolor/"
fi

if [[ ! -x "$APPIMAGETOOL" ]]; then
  curl -fsSL "$APPIMAGETOOL_URL" -o "$APPIMAGETOOL"
  chmod 0755 "$APPIMAGETOOL"
fi

ARCH=x86_64 APPIMAGE_EXTRACT_AND_RUN=1 "$APPIMAGETOOL" "$APPDIR" "$OUT"
chmod 0755 "$OUT"
echo "Built $OUT"
