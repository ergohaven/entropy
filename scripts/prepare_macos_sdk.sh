#!/usr/bin/env bash
# Download and unpack a pinned macOS SDK from joseluisq/macosx-sdks into a local
# cache (git-ignored). Idempotent: re-running is a no-op once the SDK is present.
# Prints the SDK directory path to stdout; all logs go to stderr so the caller can
# capture it as SDKROOT:  SDKROOT="$(scripts/prepare_macos_sdk.sh)"
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

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

mkdir -p "$CACHE_DIR"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

log "Downloading macOS SDK $SDK_VERSION from $URL"
curl -fsSL "$URL" -o "$tmp/$ARCHIVE"

if [[ -n "${MACOS_SDK_SHA256:-}" ]]; then
	log "Verifying SHA-256"
	echo "${MACOS_SDK_SHA256}  $tmp/$ARCHIVE" | sha256sum -c - >&2
fi

log "Unpacking into $CACHE_DIR"
tar -xJf "$tmp/$ARCHIVE" -C "$CACHE_DIR"

if [[ ! -d "$SDK_DIR/System/Library/Frameworks" ]]; then
	log "Unexpected SDK layout: $SDK_DIR/System/Library/Frameworks not found"
	exit 1
fi

log "macOS SDK ready: $SDK_DIR"
printf '%s\n' "$SDK_DIR"
