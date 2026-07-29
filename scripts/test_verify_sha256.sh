#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERIFY="$ROOT/scripts/verify_sha256.sh"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

FIXTURE="$TMP_DIR/trusted-tool"
STDERR_FILE="$TMP_DIR/stderr"
printf 'trusted\n' > "$FIXTURE"

"$VERIFY" \
  "$FIXTURE" \
  "7bd39a7cbcf687fd60f819645b8bcaf731a9f19cb102484a7b84530516d7e8b8"

if "$VERIFY" "$FIXTURE" "$(printf '0%.0s' {1..64})" 2> "$STDERR_FILE"; then
  echo "Expected a checksum mismatch to fail" >&2
  exit 1
fi
if [[ "$(<"$STDERR_FILE")" != *"SHA-256 mismatch"* ]]; then
  echo "Checksum mismatch did not report the expected error" >&2
  exit 1
fi

if "$VERIFY" "$FIXTURE" "not-a-checksum" 2> "$STDERR_FILE"; then
  echo "Expected a malformed checksum to fail" >&2
  exit 1
fi
if [[ "$(<"$STDERR_FILE")" != *"64 lowercase hexadecimal characters"* ]]; then
  echo "Malformed checksum did not report the expected error" >&2
  exit 1
fi

echo "SHA-256 verification tests passed"
