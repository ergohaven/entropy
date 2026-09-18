#!/usr/bin/env sh
# pkg_version.sh <deb|rpm|archlinux> <semver> — версия пакета для формата.
#
# nfpm сам разбирает семвер, но результат у форматов разный, и для archlinux он
# неверен: prerelease теряется, и 0.3.21-rc.1 со стабильной 0.3.21 дают один
# и тот же pkgver = 0.3.21-1. Поэтому версию считаем здесь, а nfpm получает уже
# готовую строку.
#
# Порядок сортировки (проверяется scripts/test_pkg_version.sh и штатными
# компараторами дистрибутивов в scripts/test_linux_install.sh):
#   deb/rpm    — '~' сортируется ПЕРЕД пустотой: 0.3.21~rc.1 < 0.3.21
#   archlinux  — pacman не знает '~', но в его vercmp остаток-буквы проигрывает
#                пустоте: 0.3.21rc.1 < 0.3.21
set -eu

usage() {
	echo "usage: pkg_version.sh <deb|rpm|archlinux> <semver>" >&2
	exit 2
}

[ $# -eq 2 ] || usage
format="$1"
version="$2"

base="${version%%-*}"
pre="${version#"$base"}"
pre="${pre#-}"

case "$base" in
[0-9]*.[0-9]*.[0-9]*) ;;
*)
	echo "pkg_version: '$version' is not X.Y.Z[-PRERELEASE]" >&2
	exit 1
	;;
esac
case "$base" in
*[!0-9.]*)
	echo "pkg_version: '$version' is not X.Y.Z[-PRERELEASE]" >&2
	exit 1
	;;
esac

if [ -n "$pre" ]; then
	case "$pre" in
	*[!0-9A-Za-z.]*)
		echo "pkg_version: prerelease '$pre' may only contain letters, digits and dots" >&2
		exit 1
		;;
	# Для archlinux суффикс приклеивается к версии без разделителя, поэтому
	# цифра в начале слилась бы с patch-номером: 0.3.21-1 -> 0.3.211.
	[!A-Za-z]*)
		echo "pkg_version: prerelease '$pre' must start with a letter" >&2
		exit 1
		;;
	esac
fi

case "$format" in
deb | rpm) [ -n "$pre" ] && printf '%s~%s\n' "$base" "$pre" || printf '%s\n' "$base" ;;
archlinux) [ -n "$pre" ] && printf '%s%s\n' "$base" "$pre" || printf '%s\n' "$base" ;;
*) usage ;;
esac
