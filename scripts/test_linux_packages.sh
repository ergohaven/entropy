#!/usr/bin/env bash
# Проверяет, что собранные пакеты действительно несут то, ради чего собирались:
# бинарник, .desktop, AppStream-метаданные, udev-правило и иконки hicolor.
# Ошибки такого рода иначе всплывают только после установки на живой системе.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST="${1:-$ROOT/dist/linux}"

if ! command -v bsdtar >/dev/null 2>&1; then
  echo "need bsdtar to read deb/rpm/pkg.tar.zst (libarchive-tools on Debian/Ubuntu, bsdtar on openSUSE/Fedora, libarchive on Arch)" >&2
  exit 1
fi

# Пути внутри пакета: rpm печатает их с ведущим слэшем, deb и arch — с './'.
EXPECTED=(
  usr/bin/entropy
  usr/share/applications/entropy.desktop
  usr/share/metainfo/com.ergohaven.entropy.metainfo.xml
  usr/lib/udev/rules.d/59-vial.rules
  usr/share/icons/hicolor/16x16/apps/entropy.png
  usr/share/icons/hicolor/256x256/apps/entropy.png
  usr/share/doc/ergohaven-entropy/LICENSE
)

failures=0
fail() {
  echo "FAIL: $*" >&2
  failures=$((failures + 1))
}

find_one() { # glob -> единственный файл или пусто
  local matches=()
  # shellcheck disable=SC2206
  matches=($1)
  [[ ${#matches[@]} -eq 1 && -f ${matches[0]} ]] || return 1
  printf '%s\n' "${matches[0]}"
}

list_payload() { # package -> пути внутри пакета
  # .deb — ar-архив, и bsdtar показывает у него только data.tar/control.tar,
  # поэтому полезная нагрузка распаковывается вторым проходом. rpm и pkg.tar.zst
  # libarchive читает напрямую.
  case "$1" in
  *.deb) bsdtar -xOf "$1" 'data.tar*' | bsdtar -tf - ;;
  *) bsdtar -tf "$1" ;;
  esac
}

check_contents() { # label package
  local label="$1" pkg="$2" listing entry before="$failures"
  listing="$(list_payload "$pkg" | sed 's|^\./||; s|^/||')"
  for entry in "${EXPECTED[@]}"; do
    grep -qxF "$entry" <<<"$listing" || fail "$label: missing $entry"
  done
  if ((failures == before)); then
    echo "ok: $label carries every expected path"
  fi
}

for spec in "deb:$DIST/*.deb" "rpm:$DIST/*.rpm" "arch:$DIST/*.pkg.tar.zst"; do
  label="${spec%%:*}"
  if pkg="$(find_one "${spec#*:}")"; then
    check_contents "$label ($(basename "$pkg"))" "$pkg"
  else
    fail "$label: expected exactly one package in $DIST"
  fi
done

if appimage="$(find_one "$DIST/*.AppImage")"; then
  if [[ -x $appimage ]]; then
    echo "ok: AppImage ($(basename "$appimage")) is executable"
  else
    fail "AppImage is not executable"
  fi
else
  fail "AppImage: expected exactly one image in $DIST"
fi

((failures == 0)) || {
  echo "$failures package check(s) failed" >&2
  exit 1
}
echo "Linux package contents verified"
