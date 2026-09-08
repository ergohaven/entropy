#!/usr/bin/env bash
# Порядок версий — контракт с пакетным менеджером: если rc сортируется выше
# стабильной, апгрейд с 0.3.21-rc.1 на 0.3.21 не приедет никогда. Здесь
# проверяется сам перевод, а сравнение штатными компараторами дистрибутивов —
# в scripts/test_linux_install.sh.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PKG_VERSION="$ROOT/scripts/pkg_version.sh"

failures=0
fail() {
  echo "FAIL: $*" >&2
  failures=$((failures + 1))
}

expect() { # format input expected
  local got
  if ! got="$("$PKG_VERSION" "$1" "$2" 2>&1)"; then
    fail "$1 $2: exited non-zero: $got"
    return
  fi
  [[ $got == "$3" ]] || fail "$1 $2: got '$got', want '$3'"
}

expect_reject() { # format input
  if "$PKG_VERSION" "$1" "$2" >/dev/null 2>&1; then
    fail "$1 '$2': accepted, expected a rejection"
  fi
}

# '~' сортируется перед пустотой у dpkg и rpm.
expect deb 0.3.21-rc.1 '0.3.21~rc.1'
expect deb 0.3.21 '0.3.21'
expect rpm 0.3.21-rc.1 '0.3.21~rc.1'
expect rpm 0.3.21 '0.3.21'
# pacman '~' не знает, но в его vercmp остаток из букв проигрывает пустоте,
# поэтому суффикс приклеивается без разделителя.
expect archlinux 0.3.21-rc.1 '0.3.21rc.1'
expect archlinux 0.3.21 '0.3.21'
expect archlinux 1.0.0-beta.2 '1.0.0beta.2'

expect_reject deb 0.3
expect_reject deb 'v0.3.21'
expect_reject deb '0.3.21; rm -rf /'
expect_reject archlinux '0.3.21-1'
expect_reject unknownfmt 0.3.21

((failures == 0)) || {
  echo "$failures package version check(s) failed" >&2
  exit 1
}
echo "Package version mapping verified"
