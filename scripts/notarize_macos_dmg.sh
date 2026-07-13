#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
	echo "Usage: $0 <dmg-path>" >&2
	exit 2
fi

DMG_PATH="$1"
: "${APPLE_NOTARY_KEY_PATH:?APPLE_NOTARY_KEY_PATH is required}"
: "${APPLE_NOTARY_KEY_ID:?APPLE_NOTARY_KEY_ID is required}"
: "${APPLE_NOTARY_ISSUER_ID:?APPLE_NOTARY_ISSUER_ID is required}"
NOTARY_TIMEOUT="${NOTARY_TIMEOUT:-30m}"

if [[ ! -f "$DMG_PATH" ]]; then
	echo "DMG not found: $DMG_PATH" >&2
	exit 1
fi
if [[ ! -f "$APPLE_NOTARY_KEY_PATH" ]]; then
	echo "App Store Connect API key not found: $APPLE_NOTARY_KEY_PATH" >&2
	exit 1
fi

xcrun notarytool submit "$DMG_PATH" \
	--key "$APPLE_NOTARY_KEY_PATH" \
	--key-id "$APPLE_NOTARY_KEY_ID" \
	--issuer "$APPLE_NOTARY_ISSUER_ID" \
	--wait \
	--timeout "$NOTARY_TIMEOUT"
xcrun stapler staple "$DMG_PATH"
xcrun stapler validate "$DMG_PATH"
spctl --assess \
	--type open \
	--context context:primary-signature \
	--verbose=2 \
	"$DMG_PATH"

echo "Notarized and validated $DMG_PATH"
