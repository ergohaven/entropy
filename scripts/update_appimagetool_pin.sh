#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PIN_FILE="${APPIMAGETOOL_PIN_FILE:-$ROOT/scripts/appimagetool_pin.sh}"
RELEASE="${1:-}"
ASSET="appimagetool-x86_64.AppImage"

if [[ -z "$RELEASE" ]]; then
  echo "Usage: bash $0 <immutable-release-tag>" >&2
  exit 2
fi

if [[ "$RELEASE" == "continuous" || ! "$RELEASE" =~ ^[A-Za-z0-9][A-Za-z0-9._-]*$ ]]; then
  echo "Release tag must be immutable and contain only letters, digits, dots, underscores, or hyphens" >&2
  exit 2
fi

if [[ ! -f "$PIN_FILE" ]]; then
  echo "Cannot update missing pin file: $PIN_FILE" >&2
  exit 1
fi

URL="https://github.com/AppImage/AppImageKit/releases/download/$RELEASE/$ASSET"
DOWNLOAD="$(mktemp)"
PIN_TMP="$(mktemp "${PIN_FILE}.tmp.XXXXXX")"
trap 'rm -f "$DOWNLOAD" "$PIN_TMP"' EXIT

curl --fail --location --retry 3 --silent --show-error "$URL" --output "$DOWNLOAD"

if command -v sha256sum >/dev/null 2>&1; then
  SHA256="$(sha256sum "$DOWNLOAD" | awk '{print $1}')"
elif command -v shasum >/dev/null 2>&1; then
  SHA256="$(shasum -a 256 "$DOWNLOAD" | awk '{print $1}')"
else
  echo "No SHA-256 utility found (expected sha256sum or shasum)" >&2
  exit 1
fi

if ! awk \
  -v release="$RELEASE" \
  -v url="$URL" \
  -v sha256="$SHA256" \
  '
    /^readonly APPIMAGETOOL_PINNED_RELEASE=/ {
      print "readonly APPIMAGETOOL_PINNED_RELEASE=\"" release "\""
      found_release = 1
      next
    }
    /^readonly APPIMAGETOOL_PINNED_URL=/ {
      print "readonly APPIMAGETOOL_PINNED_URL=\"" url "\""
      found_url = 1
      next
    }
    /^readonly APPIMAGETOOL_PINNED_SHA256=/ {
      print "readonly APPIMAGETOOL_PINNED_SHA256=\"" sha256 "\""
      found_sha256 = 1
      next
    }
    { print }
    END { exit !(found_release && found_url && found_sha256) }
  ' "$PIN_FILE" > "$PIN_TMP"; then
  echo "Pin file does not contain all expected AppImageTool constants: $PIN_FILE" >&2
  exit 1
fi

mv "$PIN_TMP" "$PIN_FILE"
chmod 0644 "$PIN_FILE"
trap - EXIT
rm -f "$DOWNLOAD"

echo "Pinned AppImageTool release $RELEASE ($SHA256)"
