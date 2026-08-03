#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "Usage: $0 <file> <expected-sha256>" >&2
  exit 2
fi

FILE="$1"
EXPECTED_SHA256="$2"

if [[ ! -f "$FILE" ]]; then
  echo "Cannot verify missing file: $FILE" >&2
  exit 1
fi

if [[ ! "$EXPECTED_SHA256" =~ ^[0-9a-f]{64}$ ]]; then
  echo "Expected SHA-256 must be 64 lowercase hexadecimal characters" >&2
  exit 2
fi

if command -v sha256sum >/dev/null 2>&1; then
  CHECKSUM_OUTPUT="$(sha256sum "$FILE")"
elif command -v shasum >/dev/null 2>&1; then
  CHECKSUM_OUTPUT="$(shasum -a 256 "$FILE")"
else
  echo "No SHA-256 utility found (expected sha256sum or shasum)" >&2
  exit 1
fi

ACTUAL_SHA256="${CHECKSUM_OUTPUT%% *}"
if [[ "$ACTUAL_SHA256" != "$EXPECTED_SHA256" ]]; then
  echo "SHA-256 mismatch for $FILE" >&2
  echo "Expected: $EXPECTED_SHA256" >&2
  echo "Actual:   $ACTUAL_SHA256" >&2
  exit 1
fi
