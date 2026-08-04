#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
UPDATE="$ROOT/scripts/update_appimagetool_pin.sh"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

hash_file() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  else
    shasum -a 256 "$1" | awk '{print $1}'
  fi
}

PIN_FILE="$TMP_DIR/appimagetool_pin.sh"
cat > "$PIN_FILE" <<'EOF'
readonly APPIMAGETOOL_PINNED_RELEASE="11"
readonly APPIMAGETOOL_PINNED_URL="https://github.com/AppImage/AppImageKit/releases/download/11/appimagetool-x86_64.AppImage"
readonly APPIMAGETOOL_PINNED_SHA256="0000000000000000000000000000000000000000000000000000000000000000"
EOF
cp "$PIN_FILE" "$TMP_DIR/original_pin.sh"

FIXTURE="$TMP_DIR/appimagetool-x86_64.AppImage"
printf 'trusted AppImageTool release\n' > "$FIXTURE"
EXPECTED_SHA256="$(hash_file "$FIXTURE")"

STUB_BIN="$TMP_DIR/bin"
mkdir -p "$STUB_BIN"
cat > "$STUB_BIN/curl" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

output=""
for ((index = 1; index <= $#; index += 1)); do
  if [[ "${!index}" == "--output" ]]; then
    next_index=$((index + 1))
    output="${!next_index}"
    break
  fi
done

[[ -n "$output" ]]
printf '%s\n' "$*" > "$APPIMAGETOOL_UPDATE_CURL_ARGS"
cp "$APPIMAGETOOL_UPDATE_FIXTURE" "$output"
EOF
chmod 0755 "$STUB_BIN/curl"

PATH="$STUB_BIN:$PATH" \
  APPIMAGETOOL_PIN_FILE="$PIN_FILE" \
  APPIMAGETOOL_UPDATE_FIXTURE="$FIXTURE" \
  APPIMAGETOOL_UPDATE_CURL_ARGS="$TMP_DIR/curl-args" \
  bash "$UPDATE" 12

# shellcheck disable=SC1090
source "$PIN_FILE"
[[ "$APPIMAGETOOL_PINNED_RELEASE" == "12" ]]
[[ "$APPIMAGETOOL_PINNED_URL" == "https://github.com/AppImage/AppImageKit/releases/download/12/appimagetool-x86_64.AppImage" ]]
[[ "$APPIMAGETOOL_PINNED_SHA256" == "$EXPECTED_SHA256" ]]
[[ "$(<"$TMP_DIR/curl-args")" == *"/releases/download/12/appimagetool-x86_64.AppImage"* ]]

cp "$PIN_FILE" "$TMP_DIR/updated_pin.sh"

if PATH="$STUB_BIN:$PATH" APPIMAGETOOL_PIN_FILE="$PIN_FILE" bash "$UPDATE" continuous; then
  echo "Expected the mutable continuous tag to be rejected" >&2
  exit 1
fi
cmp -s "$PIN_FILE" "$TMP_DIR/updated_pin.sh"

if PATH="$STUB_BIN:$PATH" APPIMAGETOOL_PIN_FILE="$PIN_FILE" bash "$UPDATE" '12/../continuous'; then
  echo "Expected an unsafe release tag to be rejected" >&2
  exit 1
fi

cmp -s "$PIN_FILE" "$TMP_DIR/updated_pin.sh"

echo "AppImageTool pin update tests passed"
