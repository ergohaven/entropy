#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_NAME="${APP_NAME:-Entropy}"
BUNDLE_ID="${BUNDLE_ID:-com.ergohaven.entropy}"
CODESIGN_IDENTITY="${CODESIGN_IDENTITY:--}"
CODESIGN_KEYCHAIN="${CODESIGN_KEYCHAIN:-}"
MACOS_SIGNING_CERTIFICATE_SHA1="${MACOS_SIGNING_CERTIFICATE_SHA1:-}"
REQUIRE_STABLE_SIGNING="${REQUIRE_STABLE_SIGNING:-0}"
SKIP_CARGO_BUILD="${SKIP_CARGO_BUILD:-0}"
MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-10.15}"
export MACOSX_DEPLOYMENT_TARGET

source "$ROOT/scripts/macos_stable_signing.sh"

VERSION="$(
	awk -F '"' '/^version = / { print $2; exit }' "$ROOT/Cargo.toml"
)"

target_arch_label() {
	case "$1" in
	aarch64-apple-darwin) echo "arm64" ;;
	x86_64-apple-darwin) echo "x86_64" ;;
	*) echo "${1%%-*}" ;;
	esac
}

current_arch_label() {
	case "$(uname -m)" in
	aarch64) echo "arm64" ;;
	*) uname -m ;;
	esac
}

running_under_rosetta() {
	[[ "$(sysctl -in sysctl.proc_translated 2>/dev/null || true)" == "1" ]]
}

target_root() {
	echo "${CARGO_TARGET_DIR:-$ROOT/target}"
}

validate_binary_arch() {
	if ! command -v lipo >/dev/null 2>&1; then
		echo "lipo not found; skipped binary architecture validation"
		return
	fi

	local archs
	archs="$(lipo -archs "$BIN")"
	if [[ " $archs " != *" $ARCH "* ]]; then
		echo "Expected $BIN to contain architecture '$ARCH', found: $archs" >&2
		exit 1
	fi
}

TARGET="${TARGET:-}"
if [[ -n "$TARGET" ]]; then
	BUILD_ARGS=(--release --target "$TARGET")
	BIN="$(target_root)/$TARGET/release/entropy"
	ARCH="$(target_arch_label "$TARGET")"
else
	if running_under_rosetta; then
		echo "Refusing implicit macOS build while the shell is running under Rosetta." >&2
		echo "Run from a native arm64 shell or set TARGET=aarch64-apple-darwin explicitly." >&2
		exit 1
	fi
	BUILD_ARGS=(--release)
	BIN="$(target_root)/release/entropy"
	ARCH="$(current_arch_label)"
fi

DIST_DIR="$ROOT/dist/macos"
APP_PATH="$DIST_DIR/$APP_NAME.app"
CONTENTS_DIR="$APP_PATH/Contents"
MACOS_DIR="$CONTENTS_DIR/MacOS"
RESOURCES_DIR="$CONTENTS_DIR/Resources"
ZIP_PATH="$DIST_DIR/entropy-v$VERSION-macos-$ARCH.app.zip"
DMG_PATH="$DIST_DIR/entropy-v$VERSION-macos-$ARCH.dmg"

sign_app_bundle() {
	if ! command -v plutil >/dev/null 2>&1; then
		echo "plutil not found; skipped Info.plist validation"
	else
		plutil -lint "$CONTENTS_DIR/Info.plist"
	fi

	if ! command -v codesign >/dev/null 2>&1; then
		if [[ "$REQUIRE_STABLE_SIGNING" == "1" ]]; then
			echo "codesign not found; cannot build a signed release" >&2
			return 1
		fi
		echo "codesign not found; skipped app bundle signing"
		return
	fi

	local codesign_args=(--force --sign "$CODESIGN_IDENTITY")
	if [[ -n "$CODESIGN_KEYCHAIN" ]]; then
		codesign_args+=(--keychain "$CODESIGN_KEYCHAIN")
	fi
	if [[ "$CODESIGN_IDENTITY" == "-" ]]; then
		codesign_args+=(--timestamp=none)
	else
		codesign_args+=(
			--identifier "$BUNDLE_ID"
			--requirements "$(macos_stable_designated_requirement \
				"$MACOS_SIGNING_CERTIFICATE_SHA1" \
				"$BUNDLE_ID")"
			--options runtime
			--timestamp=none
		)
	fi

	codesign "${codesign_args[@]}" "$APP_PATH"
	if [[ "$REQUIRE_STABLE_SIGNING" == "1" ]]; then
		macos_verify_stable_signature \
			"$APP_PATH" \
			"$MACOS_SIGNING_CERTIFICATE_SHA1" \
			"$BUNDLE_ID"
	else
		codesign --verify --strict "$APP_PATH"
	fi
}

create_dmg_with_retries() {
	local max_attempts=3
	local attempt=1
	local status=1

	while ((attempt <= max_attempts)); do
		rm -f "$DMG_PATH"
		if hdiutil create \
			-volname "$APP_NAME" \
			-srcfolder "$APP_PATH" \
			-ov \
			-format UDZO \
			"$DMG_PATH" >/dev/null; then
			echo "Built $DMG_PATH"
			return 0
		fi

		status=$?
		if ((attempt == max_attempts)); then
			echo "hdiutil create failed after $max_attempts attempts" >&2
			return "$status"
		fi

		echo "hdiutil create failed (attempt $attempt/$max_attempts); retrying" >&2
		sleep "$((attempt * 2))"
		attempt=$((attempt + 1))
	done

	return "$status"
}

macos_validate_stable_signing_configuration \
	"$CODESIGN_IDENTITY" \
	"$REQUIRE_STABLE_SIGNING" \
	"$MACOS_SIGNING_CERTIFICATE_SHA1" \
	"$BUNDLE_ID"

cd "$ROOT"
case "$SKIP_CARGO_BUILD" in
0)
	cargo build "${BUILD_ARGS[@]}"
	;;
1)
	if [[ ! -x "$BIN" ]]; then
		echo "SKIP_CARGO_BUILD=1 requires a prebuilt executable at $BIN" >&2
		exit 1
	fi
	;;
*)
	echo "SKIP_CARGO_BUILD must be 0 or 1" >&2
	exit 1
	;;
esac
validate_binary_arch

rm -rf "$APP_PATH" "$ZIP_PATH" "$DMG_PATH"
mkdir -p "$MACOS_DIR" "$RESOURCES_DIR"

cp "$BIN" "$MACOS_DIR/entropy"
chmod 755 "$MACOS_DIR/entropy"

ICON_PLIST=""
if [[ -f "$ROOT/assets/entropy.icns" ]]; then
	cp "$ROOT/assets/entropy.icns" "$RESOURCES_DIR/entropy.icns"
	ICON_PLIST='
    <key>CFBundleIconFile</key>
    <string>entropy</string>'
fi

cat >"$CONTENTS_DIR/Info.plist" <<PLIST
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
    <key>LSApplicationCategoryType</key>
    <string>public.app-category.utilities</string>
    <key>CFBundleShortVersionString</key>
    <string>$VERSION</string>
    <key>CFBundleVersion</key>
    <string>$VERSION</string>
    <key>LSMinimumSystemVersion</key>
    <string>$MACOSX_DEPLOYMENT_TARGET</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSInputMonitoringUsageDescription</key>
    <string>Entropy needs Input Monitoring access to configure Bluetooth keyboards through HID.</string>
</dict>
</plist>
PLIST

sign_app_bundle

if command -v ditto >/dev/null 2>&1; then
	ditto -c -k --sequesterRsrc --keepParent "$APP_PATH" "$ZIP_PATH"
else
	(cd "$DIST_DIR" && zip -qry "$(basename "$ZIP_PATH")" "$APP_NAME.app")
fi

if command -v hdiutil >/dev/null 2>&1; then
	create_dmg_with_retries
elif [[ "$REQUIRE_STABLE_SIGNING" == "1" ]]; then
	echo "hdiutil not found; cannot build a macOS release DMG" >&2
	exit 1
else
	echo "hdiutil not found; skipped DMG build"
fi

echo "Built $APP_PATH"
echo "Built $ZIP_PATH"
