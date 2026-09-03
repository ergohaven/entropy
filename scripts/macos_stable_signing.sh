#!/usr/bin/env bash

macos_stable_designated_requirement() {
	local certificate_sha1="${1:-}"
	local bundle_id="${2:-}"
	local normalized_sha1

	normalized_sha1="$(printf '%s' "$certificate_sha1" | tr '[:upper:]' '[:lower:]')"

	printf '=designated => anchor = H"%s" and identifier "%s"\n' \
		"$normalized_sha1" \
		"$bundle_id"
}

macos_validate_stable_signing_configuration() {
	local identity="${1:-}"
	local required="${2:-0}"
	local certificate_sha1="${3:-}"
	local bundle_id="${4:-}"

	if [[ "$required" != "0" && "$required" != "1" ]]; then
		echo "REQUIRE_STABLE_SIGNING must be 0 or 1" >&2
		return 1
	fi

	if [[ "$required" == "1" && ( -z "$identity" || "$identity" == "-" ) ]]; then
		echo "Stable signing is required; refusing to build an ad-hoc release" >&2
		return 1
	fi

	if [[ -n "$identity" && "$identity" != "-" ]]; then
		if [[ ! "$certificate_sha1" =~ ^[[:xdigit:]]{40}$ ]]; then
			echo "MACOS_SIGNING_CERTIFICATE_SHA1 must contain 40 hexadecimal characters" >&2
			return 1
		fi
		if [[ ! "$bundle_id" =~ ^[[:alnum:].-]+$ ]]; then
			echo "BUNDLE_ID contains unsupported characters" >&2
			return 1
		fi
	fi
}

macos_validate_stable_signature_output() {
	local details="$1"
	local requirement="$2"
	local certificate_sha1="$3"
	local bundle_id="$4"
	local expected_requirement

	certificate_sha1="$(printf '%s' "$certificate_sha1" | tr '[:upper:]' '[:lower:]')"
	expected_requirement="designated => certificate root = H\"$certificate_sha1\" and identifier \"$bundle_id\""

	if [[ "$details" == *"Signature=adhoc"* ]]; then
		echo "Release app is ad-hoc signed" >&2
		return 1
	fi
	if [[ "$details" != *"Authority="* ]]; then
		echo "Release app has no certificate authority" >&2
		return 1
	fi
	if [[ "$requirement" == *"cdhash "* ]]; then
		echo "Release app designated requirement is tied to one binary hash" >&2
		return 1
	fi
	if [[ "$requirement" != *"$expected_requirement"* ]]; then
		echo "Release app designated requirement does not match expected certificate and bundle ID" >&2
		return 1
	fi
}

macos_verify_stable_signature() {
	local app_path="$1"
	local certificate_sha1="$2"
	local bundle_id="$3"
	local details
	local requirement

	codesign --verify --deep --strict --verbose=2 "$app_path" || return 1
	details="$(codesign -dv --verbose=4 "$app_path" 2>&1)" || return 1
	requirement="$(codesign -d -r- "$app_path" 2>&1)" || return 1
	macos_validate_stable_signature_output \
		"$details" \
		"$requirement" \
		"$certificate_sha1" \
		"$bundle_id" || return 1
	echo "Validated stable certificate-anchored signature for $app_path"
}
