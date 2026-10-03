#!/usr/bin/env bash
# glibc_baseline.sh <binary> — максимальная версия символов GLIBC_*, которую
# требует бинарник.
#
# Собранная на новом дистрибутиве сборка тянет символы новее, чем есть у
# пользователя, и падает на старте с "version `GLIBC_2.39' not found". Пакет
# обязан объявить эту границу, а не просто "libc6", поэтому она считается по
# самому бинарнику, а не по тому, где шла сборка.
set -euo pipefail

bin="${1:?usage: glibc_baseline.sh <binary>}"

if ! command -v readelf >/dev/null 2>&1; then
	echo "glibc_baseline: readelf is required (binutils)" >&2
	exit 1
fi

baseline="$(readelf --version-info --wide "$bin" |
	grep -oE 'GLIBC_[0-9]+(\.[0-9]+)+' |
	sed 's/^GLIBC_//' |
	sort -uV |
	tail -n1)"

if [[ -z $baseline ]]; then
	echo "glibc_baseline: no GLIBC_* version requirements in $bin" >&2
	exit 1
fi

printf '%s\n' "$baseline"
