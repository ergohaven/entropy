#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="${1:-${GITHUB_REF_NAME:-}}"
if [[ -z "$VERSION" ]]; then
  VERSION="v$(awk -F '"' '/^version = / { print $2; exit }' "$ROOT/Cargo.toml")"
fi

OUT="${2:-$ROOT/dist/release/entropy-${VERSION}-x86_64.AppImage}"
APPDIR="${APPDIR:-$ROOT/target/appimage/Entropy.AppDir}"
APPIMAGETOOL="${APPIMAGETOOL:-$ROOT/target/tools/appimagetool-x86_64.AppImage}"
APPIMAGETOOL_URL="${APPIMAGETOOL_URL:-https://github.com/AppImage/AppImageKit/releases/download/continuous/appimagetool-x86_64.AppImage}"

cd "$ROOT"
cargo build --release --locked

rm -rf "$APPDIR" "$OUT"
mkdir -p \
  "$APPDIR/usr/bin" \
  "$APPDIR/usr/lib" \
  "$APPDIR/usr/share/applications" \
  "$(dirname "$OUT")" \
  "$(dirname "$APPIMAGETOOL")"

install -m 0755 "$ROOT/target/release/entropy" "$APPDIR/usr/bin/entropy"

if [[ -e /usr/lib/x86_64-linux-gnu/libudev.so.1 ]]; then
  install -m 0755 /usr/lib/x86_64-linux-gnu/libudev.so.1 "$APPDIR/usr/lib/libudev.so.1"
elif [[ -e /lib/x86_64-linux-gnu/libudev.so.1 ]]; then
  install -m 0755 /lib/x86_64-linux-gnu/libudev.so.1 "$APPDIR/usr/lib/libudev.so.1"
fi

cat > "$APPDIR/AppRun" <<'EOF'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
export LD_LIBRARY_PATH="$HERE/usr/lib:${LD_LIBRARY_PATH:-}"
exec "$HERE/usr/bin/entropy" "$@"
EOF
chmod 0755 "$APPDIR/AppRun"

cat > "$APPDIR/entropy.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Entropy
Exec=entropy
Icon=entropy
Categories=Utility;
Terminal=false
X-AppImage-Version=${VERSION#v}
EOF
cp "$APPDIR/entropy.desktop" "$APPDIR/usr/share/applications/entropy.desktop"

cat > "$APPDIR/entropy.svg" <<'EOF'
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256">
  <rect width="256" height="256" rx="48" fill="#101828"/>
  <path d="M63 68h130v34H103v35h80v34h-80v-23H63V68z" fill="#5EEAD4"/>
  <path d="M63 154h130v34H63z" fill="#F97316"/>
</svg>
EOF

if [[ ! -x "$APPIMAGETOOL" ]]; then
  curl -fsSL "$APPIMAGETOOL_URL" -o "$APPIMAGETOOL"
  chmod 0755 "$APPIMAGETOOL"
fi

ARCH=x86_64 APPIMAGE_EXTRACT_AND_RUN=1 "$APPIMAGETOOL" "$APPDIR" "$OUT"
chmod 0755 "$OUT"
echo "Built $OUT"
