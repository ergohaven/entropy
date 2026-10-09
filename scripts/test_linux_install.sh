#!/usr/bin/env bash
# Ставит собранные пакеты в чистые образы дистрибутивов их штатными пакетными
# менеджерами. Распаковка проверяет только пути, а установка — что руками
# выписанные зависимости существуют, разрешаются и что приложение с ними
# запускается; порядок версий проверяется тем же компаратором, каким его будет
# сравнивать апгрейд у пользователя.
#
# Образы взяты пинами по дайджесту: «последняя fedora» меняется, и падение
# такого теста иначе не отличить от нашей же регрессии.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST="${1:-$ROOT/dist/linux}"
VERSION="${VERSION:-$(awk -F '"' '/^version = / { print $2; exit }' "$ROOT/Cargo.toml")}"
BASE="${VERSION%%-*}"

command -v docker >/dev/null 2>&1 || {
	echo "docker is required for the clean-install test" >&2
	exit 1
}

DIST="$(cd "$DIST" && pwd)"

# Тот же обход SELinux, что в Taskfile: без него контейнер не видит
# смонтированный каталог и тест «падает» на пустом /pkg.
SELINUX=()
if command -v getenforce >/dev/null 2>&1 && [[ "$(getenforce)" != Disabled ]]; then
	SELINUX=(--security-opt label=disable)
fi

IMAGES=(
	"deb|debian@sha256:88200866dfff7ea7f5cbcb6ec7c8a701889efe6fe859fe64d6990e4b07ea4171"
	"rpm|fedora@sha256:43b29f65a41eb9c35e1cd5323e3bdf3b655c2357a9f4f1ff2f9c2798e5045d80"
	"archlinux|archlinux@sha256:82b1b08faae9d61e3e7e13d562f4d09114d939105b0d59ff34140f3bd418593a"
)

failures=0
for spec in "${IMAGES[@]}"; do
	format="${spec%%|*}"
	image="${spec#*|}"
	echo "--- $format in $image"
	if ! docker run --rm "${SELINUX[@]}" \
		-v "$DIST":/pkg:ro \
		-v "$ROOT/scripts/test_linux_install_guest.sh":/guest.sh:ro \
		-e PKG_EXPECTED="$("$ROOT/scripts/pkg_version.sh" "$format" "$VERSION")" \
		-e PKG_RC="$("$ROOT/scripts/pkg_version.sh" "$format" "$BASE-rc.1")" \
		-e PKG_RC2="$("$ROOT/scripts/pkg_version.sh" "$format" "$BASE-rc.2")" \
		-e PKG_STABLE="$("$ROOT/scripts/pkg_version.sh" "$format" "$BASE")" \
		"$image" bash /guest.sh; then
		echo "FAIL: $format install test in $image" >&2
		failures=$((failures + 1))
	fi
done

((failures == 0)) || {
	echo "$failures clean-install test(s) failed" >&2
	exit 1
}
echo "Clean installs verified on Debian, Fedora and Arch"
