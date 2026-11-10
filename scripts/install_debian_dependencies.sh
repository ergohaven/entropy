#!/bin/sh
# Keep package selection/authentication on the pinned snapshot. If its package
# endpoint is unavailable, fetch the identical .deb from Debian's live pool;
# apt-helper enforces the SHA256 from APT's authenticated snapshot metadata.
set -eu

apt-get -o Acquire::Retries=2 -o Acquire::https::Timeout=30 \
	-o APT::Update::Error-Mode=any update
if ! apt-get -o Acquire::Retries=2 -o Acquire::https::Timeout=30 \
	--download-only -y --no-install-recommends install "$@"; then
	plan="$(mktemp)"
	trap 'rm -f "$plan"' 0
	trap 'exit 1' HUP INT TERM
	apt-get -o Acquire::ForceHash=SHA256 --print-uris \
		--download-only -y --no-install-recommends install "$@" > "$plan"
	eval "$(apt-config shell ARCHIVES Dir::Cache::archives/d)"
	: "${ARCHIVES:?APT archive directory is missing}"
	while read -r uri filename size checksum rest; do
		case "$uri" in
		\'https://snapshot.debian.org/archive/debian/*/pool/*\')
			uri="${uri#\'}"; uri="${uri%\'}"
			path="${uri#https://snapshot.debian.org/archive/debian/}"
			mirror="https://deb.debian.org/debian/${path#*/}" ;;
		\'https://snapshot.debian.org/archive/debian-security/*/pool/*\')
			uri="${uri#\'}"; uri="${uri%\'}"
			path="${uri#https://snapshot.debian.org/archive/debian-security/}"
			mirror="https://deb.debian.org/debian-security/${path#*/}" ;;
		\'*) echo "Refusing non-snapshot package URI: $uri" >&2; exit 1 ;;
		*) continue ;;
		esac
		case "$filename" in
		'' | .* | *[!a-zA-Z0-9.+_:%~-]*) echo 'Unsafe APT archive filename' >&2; exit 1 ;;
		*.deb) ;;
		*) echo 'Expected a Debian package archive' >&2; exit 1 ;;
		esac
		hash="${checksum#SHA256:}"
		case "$hash" in
		'' | *[!a-fA-F0-9]*) echo 'Missing SHA256 in APT download plan' >&2; exit 1 ;;
		esac
		[ "$checksum" != "$hash" ] && [ "${#hash}" -eq 64 ] || {
			echo 'Expected a complete SHA256 in APT download plan' >&2; exit 1;
		}
		apt-helper -o Acquire::Retries=2 -o Acquire::https::Timeout=30 \
			download-file "$mirror" "${ARCHIVES}${filename}" "$checksum"
	done < "$plan"
fi
# APT retains authentication/dependency checks; it must not silently return to
# the failing network endpoint or install a different package version.
apt-get --no-download -y --no-install-recommends install "$@"
