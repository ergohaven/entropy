#!/usr/bin/env bash
# Cross-build the macOS app from Linux with cargo-zigbuild + an
# external macOS SDK, then bundle a universal .app, ad-hoc sign it with quill,
# and pack a .dmg (libdmg-hfsplus) — no Mac required.
#
# Needs: cargo-zigbuild, zig, the macOS SDK (scripts/prepare_macos_sdk.sh),
#        quill, genisoimage + the `dmg` tool from libdmg-hfsplus.
# Gatekeeper/notarization still require a real Mac and Apple credentials.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# quill и dmg ставит `task prepare:cross` в кэш проекта, а не в систему.
PATH="$PATH:$ROOT/.cache/tools"
export PATH

APP_NAME="${APP_NAME:-Entropy}"
BUNDLE_ID="${BUNDLE_ID:-com.ergohaven.entropy}"
DEPLOY_TARGET="${MACOSX_DEPLOYMENT_TARGET:-11.0}"
TARGET="${MACOS_TARGET:-universal2-apple-darwin}"
VERSION="${VERSION:-$(awk -F '"' '/^version = / { print $2; exit }' Cargo.toml)}"
# CFBundleVersion — только числовые компоненты, поэтому предрелизный суффикс
# остаётся в CFBundleShortVersionString и в имени .dmg.
NUMERIC_VERSION="${VERSION%%-*}"

DIST="$ROOT/dist/macos"
APP="$DIST/$APP_NAME.app"
DMG="$DIST/entropy-v$VERSION-macos-universal.dmg"

log() { printf '\033[1;36m==>\033[0m %s\n' "$*" >&2; }

# 1. macOS SDK (frameworks/libs for zig linking)
log "Preparing macOS SDK"
SDKROOT="$(scripts/prepare_macos_sdk.sh)"
export SDKROOT
export MACOSX_DEPLOYMENT_TARGET="$DEPLOY_TARGET"
log "SDKROOT=$SDKROOT  deployment target=$DEPLOY_TARGET"

# 2. Universal binary (cargo-zigbuild builds both arches and lipos them)
log "Building $TARGET"
cargo zigbuild --release --locked --target "$TARGET"
BIN="target/$TARGET/release/entropy"
[[ -f "$BIN" ]] || { echo "binary not found: $BIN" >&2; exit 1; }
file "$BIN" >&2 || true

# 3. .app bundle
log "Assembling $APP"
rm -rf "$APP" "$DMG"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
install -m 0755 "$BIN" "$APP/Contents/MacOS/entropy"

if [[ ! -f "$ROOT/assets/entropy.icns" ]]; then
	echo "assets/entropy.icns is missing — the bundle would ship with a blank Finder icon" >&2
	echo "generate it first: task icons" >&2
	exit 1
fi
cp "$ROOT/assets/entropy.icns" "$APP/Contents/Resources/entropy.icns"
ICON_PLIST=$'\n    <key>CFBundleIconFile</key>\n    <string>entropy</string>'

cat >"$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleDisplayName</key>
    <string>$APP_NAME</string>
    <key>CFBundleExecutable</key>
    <string>entropy</string>
    <key>CFBundleIdentifier</key>
    <string>$BUNDLE_ID</string>$ICON_PLIST
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>$APP_NAME</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>$VERSION</string>
    <key>CFBundleVersion</key>
    <string>$NUMERIC_VERSION</string>
    <key>LSApplicationCategoryType</key>
    <string>public.app-category.utilities</string>
    <key>LSMinimumSystemVersion</key>
    <string>$DEPLOY_TARGET</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSInputMonitoringUsageDescription</key>
    <string>Entropy needs Input Monitoring access to configure Bluetooth keyboards through HID.</string>
</dict>
</plist>
PLIST

# 4. Ad-hoc code signing with quill (Linux-native). Set QUILL_SIGN_P12 +
#    QUILL_SIGN_PASSWORD to sign with a real Developer ID instead.
if command -v quill >/dev/null 2>&1; then
	log "Signing with quill"
	quill sign "$APP/Contents/MacOS/entropy" >&2 || echo "quill sign failed (continuing unsigned)" >&2
else
	echo "quill not found; shipping an unsigned bundle" >&2
fi

# 5. .dmg from Linux (HFS hybrid ISO -> compressed UDIF)
# genisoimage (Debian/Fedora) и mkisofs (openSUSE/Arch) — одна и та же утилита
# schily-происхождения; нужен именно её флаг -apple, иначе HFS-гибрида не будет
# и macOS такой образ не смонтирует. xorrisofs здесь не подходит: -apple нет.
ISOGEN=""
for candidate in genisoimage mkisofs; do
	if command -v "$candidate" >/dev/null 2>&1; then
		ISOGEN="$candidate"
		break
	fi
done

if [[ -z "$ISOGEN" ]] || ! command -v dmg >/dev/null 2>&1; then
	echo "need genisoimage or mkisofs plus the 'dmg' tool from libdmg-hfsplus to build the .dmg" >&2
	echo "the ISO generator comes from 'task prepare', but libdmg-hfsplus has no distro package" >&2
	echo "and is built from source — the container already has it: task docker:macos-cross" >&2
	exit 1
fi

log "Packing $DMG"
stage="$(mktemp -d)"
cp -a "$APP" "$stage/"
ln -s /Applications "$stage/Applications"
raw="$(mktemp -u).dmg"
"$ISOGEN" -quiet -V "$APP_NAME" -D -R -apple -no-pad -o "$raw" "$stage"
dmg "$raw" "$DMG"
rm -rf "$raw" "$stage"
log "Built $DMG"

log "Done: $APP"
