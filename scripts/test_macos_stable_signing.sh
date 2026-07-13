#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/macos_stable_signing.sh"

fail() {
	echo "FAIL: $*" >&2
	exit 1
}

expect_success() {
	local description="$1"
	shift
	"$@" || fail "$description"
}

expect_failure() {
	local description="$1"
	shift
	if "$@"; then
		fail "$description"
	fi
}

adhoc_details=$'Signature=adhoc\nTeamIdentifier=not set'
adhoc_requirement='designated => cdhash H"d8b56c9110cf6f078cd45f0c4d19876a3ff9b288"'
certificate_sha1='0123456789abcdef0123456789abcdef01234567'
bundle_id='com.ergohaven.entropy'
self_signed_details=$'Authority=Entropy Open Source Release Signing\nTeamIdentifier=not set'
self_signed_requirement="designated => certificate root = H\"$certificate_sha1\" and identifier \"$bundle_id\""

expect_failure \
	"required stable signing accepted an ad-hoc identity" \
	macos_validate_stable_signing_configuration "-" "1" "" "$bundle_id"
expect_success \
	"local ad-hoc packaging should remain available" \
	macos_validate_stable_signing_configuration "-" "0" "" "$bundle_id"
expect_failure \
	"stable signing accepted a missing certificate hash" \
	macos_validate_stable_signing_configuration \
	"Entropy Open Source Release Signing" \
	"1" \
	"" \
	"$bundle_id"
expect_failure \
	"stable signing accepted an invalid certificate hash" \
	macos_validate_stable_signing_configuration \
	"Entropy Open Source Release Signing" \
	"1" \
	"invalid" \
	"$bundle_id"
expect_success \
	"valid stable signing configuration failed" \
	macos_validate_stable_signing_configuration \
	"Entropy Open Source Release Signing" \
	"1" \
	"$certificate_sha1" \
	"$bundle_id"
expect_failure \
	"ad-hoc signature passed stable validation" \
	macos_validate_stable_signature_output \
	"$adhoc_details" \
	"$adhoc_requirement" \
	"$certificate_sha1" \
	"$bundle_id"
expect_failure \
	"hash-only designated requirement passed stable validation" \
	macos_validate_stable_signature_output \
	"$self_signed_details" \
	"$adhoc_requirement" \
	"$certificate_sha1" \
	"$bundle_id"
expect_failure \
	"wrong certificate passed stable validation" \
	macos_validate_stable_signature_output \
	"$self_signed_details" \
	"$self_signed_requirement" \
	"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" \
	"$bundle_id"
expect_success \
	"self-signed certificate requirement failed stable validation" \
	macos_validate_stable_signature_output \
	"$self_signed_details" \
	"$self_signed_requirement" \
	"$certificate_sha1" \
	"$bundle_id"

mock_codesign_details="$adhoc_details"
mock_codesign_requirement="$adhoc_requirement"
codesign() {
	case "$1" in
	--verify) return 0 ;;
	-dv) printf '%s\n' "$mock_codesign_details" >&2 ;;
	-d) printf '%s\n' "$mock_codesign_requirement" >&2 ;;
	*) fail "unexpected mock codesign arguments: $*" ;;
	esac
}

expect_failure \
	"app verification swallowed ad-hoc signature rejection" \
	macos_verify_stable_signature \
	"/tmp/Entropy.app" \
	"$certificate_sha1" \
	"$bundle_id"
mock_codesign_details="$self_signed_details"
mock_codesign_requirement="$self_signed_requirement"
expect_success \
	"app verification rejected a stable self-signed signature" \
	macos_verify_stable_signature \
	"/tmp/Entropy.app" \
	"$certificate_sha1" \
	"$bundle_id"

echo "macOS stable signing unit tests passed"
