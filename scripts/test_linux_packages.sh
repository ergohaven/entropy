#!/usr/bin/env bash
# Проверяет, что собранные артефакты несут то, ради чего собирались: бинарник,
# .desktop, AppStream-метаданные, udev-правило и иконки hicolor — с нужными
# правами и версией, приведённой к формату пакета. AppImage распаковывается и
# запускается по-настоящему: исполняемого бита мало, чтобы утверждать, что он
# работает.
# Установка в чистый дистрибутив — отдельно, scripts/test_linux_install.sh.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST="${1:-$ROOT/dist/linux}"
# Абсолютный: проверка AppImage запускает его из своего временного каталога, и
# относительный путь оттуда уже не разрешается.
DIST="$(cd "$DIST" 2>/dev/null && pwd)" || {
  echo "no such directory: ${1:-$ROOT/dist/linux}" >&2
  exit 1
}
VERSION="${VERSION:-$(awk -F '"' '/^version = / { print $2; exit }' "$ROOT/Cargo.toml")}"
HOST_ARCH="$(uname -m)"

if ! command -v bsdtar >/dev/null 2>&1; then
  echo "need bsdtar to read deb/rpm/pkg.tar.zst (libarchive-tools on Debian/Ubuntu, bsdtar on openSUSE/Fedora, libarchive on Arch)" >&2
  exit 1
fi

# Пути внутри пакета: rpm печатает их с ведущим слэшем, deb и arch — с './'.
EXECUTABLE=usr/bin/entropy
READABLE=(
  usr/share/applications/entropy.desktop
  usr/share/metainfo/com.ergohaven.entropy.metainfo.xml
  usr/lib/udev/rules.d/59-vial.rules
  usr/share/icons/hicolor/16x16/apps/entropy.png
  usr/share/icons/hicolor/32x32/apps/entropy.png
  usr/share/icons/hicolor/48x48/apps/entropy.png
  usr/share/icons/hicolor/64x64/apps/entropy.png
  usr/share/icons/hicolor/128x128/apps/entropy.png
  usr/share/icons/hicolor/256x256/apps/entropy.png
  usr/share/doc/ergohaven-entropy/LICENSE
)
# Маркер версии правила: без него приложение считает установленное правило
# устаревшим и продолжает просить настроить доступ (src/ui/app_settings.rs).
UDEV_MARKER='# Entropy Vial hidraw access v2'

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

payload_stream() { # package -> tar-поток с полезной нагрузкой
  # .deb — ar-архив, и bsdtar видит у него только data.tar/control.tar, поэтому
  # полезная нагрузка читается вторым проходом. rpm и pkg.tar.zst libarchive
  # читает напрямую.
  case "$1" in
  *.deb) bsdtar -xOf "$1" 'data.tar*' ;;
  *) cat "$1" ;;
  esac
}

# "-rw-r--r-- путь" по одной строке на запись; ведущие ./ и / срезаны, чтобы
# сравнивать пути одинаково во всех трёх форматах.
payload_modes() {
  payload_stream "$1" | LC_ALL=C bsdtar -tvf - | awk '{ print $1, $NF }' | sed 's| \./| |; s| /| |'
}

mode_of() { # listing path -> строка режима или пусто
  awk -v want="$2" '$2 == want { print $1; found = 1 } END { exit !found }' <<<"$1"
}

check_contents() { # label package
  local label="$1" pkg="$2" listing entry mode before="$failures"
  listing="$(payload_modes "$pkg")"

  if mode="$(mode_of "$listing" "$EXECUTABLE")"; then
    [[ $mode == -rwxr-xr-x ]] || fail "$label: $EXECUTABLE has mode $mode, want -rwxr-xr-x"
  else
    fail "$label: missing $EXECUTABLE"
  fi

  for entry in "${READABLE[@]}"; do
    if mode="$(mode_of "$listing" "$entry")"; then
      # 0664 приезжает из umask сборщика, если режим не задан в nfpm.yaml явно.
      [[ $mode == -rw-r--r-- ]] || fail "$label: $entry has mode $mode, want -rw-r--r--"
    else
      fail "$label: missing $entry"
    fi
  done

  local rule
  rule="$(payload_stream "$pkg" | bsdtar -xOf - '*59-vial.rules' 2>/dev/null || true)"
  grep -qxF "$UDEV_MARKER" <<<"$rule" || fail "$label: udev rule lacks the '$UDEV_MARKER' marker"

  ((failures != before)) || echo "ok: $label carries every expected path with the expected modes"
}

check_version() { # label package expected
  local label="$1" pkg="$2" want="$3" got=""
  case "$pkg" in
  *.deb) got="$(bsdtar -xOf "$pkg" 'control.tar*' | bsdtar -xOf - ./control | awk '/^Version:/ { print $2 }')" ;;
  *.pkg.tar.zst) got="$(bsdtar -xOf "$pkg" .PKGINFO | awk '$1 == "pkgver" { sub(/-[0-9]+$/, "", $3); print $3 }')" ;;
  # У rpm заголовок libarchive не отдаёт; настоящая проверка — rpm -qp
  # в scripts/test_linux_install.sh, здесь достаточно имени файла.
  *.rpm) got="$(basename "$pkg")"; got="${got#ergohaven-entropy-}"; got="${got%-1.*.rpm}" ;;
  esac
  [[ $got == "$want" ]] || fail "$label: version is '$got', want '$want'"
}

for spec in "deb:$DIST/*.deb" "rpm:$DIST/*.rpm" "archlinux:$DIST/*.pkg.tar.zst"; do
  format="${spec%%:*}"
  if pkg="$(find_one "${spec#*:}")"; then
    label="$format ($(basename "$pkg"))"
    check_contents "$label" "$pkg"
    check_version "$label" "$pkg" "$("$ROOT/scripts/pkg_version.sh" "$format" "$VERSION")"
  else
    fail "$format: expected exactly one package in $DIST"
  fi
done

check_appimage() { # appimage
  local image="$1" work before="$failures"
  work="$(mktemp -d)"
  # shellcheck disable=SC2064
  trap "rm -rf '$work'" RETURN

  [[ -x $image ]] || { fail "AppImage is not executable"; return; }
  if ! (cd "$work" && "$image" --appimage-extract >/dev/null 2>&1); then
    fail "AppImage: --appimage-extract failed"
    return
  fi

  local root="$work/squashfs-root" entry
  for entry in \
    AppRun \
    usr/bin/entropy \
    entropy.desktop \
    entropy.png \
    .DirIcon \
    usr/share/applications/entropy.desktop \
    usr/share/metainfo/com.ergohaven.entropy.metainfo.xml \
    usr/share/icons/hicolor/16x16/apps/entropy.png \
    usr/share/icons/hicolor/256x256/apps/entropy.png; do
    [[ -e "$root/$entry" ]] || fail "AppImage: missing $entry"
  done
  [[ -x "$root/usr/bin/entropy" ]] || fail "AppImage: usr/bin/entropy is not executable"

  # Настоящий запуск: без дисплея приложение обязано дойти до своего стартового
  # лога и упасть на дисплее, а не на ненайденной библиотеке.
  local output
  output="$(cd "$root" && env -u DISPLAY -u WAYLAND_DISPLAY timeout 60 ./AppRun 2>&1 || true)"
  grep -q 'starting on linux' <<<"$output" || fail "AppImage: did not reach startup, output: ${output:0:400}"
  ! grep -q 'cannot open shared object file' <<<"$output" ||
    fail "AppImage: a shared library is missing at runtime: ${output:0:400}"

  ((failures != before)) || echo "ok: AppImage ($(basename "$image")) extracts, carries its payload and starts"
}

if appimage="$(find_one "$DIST/*.AppImage")"; then
  check_appimage "$appimage"
elif [[ $HOST_ARCH == x86_64 ]]; then
  fail "AppImage: expected exactly one image in $DIST"
else
  echo "skip: AppImage is x86_64-only, this host is $HOST_ARCH"
fi

((failures == 0)) || {
  echo "$failures package check(s) failed" >&2
  exit 1
}
echo "Linux artifacts verified"
