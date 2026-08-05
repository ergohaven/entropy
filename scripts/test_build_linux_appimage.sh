#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUILD="$ROOT/scripts/build_linux_appimage.sh"
VERIFY="$ROOT/scripts/verify_sha256.sh"
# shellcheck disable=SC1091
source "$ROOT/scripts/appimagetool_pin.sh"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

hash_file() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  else
    shasum -a 256 "$1" | awk '{print $1}'
  fi
}

PINNED_TOOL="$TMP_DIR/pinned-appimagetool"
curl --connect-timeout 30 --max-time 300 -fsSL \
  "$APPIMAGETOOL_PINNED_URL" \
  -o "$PINNED_TOOL"
"$VERIFY" "$PINNED_TOOL" "$APPIMAGETOOL_PINNED_SHA256"

STUB_BIN="$TMP_DIR/bin"
mkdir -p "$STUB_BIN"

cat > "$STUB_BIN/cargo" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF

cat > "$STUB_BIN/install" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
destination="${!#}"
mkdir -p "$(dirname "$destination")"
: > "$destination"
chmod 0755 "$destination"
EOF

cat > "$STUB_BIN/curl" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
output=""
while [[ $# -gt 0 ]]; do
  if [[ "$1" == "-o" ]]; then
    output="$2"
    shift 2
  else
    shift
  fi
done
[[ -n "$output" ]]
printf 'called\n' > "$APPIMAGETOOL_CURL_MARKER"
cp "$APPIMAGETOOL_FIXTURE" "$output"
EOF

chmod 0755 "$STUB_BIN/cargo" "$STUB_BIN/install" "$STUB_BIN/curl"

TRUSTED_TOOL="$TMP_DIR/trusted-tool"
cat > "$TRUSTED_TOOL" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf 'ran\n' > "$APPIMAGETOOL_RUN_MARKER"
: > "$2"
EOF
chmod 0755 "$TRUSTED_TOOL"
TRUSTED_SHA256="$(hash_file "$TRUSTED_TOOL")"

CORRUPT_TOOL="$TMP_DIR/corrupt-tool"
printf 'corrupt\n' > "$CORRUPT_TOOL"

run_build() {
  local scenario="$1"
  local fixture="$2"
  local tool="$scenario/appimagetool"
  local output="$scenario/entropy.AppImage"

  mkdir -p "$scenario"
  PATH="$STUB_BIN:$PATH" \
    APPDIR="$scenario/Entropy.AppDir" \
    APPIMAGETOOL="$tool" \
    APPIMAGETOOL_URL="https://example.invalid/appimagetool" \
    APPIMAGETOOL_SHA256="$TRUSTED_SHA256" \
    APPIMAGETOOL_FIXTURE="$fixture" \
    APPIMAGETOOL_CURL_MARKER="$scenario/curl-called" \
    APPIMAGETOOL_RUN_MARKER="$scenario/tool-ran" \
    "$BUILD" vtest "$output"
}

FRESH_VALID="$TMP_DIR/fresh-valid"
run_build "$FRESH_VALID" "$TRUSTED_TOOL"
[[ -x "$FRESH_VALID/appimagetool" ]]
[[ -f "$FRESH_VALID/curl-called" ]]
[[ -f "$FRESH_VALID/tool-ran" ]]
[[ -f "$FRESH_VALID/entropy.AppImage" ]]

FRESH_CORRUPT="$TMP_DIR/fresh-corrupt"
mkdir -p "$FRESH_CORRUPT"
if run_build "$FRESH_CORRUPT" "$CORRUPT_TOOL" 2> "$FRESH_CORRUPT/stderr"; then
  echo "Expected a corrupt download to fail" >&2
  exit 1
fi
[[ "$(<"$FRESH_CORRUPT/stderr")" == *"SHA-256 mismatch"* ]]
[[ ! -e "$FRESH_CORRUPT/appimagetool" ]]
[[ ! -e "$FRESH_CORRUPT/tool-ran" ]]
if compgen -G "$FRESH_CORRUPT/appimagetool.download.*" >/dev/null; then
  echo "Corrupt download temporary file was not cleaned up" >&2
  exit 1
fi

CACHED_VALID="$TMP_DIR/cached-valid"
mkdir -p "$CACHED_VALID"
cp "$TRUSTED_TOOL" "$CACHED_VALID/appimagetool"
run_build "$CACHED_VALID" "$CORRUPT_TOOL"
[[ ! -e "$CACHED_VALID/curl-called" ]]
[[ -f "$CACHED_VALID/tool-ran" ]]
[[ -f "$CACHED_VALID/entropy.AppImage" ]]

# A cached tool that no longer matches the pin is refetched: that is what a pin
# bump looks like locally. The replacement is verified like any other download.
CACHED_REFETCH="$TMP_DIR/cached-refetch"
mkdir -p "$CACHED_REFETCH"
cp "$CORRUPT_TOOL" "$CACHED_REFETCH/appimagetool"
chmod 0755 "$CACHED_REFETCH/appimagetool"
run_build "$CACHED_REFETCH" "$TRUSTED_TOOL"
[[ -f "$CACHED_REFETCH/curl-called" ]]
[[ "$(hash_file "$CACHED_REFETCH/appimagetool")" == "$TRUSTED_SHA256" ]]
[[ -f "$CACHED_REFETCH/tool-ran" ]]
[[ -f "$CACHED_REFETCH/entropy.AppImage" ]]

# ...but the refetch is not an escape hatch: if the source is corrupt too, the
# build fails and the unverified tool is never executed.
CACHED_CORRUPT="$TMP_DIR/cached-corrupt"
mkdir -p "$CACHED_CORRUPT"
cp "$CORRUPT_TOOL" "$CACHED_CORRUPT/appimagetool"
chmod 0755 "$CACHED_CORRUPT/appimagetool"
if run_build "$CACHED_CORRUPT" "$CORRUPT_TOOL" 2> "$CACHED_CORRUPT/stderr"; then
  echo "Expected a corrupt cached tool with a corrupt source to fail" >&2
  exit 1
fi
[[ "$(<"$CACHED_CORRUPT/stderr")" == *"SHA-256 mismatch"* ]]
[[ ! -e "$CACHED_CORRUPT/tool-ran" ]]
if compgen -G "$CACHED_CORRUPT/appimagetool.download.*" >/dev/null; then
  echo "Corrupt refetch temporary file was not cleaned up" >&2
  exit 1
fi

echo "AppImage tool integration tests passed"
