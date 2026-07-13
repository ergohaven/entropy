#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/macos_distribution_signing.sh"

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
adhoc_requirement='# designated => cdhash H"d8b56c9110cf6f078cd45f0c4d19876a3ff9b288"'
developer_id_details=$'Authority=Developer ID Application: Example Developer (TEAM123456)\nTeamIdentifier=TEAM123456'
developer_id_requirement='# designated => anchor apple generic and identifier "com.ergohaven.entropy" and certificate leaf[subject.OU] = TEAM123456'

expect_failure \
	"required distribution signing accepted an ad-hoc identity" \
	macos_validate_signing_configuration "-" "1"
expect_success \
	"local ad-hoc packaging should remain available" \
	macos_validate_signing_configuration "-" "0"
expect_failure \
	"ad-hoc signature passed distribution validation" \
	macos_validate_distribution_signature_output "$adhoc_details" "$adhoc_requirement"
expect_failure \
	"hash-only designated requirement passed distribution validation" \
	macos_validate_distribution_signature_output \
	"$developer_id_details" \
	"$adhoc_requirement"
expect_success \
	"Developer ID signature failed distribution validation" \
	macos_validate_distribution_signature_output \
	"$developer_id_details" \
	"$developer_id_requirement"

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
	macos_verify_distribution_signature "/tmp/Entropy.app"
mock_codesign_details="$developer_id_details"
mock_codesign_requirement="$developer_id_requirement"
expect_success \
	"app verification rejected a Developer ID signature" \
	macos_verify_distribution_signature "/tmp/Entropy.app"

echo "macOS distribution signing tests passed"
