#!/usr/bin/env bash

# Версии и контрольные суммы стороннего тулчейна — один источник и для
# Dockerfile, и для scripts/prepare_env.sh. Пишется как POSIX sh: в образе файл
# читается через `.` из RUN, то есть из dash.
#
# Обновление: поднять версию и заменить суммы значениями с релизной страницы
# проекта (checksums.txt у nfpm/task/quill/lipo, ziglang.org/download/index.json
# у zig). Пин appimagetool живёт отдельно в appimagetool_pin.sh — его двигает
# update_appimagetool_pin.sh, и в образе он лежит в последнем слое.

ZIG_VERSION="${ZIG_VERSION:-0.16.0}"
NFPM_VERSION="${NFPM_VERSION:-2.47.0}"
TASK_VERSION="${TASK_VERSION:-3.44.0}"
QUILL_VERSION="${QUILL_VERSION:-0.7.1}"
LIPO_VERSION="${LIPO_VERSION:-0.9.4}"
# Каждая линия cargo-zigbuild рассчитана на свою версию zig, поэтому ~0.23 и
# ZIG_VERSION меняются только вместе — и должны совпадать с .tool-versions,
# иначе локальная и контейнерная сборки разъезжаются по тулчейну.
CARGO_ZIGBUILD_VERSION="${CARGO_ZIGBUILD_VERSION:-~0.23}"
# Ревизия, а не HEAD: исходники компилируются и запускаются, и «последний
# коммит» тут ничего не гарантирует. Это дефолтная ветка репозитория
# (only_what_core_needs) — на master остались filevault/AES, которые не
# собираются с OpenSSL 3.
LIBDMG_REPO="${LIBDMG_REPO:-https://github.com/fanquake/libdmg-hfsplus}"
LIBDMG_REV="${LIBDMG_REV:-1cc791e4173da9cb0b0cc16c5a1aaa25d5eb5efa}"

# tool_sha256 <tool>:<flavour> — печатает ожидаемую сумму или падает, если
# комбинации нет: неизвестная платформа не должна означать «скачать без проверки».
tool_sha256() {
	case "$1" in
	zig:x86_64) echo 70e49664a74374b48b51e6f3fdfbf437f6395d42509050588bd49abe52ba3d00 ;;
	zig:aarch64) echo ea4b09bfb22ec6f6c6ceac57ab63efb6b46e17ab08d21f69f3a48b38e1534f17 ;;
	nfpm-deb:amd64) echo 3f1cf344bd0b57373ca55636a78c08b0491f7293d609a456a9ac3b0b150fda97 ;;
	nfpm-deb:arm64) echo 27419eb382695a7942be8ad52259f3ec1854fad001b3ae4baed34ce39a223b97 ;;
	nfpm-tar:Linux_x86_64) echo 0660ca602b2d2d2ae4781a06c692b3eeb9d437ffea05b831d76e41f4a3188783 ;;
	nfpm-tar:Linux_arm64) echo 1c0f5f2999b9a974bfb04fdb0cc3306096de530ac5dbb25d739cc5f5219c919c ;;
	nfpm-tar:Darwin_x86_64) echo 2b04108f8757313dde92ed729560845aadfb7782887eb6988a5dd96f9c146861 ;;
	nfpm-tar:Darwin_arm64) echo e8c9d1d9ac218eeed479375143dc46b8d51a2b8dbba8e2f9f15ecc8faa2e404b ;;
	task-deb:amd64) echo cdd55b9908d3ef0889bb2270132f7bdb90e50d85b645c57434385cb8ea80cc42 ;;
	task-deb:arm64) echo 13d82f9194b3d2f9b601a29501e53adc5c7f151e181b93d6858404f35da0295f ;;
	quill:amd64) echo e58c6f86378a22507c1123e24412afd4ee2d3bb32ebd94d6059827dc0c1b3fbf ;;
	quill:arm64) echo b3d5bbc006f1aa0387e06349db1a988597ce348913e6e8d38d4bfb34cc93e78d ;;
	lipo:amd64) echo fe9ac9226dd9a0fa4fd4779092c731f25120b50d0a6202f1fe6ab883e1f1085b ;;
	lipo:arm64) echo 0e266ad71f0e371a2033232708b57e2f3bf09881d1c90ceee0b42892156fbda8 ;;
	# Сторонняя пересборка Apple SDK: распаковывается и используется для
	# линковки, поэтому без сверки её брать нельзя вовсе.
	macos-sdk:15.5) echo c15cf0f3f17d714d1aa5a642da8e118db53d79429eb015771ba816aa7c6c1cbd ;;
	*)
		echo "no pinned SHA-256 for '$1' — add it to scripts/tool_pins.sh" >&2
		return 1
		;;
	esac
}
