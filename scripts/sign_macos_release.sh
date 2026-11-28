#!/usr/bin/env bash
# Release packaging only: Cargo and the importer are built before this step.
set -euo pipefail
set +x
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/macos_stable_signing.sh"
[[ "${GITHUB_REPOSITORY:-}" == ergohaven/entropy && (
   ( "${GITHUB_EVENT_NAME:-}" == push && "${GITHUB_REF:-}" == refs/tags/v0.* ) ||
   ( "${GITHUB_EVENT_NAME:-}" == workflow_dispatch &&
     "${GITHUB_REF:-}" == refs/heads/ci/macos-signing-validation-pr195 &&
     "${MACOS_SIGNING_TEST:-}" == 1 ) ||
   ( "${GITHUB_EVENT_NAME:-}" == workflow_dispatch &&
     "${GITHUB_REF:-}" == refs/heads/main &&
     "${GITHUB_WORKFLOW_REF:-}" == ergohaven/entropy/.github/workflows/release.yml@refs/heads/main &&
     "${ENTROPY_RELEASE_CANDIDATE:-}" == 1 )
) ]] || {
    echo 'Signing is restricted to upstream release tags, the exact opted-in main candidate or manual test branch' >&2
    exit 1
}
[[ -n "${MACOS_CERTIFICATE_P12_BASE64:-}" && -n "${MACOS_CERTIFICATE_PASSWORD:-}" ]] || {
    echo 'Missing signing credentials; refusing ad-hoc release' >&2; exit 1;
}
# Keep credentials out of all child environments except the importer.
CERTIFICATE_P12_BASE64="$MACOS_CERTIFICATE_P12_BASE64"
CERTIFICATE_PASSWORD="$MACOS_CERTIFICATE_PASSWORD"
unset MACOS_CERTIFICATE_P12_BASE64 MACOS_CERTIFICATE_PASSWORD
[[ "$(uname -s)" == Darwin ]] || { echo 'Requires macOS' >&2; exit 1; }
: "${RUNNER_TEMP:?Missing runner temp directory}"
: "${TARGET:?Missing macOS target}"
[[ "$TARGET" == aarch64-apple-darwin || "$TARGET" == x86_64-apple-darwin ]] || exit 1
[[ -x "$RUNNER_TEMP/entropy-certificate-import" ]] || {
    echo 'Compile the certificate importer before providing credentials' >&2; exit 1;
}
umask 077
# Only fixed public success markers may leave the signing process. Remove stale
# evidence before doing anything with a keychain; failures never retain a pass.
rm -f "$RUNNER_TEMP/entropy-signing-cleanup.txt"
# Snapshot stays in memory; preserve quoted paths, including spaces.
ORIGINAL_KEYCHAINS="$(security list-keychains -d user)"
SIGNING_DIR="$(mktemp -d "$RUNNER_TEMP/entropy-signing.XXXXXX")"
KEYCHAIN="$SIGNING_DIR/signing.keychain-db"
set_search_list() {
    local keychain
    local paths=()
    while read -r keychain; do
        keychain="${keychain#\"}"
        keychain="${keychain%\"}"
        [[ -z "$keychain" ]] || paths+=("$keychain")
    done <<< "$ORIGINAL_KEYCHAINS"
    if [[ "${1:-}" == register ]]; then paths+=("$KEYCHAIN"); fi
    security list-keychains -d user -s "${paths[@]}"
}
cleanup() {
    local status=$? failed=0 restored=''
    trap - EXIT
    # Delete first: security may itself update the list on deletion.
    if [[ -e "$KEYCHAIN" ]]; then
        security delete-keychain "$KEYCHAIN" >/dev/null 2>&1 || failed=1
    fi
    set_search_list >/dev/null 2>&1 || failed=1
    restored="$(security list-keychains -d user 2>/dev/null)" || failed=1
    [[ "$restored" == "$ORIGINAL_KEYCHAINS" ]] || failed=1
    rm -rf "$SIGNING_DIR" || failed=1
    [[ ! -e "$SIGNING_DIR" && ! -e "$KEYCHAIN" ]] || failed=1
    if [[ "$failed" != 0 ]]; then
        echo 'Failed to clean up signing keychain or restore search list' >&2
        exit 1
    fi
    if [[ "$status" == 0 ]]; then
        printf '%s\n' 'temporary_keychain_deleted=yes' 'search_list_restored=yes' \
            > "$RUNNER_TEMP/entropy-signing-cleanup.txt"
    fi
    exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
KEYCHAIN_PASSWORD="$(openssl rand -hex 32)"
security create-keychain -p "$KEYCHAIN_PASSWORD" "$KEYCHAIN" >/dev/null 2>&1
security set-keychain-settings -lut 21600 "$KEYCHAIN" >/dev/null 2>&1
security unlock-keychain -p "$KEYCHAIN_PASSWORD" "$KEYCHAIN" >/dev/null 2>&1
# Only this importer receives P12/password; raw diagnostics are never logged.
MACOS_CERTIFICATE_P12_BASE64="$CERTIFICATE_P12_BASE64" \
    MACOS_CERTIFICATE_PASSWORD="$CERTIFICATE_PASSWORD" \
    "$RUNNER_TEMP/entropy-certificate-import" "$KEYCHAIN" >/dev/null 2>&1
unset CERTIFICATE_P12_BASE64 CERTIFICATE_PASSWORD
security set-key-partition-list -S apple-tool:,apple:,codesign: -s \
    -k "$KEYCHAIN_PASSWORD" "$KEYCHAIN" >/dev/null 2>&1
unset KEYCHAIN_PASSWORD
# find-identity -v excludes untrusted self-signed certificates; pin the public cert.
security find-certificate -a -Z "$KEYCHAIN" |
    grep -Fx "SHA-1 hash: $MACOS_RELEASE_CERTIFICATE_SHA1" >/dev/null
set_search_list register >/dev/null 2>&1
CODESIGN_IDENTITY="$MACOS_RELEASE_CERTIFICATE_SHA1" CODESIGN_KEYCHAIN="$KEYCHAIN" \
    BUNDLE_ID=com.ergohaven.entropy REQUIRE_STABLE_SIGNING=1 SKIP_BUILD=1 \
    bash "$ROOT/scripts/build_macos_app.sh"
