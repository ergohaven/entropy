#!/usr/bin/env sh
# One public version owner in Cargo.toml; retain historical SemVer-only manifests.
set -eu
root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
awk -F '"' '
    /^\[package\]$/ { section = "package"; next }
    /^\[package.metadata.entropy\]$/ { section = "public"; next }
    /^\[/ { section = "" }
    section == "package" && /^version = / { internal = $2 }
    section == "public" && /^public-version = / { public = $2; has_public = 1 }
    END {
        if (internal == "") exit 1
        if (!has_public) { print internal; exit }
        if (public !~ /^[0-9]+\.[0-9]+(\.[0-9]+)?$/) exit 1
        normalized = public
        if (split(public, parts, ".") == 2) normalized = public ".0"
        if (normalized != internal) exit 1
        print public
    }
' "${1:-$root/Cargo.toml}"
