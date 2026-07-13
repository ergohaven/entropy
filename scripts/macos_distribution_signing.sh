#!/usr/bin/env bash

macos_validate_signing_configuration() {
	local identity="${1:-}"
	local required="${2:-0}"

	if [[ "$required" != "0" && "$required" != "1" ]]; then
		echo "REQUIRE_DISTRIBUTION_SIGNING must be 0 or 1" >&2
		return 1
	fi

	if [[ "$required" == "1" && ( -z "$identity" || "$identity" == "-" ) ]]; then
		echo "Developer ID signing is required; refusing to build an ad-hoc release" >&2
		return 1
	fi
}

macos_validate_distribution_signature_output() {
	local details="$1"
	local requirement="$2"

	if [[ "$details" == *"Signature=adhoc"* ]]; then
		echo "Distribution app is ad-hoc signed" >&2
		return 1
	fi
	if [[ "$details" != *"TeamIdentifier="* ]] ||
		[[ "$details" == *"TeamIdentifier=not set"* ]]; then
		echo "Distribution app has no Team ID" >&2
		return 1
	fi
	if [[ "$details" != *"Authority=Developer ID Application:"* ]]; then
		echo "Distribution app is not signed with a Developer ID Application certificate" >&2
		return 1
	fi
	if [[ "$requirement" != *"# designated =>"* ]]; then
		echo "Distribution app has no designated requirement" >&2
		return 1
	fi
	if [[ "$requirement" == *"# designated => cdhash "* ]]; then
		echo "Distribution app designated requirement is tied to one binary hash" >&2
		return 1
	fi
}

macos_verify_distribution_signature() {
	local app_path="$1"
	local details
	local requirement

	codesign --verify --deep --strict --verbose=2 "$app_path" || return 1
	details="$(codesign -dv --verbose=4 "$app_path" 2>&1)" || return 1
	requirement="$(codesign -d -r- "$app_path" 2>&1)" || return 1
	macos_validate_distribution_signature_output "$details" "$requirement" || return 1
	echo "Validated stable Developer ID signature for $app_path"
}
