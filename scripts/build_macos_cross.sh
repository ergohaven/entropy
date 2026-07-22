#!/usr/bin/env bash
# EXPERIMENTAL: cross-build the macOS app from Linux with cargo-zigbuild + an
# external macOS SDK, then bundle a universal .app, ad-hoc sign it with quill,
# and pack a .dmg (libdmg-hfsplus) — no Mac required.
#
# Needs: cargo-zigbuild, zig, the macOS SDK (scripts/prepare_macos_sdk.sh),
#        quill, genisoimage + the `dmg` tool from libdmg-hfsplus.
# Gatekeeper/notarization still require a real Mac and Apple credentials.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

APP_NAME="${APP_NAME:-Entropy}"
BUNDLE_ID="${BUNDLE_ID:-com.ergohaven.entropy}"
DEPLOY_TARGET="${MACOSX_DEPLOYMENT_TARGET:-11.0}"
TARGET="${MACOS_TARGET:-universal2-apple-darwin}"
VERSION="$(awk -F '"' '/^version = / { print $2; exit }' Cargo.toml)"

DIST="$ROOT/dist/macos"
APP="$DIST/$APP_NAME.app"
DMG="$DIST/entropy-v$VERSION-macos-universal.dmg"
ZIP="$DIST/entropy-v$VERSION-macos-universal.app.zip"

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
rm -rf "$APP" "$DMG" "$ZIP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
install -m 0755 "$BIN" "$APP/Contents/MacOS/entropy"

ICON_PLIST=""
if [[ -f "$ROOT/assets/entropy.icns" ]]; then
	cp "$ROOT/assets/entropy.icns" "$APP/Contents/Resources/entropy.icns"
	ICON_PLIST=$'\n    <key>CFBundleIconFile</key>\n    <string>entropy</string>'
fi

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
    <string>$VERSION</string>
    <key>LSApplicationCategoryType</key>
    <string>public.app-category.utilities</string>
    <key>LSMinimumSystemVersion</key>
    <string>$DEPLOY_TARGET</string>
    <key>NSHighResolutionCapable</key>
    <true/>
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

# 5a. Always produce a zip of the .app (guaranteed-portable artifact)
log "Packing $ZIP"
( cd "$DIST" && zip -qry "$(basename "$ZIP")" "$APP_NAME.app" )

# 5b. Best-effort .dmg from Linux (genisoimage HFS hybrid -> compressed UDIF)
if command -v genisoimage >/dev/null 2>&1 && command -v dmg >/dev/null 2>&1; then
	log "Packing $DMG"
	stage="$(mktemp -d)"
	cp -a "$APP" "$stage/"
	ln -s /Applications "$stage/Applications"
	raw="$(mktemp -u).dmg"
	genisoimage -quiet -V "$APP_NAME" -D -R -apple -no-pad -o "$raw" "$stage"
	dmg dmg "$raw" "$DMG"
	rm -rf "$raw" "$stage"
	log "Built $DMG"
else
	echo "genisoimage or libdmg-hfsplus 'dmg' not found; skipped .dmg (zip is available)" >&2
fi

log "Done: $APP"
