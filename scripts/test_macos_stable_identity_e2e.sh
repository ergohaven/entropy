#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
	echo "Skipped macOS stable identity end-to-end test"
	exit 0
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/macos_stable_signing.sh"

TMP_DIR="$(mktemp -d /tmp/entropy-stable-signing.XXXXXX)"
KEYCHAIN_PATH="$TMP_DIR/test.keychain-db"
KEYCHAIN_PASSWORD="entropy-test-keychain"
P12_PASSWORD="entropy-test-p12"
IDENTITY="Entropy Open Source Release Signing"
BUNDLE_ID="com.ergohaven.entropy"
ORIGINAL_KEYCHAINS=()

while IFS= read -r keychain; do
	keychain="${keychain//\"/}"
	ORIGINAL_KEYCHAINS+=("$keychain")
done < <(security list-keychains -d user)

cleanup() {
	security list-keychains -d user -s "${ORIGINAL_KEYCHAINS[@]}" >/dev/null 2>&1 || true
	security delete-keychain "$KEYCHAIN_PATH" >/dev/null 2>&1 || true
	rm -rf "$TMP_DIR"
}
trap cleanup EXIT

openssl req -new -newkey rsa:2048 -x509 -sha256 -nodes \
	-days 30 \
	-subj "/CN=$IDENTITY/O=Entropy Open Source" \
	-addext "basicConstraints=critical,CA:TRUE" \
	-addext "keyUsage=critical,digitalSignature,keyCertSign" \
	-addext "extendedKeyUsage=codeSigning" \
	-keyout "$TMP_DIR/key.pem" \
	-out "$TMP_DIR/cert.pem" >/dev/null 2>&1
openssl pkcs12 -export \
	-inkey "$TMP_DIR/key.pem" \
	-in "$TMP_DIR/cert.pem" \
	-name "$IDENTITY" \
	-passout "pass:$P12_PASSWORD" \
	-out "$TMP_DIR/identity.p12"

security create-keychain -p "$KEYCHAIN_PASSWORD" "$KEYCHAIN_PATH"
security set-keychain-settings -lut 21600 "$KEYCHAIN_PATH"
security unlock-keychain -p "$KEYCHAIN_PASSWORD" "$KEYCHAIN_PATH"
security import "$TMP_DIR/identity.p12" \
	-P "$P12_PASSWORD" \
	-A \
	-f pkcs12 \
	-k "$KEYCHAIN_PATH" >/dev/null
security set-key-partition-list \
	-S apple-tool:,apple:,codesign: \
	-s \
	-k "$KEYCHAIN_PASSWORD" \
	"$KEYCHAIN_PATH" >/dev/null
security list-keychains -d user -s "$KEYCHAIN_PATH" "${ORIGINAL_KEYCHAINS[@]}"

security find-certificate \
	-c "$IDENTITY" \
	-p \
	"$KEYCHAIN_PATH" > "$TMP_DIR/imported-cert.pem"

printf '%s\n' 'int main(void) { return 1; }' |
	clang -x c - -o "$TMP_DIR/entropy-v1"
printf '%s\n' 'int main(void) { return 2; }' |
	clang -x c - -o "$TMP_DIR/entropy-v2"

CERTIFICATE_SHA1="$(
	openssl x509 -in "$TMP_DIR/imported-cert.pem" -noout -fingerprint -sha1 |
		sed 's/^.*=//; s/://g'
)"
REQUIREMENT="$(macos_stable_designated_requirement "$CERTIFICATE_SHA1" "$BUNDLE_ID")"

for binary in "$TMP_DIR/entropy-v1" "$TMP_DIR/entropy-v2"; do
	codesign --force \
		--sign "$IDENTITY" \
		--keychain "$KEYCHAIN_PATH" \
		--identifier "$BUNDLE_ID" \
		--requirements "$REQUIREMENT" \
		--options runtime \
		--timestamp=none \
		"$binary"
	macos_verify_stable_signature "$binary" "$CERTIFICATE_SHA1" "$BUNDLE_ID"
done

requirement_v1="$(codesign -d -r- "$TMP_DIR/entropy-v1" 2>&1 | sed -n '/^designated =>/p')"
requirement_v2="$(codesign -d -r- "$TMP_DIR/entropy-v2" 2>&1 | sed -n '/^designated =>/p')"
cdhash_v1="$(codesign -dvvv "$TMP_DIR/entropy-v1" 2>&1 | awk -F= '/^CDHash=/{print $2}')"
cdhash_v2="$(codesign -dvvv "$TMP_DIR/entropy-v2" 2>&1 | awk -F= '/^CDHash=/{print $2}')"

if [[ "$requirement_v1" != "$requirement_v2" ]]; then
	echo "Different builds produced different designated requirements" >&2
	exit 1
fi
if [[ -z "$cdhash_v1" || -z "$cdhash_v2" || "$cdhash_v1" == "$cdhash_v2" ]]; then
	echo "Test binaries did not produce distinct code hashes" >&2
	exit 1
fi

echo "$requirement_v1"
echo "macOS stable identity end-to-end test passed"
