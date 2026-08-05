#!/usr/bin/env bash
# Download and unpack a pinned macOS SDK from joseluisq/macosx-sdks into a local
# cache (git-ignored). Idempotent: re-running is a no-op once the SDK is present.
# Prints the SDK directory path to stdout; all logs go to stderr so the caller can
# capture it as SDKROOT:  SDKROOT="$(scripts/prepare_macos_sdk.sh)"
#
# NOTE: this is only for the Linux->macOS cross-build. Apple's SDK is licensed for
# use on Apple-branded hardware; using it to cross-compile from Linux is a legal
# grey area, which is why releases are built on Mac runners and the cross path
# stays opt-in for local use.
#
# The download is always checksummed: the pinned version's digest lives in
# scripts/tool_pins.sh, and any URL or version override must bring its own
# MACOS_SDK_SHA256 (checksums are on the SDK release page).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck disable=SC1091
source "$ROOT/scripts/tool_pins.sh"

SDK_VERSION="${MACOS_SDK_VERSION:-15.5}"
CACHE_DIR="${MACOS_SDK_CACHE:-$ROOT/.cache/macos-sdk}"
SDK_DIR="$CACHE_DIR/MacOSX${SDK_VERSION}.sdk"
ARCHIVE="MacOSX${SDK_VERSION}.sdk.tar.xz"
URL="${MACOS_SDK_URL:-https://github.com/joseluisq/macosx-sdks/releases/download/${SDK_VERSION}/${ARCHIVE}}"

log() { printf '%s\n' "$*" >&2; }

if [[ -d "$SDK_DIR" && -d "$SDK_DIR/System/Library/Frameworks" ]]; then
	log "macOS SDK already present: $SDK_DIR"
	printf '%s\n' "$SDK_DIR"
	exit 0
fi

command -v curl >/dev/null 2>&1 || { log "curl is required"; exit 1; }
command -v tar >/dev/null 2>&1 || { log "tar is required"; exit 1; }

# Своё зеркало — своя ответственность: пиннутая сумма относится к пиннутому
# URL, поэтому переопределение без явного дайджеста отклоняется, а не
# «проверяется» несуществующей суммой.
if [[ -n "${MACOS_SDK_URL:-}" && -z "${MACOS_SDK_SHA256:-}" ]]; then
	log "MACOS_SDK_URL overrides the pinned mirror — set MACOS_SDK_SHA256 as well"
	exit 1
fi

if [[ -n "${MACOS_SDK_SHA256:-}" ]]; then
	EXPECTED_SHA256="$MACOS_SDK_SHA256"
elif ! EXPECTED_SHA256="$(tool_sha256 "macos-sdk:${SDK_VERSION}" 2>/dev/null)"; then
	log "no pinned checksum for macOS SDK $SDK_VERSION"
	log "add it to scripts/tool_pins.sh or pass MACOS_SDK_SHA256=<sha256>"
	exit 1
fi

mkdir -p "$CACHE_DIR"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

log "Downloading macOS SDK $SDK_VERSION from $URL"
curl -fsSL --retry 3 --retry-all-errors --connect-timeout 15 --max-time 1800 "$URL" -o "$tmp/$ARCHIVE"

log "Verifying SHA-256"
"$ROOT/scripts/verify_sha256.sh" "$tmp/$ARCHIVE" "$EXPECTED_SHA256"

log "Unpacking into $CACHE_DIR"
tar -xJf "$tmp/$ARCHIVE" -C "$CACHE_DIR"

if [[ ! -d "$SDK_DIR/System/Library/Frameworks" ]]; then
	log "Unexpected SDK layout: $SDK_DIR/System/Library/Frameworks not found"
	exit 1
fi

log "macOS SDK ready: $SDK_DIR"
printf '%s\n' "$SDK_DIR"
