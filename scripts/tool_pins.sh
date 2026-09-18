#!/usr/bin/env bash

# Версии и контрольные суммы стороннего тулчейна, который не пакетируется
# дистрибутивами и потому скачивается. Пишется как POSIX sh, чтобы файл читался
# через `.` из любого шелла.
#
# Обновление: поднять версию и заменить суммы значениями из checksums.txt на
# релизной странице проекта. Пин appimagetool живёт отдельно в
# appimagetool_pin.sh — его двигает update_appimagetool_pin.sh.

NFPM_VERSION="${NFPM_VERSION:-2.47.0}"
TASK_VERSION="${TASK_VERSION:-3.44.0}"

# tool_sha256 <tool>:<flavour> — печатает ожидаемую сумму или падает, если
# комбинации нет: неизвестная платформа не должна означать «скачать без проверки».
# Форматы разные, потому что разные потребители: локальной подготовке нужен
# tar.gz в кэш проекта, образу — .deb, который ставится системно.
tool_sha256() {
	case "$1" in
	nfpm-tar:Linux_x86_64) echo 0660ca602b2d2d2ae4781a06c692b3eeb9d437ffea05b831d76e41f4a3188783 ;;
	nfpm-tar:Linux_arm64) echo 1c0f5f2999b9a974bfb04fdb0cc3306096de530ac5dbb25d739cc5f5219c919c ;;
	nfpm-deb:amd64) echo 3f1cf344bd0b57373ca55636a78c08b0491f7293d609a456a9ac3b0b150fda97 ;;
	nfpm-deb:arm64) echo 27419eb382695a7942be8ad52259f3ec1854fad001b3ae4baed34ce39a223b97 ;;
	task-deb:amd64) echo cdd55b9908d3ef0889bb2270132f7bdb90e50d85b645c57434385cb8ea80cc42 ;;
	task-deb:arm64) echo 13d82f9194b3d2f9b601a29501e53adc5c7f151e181b93d6858404f35da0295f ;;
	*)
		echo "no pinned SHA-256 for '$1' — add it to scripts/tool_pins.sh" >&2
		return 1
		;;
	esac
}
