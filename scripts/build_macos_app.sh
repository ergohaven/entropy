#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_NAME="${APP_NAME:-Entropy}"
BUNDLE_ID="${BUNDLE_ID:-com.ergohaven.entropy}"
CODESIGN_IDENTITY="${CODESIGN_IDENTITY:--}"
MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-10.15}"
export MACOSX_DEPLOYMENT_TARGET

# Версию можно задать снаружи: релиз передаёт сюда имя тега, чтобы предрелизный
# v1.2.3-rc.1 не выложил .dmg с именем будущего стабильного v1.2.3.
VERSION="${VERSION:-$(
	awk -F '"' '/^version = / { print $2; exit }' "$ROOT/Cargo.toml"
)}"
# CFBundleVersion — только числовые компоненты, поэтому предрелизный суффикс
# остаётся в CFBundleShortVersionString и в имени .dmg.
NUMERIC_VERSION="${VERSION%%-*}"

target_arch_label() {
	case "$1" in
	aarch64-apple-darwin) echo "arm64" ;;
	x86_64-apple-darwin) echo "x86_64" ;;
	*) echo "${1%%-*}" ;;
	esac
}

target_root() {
	echo "${CARGO_TARGET_DIR:-$ROOT/target}"
}

# Обе арки в одном бандле: так пользователю не нужно выбирать сборку под свой
# Mac, а Rosetta перестаёт влиять на результат — арка выбирается явно, а не по
# тому, под какой архитектурой запущен шелл. TARGET (одна арка) поддержан ради
# вызывающих, которые ещё передают его: PR-гейт собирает по одной арке на раннер.
TARGETS="${TARGETS:-${TARGET:-aarch64-apple-darwin x86_64-apple-darwin}}"
read -r -a BUILD_TARGETS <<<"$TARGETS"

if ((${#BUILD_TARGETS[@]} > 1)); then
	ARCH="universal"
	BIN="$(target_root)/universal-apple-darwin/release/entropy"
else
	ARCH="$(target_arch_label "${BUILD_TARGETS[0]}")"
	BIN="$(target_root)/${BUILD_TARGETS[0]}/release/entropy"
fi

build_binary() {
	local target
	for target in "${BUILD_TARGETS[@]}"; do
		echo "==> Building $target"
		rustup target add "$target" >/dev/null 2>&1 || true
		cargo build --release --locked --target "$target"
	done

	((${#BUILD_TARGETS[@]} > 1)) || return 0

	local inputs=()
	for target in "${BUILD_TARGETS[@]}"; do
		inputs+=("$(target_root)/$target/release/entropy")
	done
	mkdir -p "$(dirname "$BIN")"
	echo "==> Merging ${#inputs[@]} slices into a universal binary"
	lipo -create -output "$BIN" "${inputs[@]}"
}

validate_binary_arch() {
	if ! command -v lipo >/dev/null 2>&1; then
		echo "lipo not found; skipped binary architecture validation"
		return
	fi

	local archs target expected
	archs="$(lipo -archs "$BIN")"
	for target in "${BUILD_TARGETS[@]}"; do
		expected="$(target_arch_label "$target")"
		if [[ " $archs " != *" $expected "* ]]; then
			echo "Expected $BIN to contain architecture '$expected', found: $archs" >&2
			exit 1
		fi
	done
}

DIST_DIR="$ROOT/dist/macos"
APP_PATH="$DIST_DIR/$APP_NAME.app"
CONTENTS_DIR="$APP_PATH/Contents"
MACOS_DIR="$CONTENTS_DIR/MacOS"
RESOURCES_DIR="$CONTENTS_DIR/Resources"
DMG_PATH="$DIST_DIR/entropy-v$VERSION-macos-$ARCH.dmg"

sign_app_bundle() {
	if ! command -v plutil >/dev/null 2>&1; then
		echo "plutil not found; skipped Info.plist validation"
	else
		plutil -lint "$CONTENTS_DIR/Info.plist"
	fi

	if ! command -v codesign >/dev/null 2>&1; then
		echo "codesign not found; skipped app bundle signing"
		return
	fi

	local codesign_args=(--force --sign "$CODESIGN_IDENTITY")
	if [[ "$CODESIGN_IDENTITY" == "-" ]]; then
		codesign_args+=(--timestamp=none)
	fi

	codesign "${codesign_args[@]}" "$APP_PATH"
	codesign --verify --strict "$APP_PATH"
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

cd "$ROOT"
build_binary
validate_binary_arch

rm -rf "$APP_PATH" "$DMG_PATH"
mkdir -p "$MACOS_DIR" "$RESOURCES_DIR"

cp "$BIN" "$MACOS_DIR/entropy"
chmod 755 "$MACOS_DIR/entropy"

if [[ ! -f "$ROOT/assets/entropy.icns" ]]; then
	echo "assets/entropy.icns is missing — the bundle would ship with a blank Finder icon" >&2
	echo "generate it first: task icons" >&2
	exit 1
fi
cp "$ROOT/assets/entropy.icns" "$RESOURCES_DIR/entropy.icns"
ICON_PLIST='
    <key>CFBundleIconFile</key>
    <string>entropy</string>'

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
    <string>$NUMERIC_VERSION</string>
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

if command -v hdiutil >/dev/null 2>&1; then
	create_dmg_with_retries
else
	echo "hdiutil not found; skipped DMG build"
fi

echo "Built $APP_PATH"
