#!/usr/bin/env bash

# Версии и контрольные суммы стороннего тулчейна, который не пакетируется
# дистрибутивами и потому скачивается. Пишется как POSIX sh, чтобы файл читался
# через `.` из любого шелла.
#
# Обновление: поднять версию и заменить суммы значениями из checksums.txt на
# релизной странице проекта. Пин appimagetool живёт отдельно в
# appimagetool_pin.sh — его двигает update_appimagetool_pin.sh.

NFPM_VERSION="${NFPM_VERSION:-2.47.0}"

# tool_sha256 <tool>:<flavour> — печатает ожидаемую сумму или падает, если
# комбинации нет: неизвестная платформа не должна означать «скачать без проверки».
tool_sha256() {
	case "$1" in
	nfpm-tar:Linux_x86_64) echo 0660ca602b2d2d2ae4781a06c692b3eeb9d437ffea05b831d76e41f4a3188783 ;;
	nfpm-tar:Linux_arm64) echo 1c0f5f2999b9a974bfb04fdb0cc3306096de530ac5dbb25d739cc5f5219c919c ;;
	*)
		echo "no pinned SHA-256 for '$1' — add it to scripts/tool_pins.sh" >&2
		return 1
		;;
	esac
}
