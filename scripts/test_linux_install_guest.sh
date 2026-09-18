#!/usr/bin/env bash
# Гостевая половина scripts/test_linux_install.sh: выполняется внутри чистого
# образа дистрибутива. Ставит собранный пакет штатным пакетным менеджером,
# запускает приложение и сверяет порядок версий родным компаратором.
#
# Собранные пакеты ожидаются в /pkg, версии приходят окружением: PKG_EXPECTED —
# та, что должна оказаться в базе пакетного менеджера, PKG_RC/PKG_RC2/PKG_STABLE
# — тройка для проверки порядка. Все считает хост через scripts/pkg_version.sh.
set -euo pipefail

PKG_DIR="${PKG_DIR:-/pkg}"
NAME=ergohaven-entropy

failures=0
fail() {
	echo "FAIL: $*" >&2
	failures=$((failures + 1))
}

# Установка молчит, пока идёт хорошо: иначе вывод пакетного менеджера топит
# результат теста. Но при падении вывод нужен целиком — это и есть диагноз.
run_quiet() {
	local out
	if ! out="$("$@" 2>&1)"; then
		printf '%s\n' "$out" >&2
		echo "command failed: $*" >&2
		exit 1
	fi
}

family() {
	local id="" like=""
	# shellcheck disable=SC1091
	id="$(. /etc/os-release && echo "${ID:-}")"
	# shellcheck disable=SC1091
	like="$(. /etc/os-release && echo "${ID_LIKE:-}")"
	case " $id $like " in
	*" debian "* | *" ubuntu "*) echo debian ;;
	*" fedora "* | *" rhel "*) echo fedora ;;
	*" arch "*) echo arch ;;
	*) echo "unknown:${id:-none}" ;;
	esac
}

FAMILY="$(family)"
echo "== $FAMILY: clean install"

case "$FAMILY" in
debian)
	export DEBIAN_FRONTEND=noninteractive
	run_quiet apt-get update -qq
	run_quiet apt-get install -y -qq "$PKG_DIR"/*.deb
	installed="$(dpkg-query -W -f='${Version}' "$NAME")"
	;;
fedora)
	run_quiet dnf install -y -q rpmdevtools
	run_quiet dnf install -y -q "$PKG_DIR"/*.rpm
	installed="$(rpm -q --qf '%{VERSION}' "$NAME")"
	;;
arch)
	run_quiet pacman -Sy --noconfirm
	run_quiet pacman -U --noconfirm "$PKG_DIR"/*.pkg.tar.zst
	installed="$(pacman -Q "$NAME" | awk '{ print $2 }')"
	installed="${installed%-*}"
	;;
*)
	echo "unsupported image: $FAMILY" >&2
	exit 1
	;;
esac

# Версия, приведённая к формату, должна доехать до базы пакетного менеджера, а
# не только до имени файла: именно её сравнивает апгрейд.
[[ $installed == "$PKG_EXPECTED" ]] || fail "installed version is '$installed', want '$PKG_EXPECTED'"

# /usr/share/doc здесь не проверяется: и debian-slim (path-exclude в
# /etc/dpkg/dpkg.cfg.d), и официальный образ archlinux (NoExtract) выбрасывают
# документацию при распаковке. Наличие LICENSE в самом пакете проверяет
# scripts/test_linux_packages.sh.
echo "== $FAMILY: payload"
for path in \
	/usr/bin/entropy \
	/usr/share/applications/entropy.desktop \
	/usr/share/metainfo/com.ergohaven.entropy.metainfo.xml \
	/usr/lib/udev/rules.d/59-vial.rules \
	/usr/share/icons/hicolor/16x16/apps/entropy.png \
	/usr/share/icons/hicolor/256x256/apps/entropy.png; do
	[[ -e $path ]] || fail "missing $path"
done
[[ "$(stat -c '%a' /usr/bin/entropy)" == 755 ]] || fail "/usr/bin/entropy mode is $(stat -c '%a' /usr/bin/entropy), want 755"
[[ "$(stat -c '%a' /usr/lib/udev/rules.d/59-vial.rules)" == 644 ]] ||
	fail "udev rule mode is $(stat -c '%a' /usr/lib/udev/rules.d/59-vial.rules), want 644"
grep -qxF '# Entropy Vial hidraw access v2' /usr/lib/udev/rules.d/59-vial.rules ||
	fail "installed udev rule lacks the version marker"

echo "== $FAMILY: runtime"
if ldd /usr/bin/entropy | grep -q 'not found'; then
	fail "unresolved ELF dependencies: $(ldd /usr/bin/entropy | grep 'not found' | tr '\n' ' ')"
fi
# Без дисплея приложение обязано дойти до собственного стартового лога: падение
# раньше означает, что объявленных зависимостей не хватило.
output="$(env -u DISPLAY -u WAYLAND_DISPLAY timeout 60 /usr/bin/entropy 2>&1 || true)"
grep -q 'starting on linux' <<<"$output" || fail "did not reach startup: ${output:0:400}"
! grep -q 'cannot open shared object file' <<<"$output" ||
	fail "a shared library is missing at runtime: ${output:0:400}"

echo "== $FAMILY: version ordering"
lt() { # a b — a должна быть строго старее b
	case "$FAMILY" in
	debian) dpkg --compare-versions "$1" lt "$2" ;;
	fedora)
		local status=0
		rpmdev-vercmp "$1" "$2" >/dev/null 2>&1 || status=$?
		[[ $status -eq 12 ]]
		;;
	arch) [[ "$(vercmp "$1" "$2")" -lt 0 ]] ;;
	esac
}
lt "$PKG_RC" "$PKG_STABLE" || fail "$PKG_RC is not older than $PKG_STABLE — upgrade from rc to stable would never happen"
lt "$PKG_RC" "$PKG_RC2" || fail "$PKG_RC is not older than $PKG_RC2"

echo "== $FAMILY: removal"
case "$FAMILY" in
debian) run_quiet apt-get remove -y -qq "$NAME" ;;
fedora) run_quiet dnf remove -y -q "$NAME" ;;
arch) run_quiet pacman -R --noconfirm "$NAME" ;;
esac
[[ ! -e /usr/bin/entropy ]] || fail "/usr/bin/entropy survived the removal"

((failures == 0)) || {
	echo "$failures check(s) failed on $FAMILY" >&2
	exit 1
}
echo "OK: $FAMILY"
