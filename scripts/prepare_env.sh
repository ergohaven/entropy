#!/usr/bin/env bash
# Installs everything the Taskfile targets need on this machine: detects the OS
# and (on Linux) the distro family, then installs the build and packaging
# prerequisites with the native package manager.
#
#   --cross     also install the Windows/macOS cross-build toolchain
#   --dry-run   print the commands instead of running them
#   --no-strict exit 0 even if some tools are still missing
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# Версии и суммы общие с Dockerfile: разъехавшиеся тулчейны дают разные пакеты
# у разработчика и в релизе.
# shellcheck disable=SC1091
source scripts/tool_pins.sh
# Скачиваемые инструменты кладём в кэш проекта, а не в систему: их подхватывают
# Taskfile и скрипты сборки, дополняя PATH только на время запуска.
TOOLS_DIR="$ROOT/.cache/tools"
WITH_CROSS=0
DRY_RUN=0
STRICT=1

log() { printf '\033[1;36m==>\033[0m %s\n' "$*" >&2; }
warn() { printf '\033[1;33m!!\033[0m %s\n' "$*" >&2; }

usage() {
	sed -n '2,8p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
	exit "${1:-0}"
}

while [[ $# -gt 0 ]]; do
	case "$1" in
	--cross) WITH_CROSS=1 ;;
	--dry-run | -n) DRY_RUN=1 ;;
	--no-strict) STRICT=0 ;;
	-h | --help) usage 0 ;;
	*)
		warn "unknown option: $1"
		usage 1
		;;
	esac
	shift
done

run() {
	if ((DRY_RUN)); then
		printf '  %s\n' "$*"
		return 0
	fi
	"$@"
}

# sudo only when it is actually needed: root and rootless macOS installs skip it.
SUDO=()
if [[ "$(id -u)" -ne 0 ]] && command -v sudo >/dev/null 2>&1; then
	SUDO=(sudo)
fi

have() { command -v "$1" >/dev/null 2>&1; }

# Загружаемые инструменты ищем и в кэше проекта: он не в PATH, но сборка берёт
# их именно оттуда, так что повторно качать не нужно.
have_tool() { have "$1" || [[ -x "$TOOLS_DIR/$1" ]]; }

# Сумма сверяется до распаковки, поэтому качаем в файл, а не в pipe. Таймауты
# обязательны: без --max-time оборванное соединение висит без ограничения.
fetch_verified() { # url pin-key destination
	local url="$1" pin="$2" dest="$3" sha
	sha="$(tool_sha256 "$pin")"
	rm -f "$dest"
	curl -fsSL --retry 3 --retry-all-errors --connect-timeout 15 --max-time 300 "$url" -o "$dest"
	"$ROOT/scripts/verify_sha256.sh" "$dest" "$sha"
}

detect_linux_family() {
	local id="" like=""
	if [[ -r /etc/os-release ]]; then
		# shellcheck disable=SC1091
		id="$(. /etc/os-release && echo "${ID:-}")"
		like="$(. /etc/os-release && echo "${ID_LIKE:-}")"
	fi
	case " $id $like " in
	*" debian "* | *" ubuntu "*) echo debian ;;
	*" suse "* | *" opensuse "* | *" sles "*) echo suse ;;
	*" fedora "* | *" rhel "* | *" centos "*) echo fedora ;;
	*" arch "*) echo arch ;;
	*" alpine "*) echo alpine ;;
	*) echo "unknown:${id:-none}" ;;
	esac
}

# Package sets per distro family. Build = what `cargo build` needs, pack = what
# the packaging targets shell out to, cross = the Windows/macOS cross toolchain.
# zlib в rpm-семействах просим как pkgconfig(zlib), а не пакетом zlib-devel:
# в Tumbleweed и Fedora по умолчанию стоит zlib-ng-compat-devel, и запрос по
# имени тянет классический zlib-devel, который с ним конфликтует.
packages_for() {
	local family="$1" group="$2"
	case "$family:$group" in
	debian:build) echo "build-essential pkg-config libhidapi-dev libudev-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev libssl-dev libgtk-3-dev" ;;
	debian:pack) echo "imagemagick librsvg2-bin gettext-base wixl icnsutils desktop-file-utils genisoimage" ;;
	debian:cross) echo "mingw-w64 cmake git zlib1g-dev" ;;

	suse:build) echo "gcc gcc-c++ pkgconf-pkg-config libhidapi-devel systemd-devel libxcb-devel libxkbcommon-devel libopenssl-devel gtk3-devel" ;;
	suse:pack) echo "ImageMagick rsvg-convert envsubst msitools icns-utils desktop-file-utils mkisofs" ;;
	suse:cross) echo "mingw64-cross-gcc cmake git pkgconfig(zlib)" ;;

	fedora:build) echo "gcc gcc-c++ pkgconf-pkg-config hidapi-devel systemd-devel libxcb-devel libxkbcommon-devel openssl-devel gtk3-devel" ;;
	fedora:pack) echo "ImageMagick librsvg2-tools gettext-envsubst msitools libicns-utils desktop-file-utils genisoimage" ;;
	fedora:cross) echo "mingw64-gcc cmake git pkgconfig(zlib)" ;;

	arch:build) echo "base-devel pkgconf hidapi systemd-libs libxcb libxkbcommon openssl gtk3" ;;
	arch:pack) echo "imagemagick librsvg gettext msitools desktop-file-utils libisoburn" ;;
	arch:cross) echo "mingw-w64-gcc cmake git zlib" ;;

	alpine:build) echo "build-base pkgconf hidapi-dev eudev-dev libxcb-dev libxkbcommon-dev openssl-dev gtk+3.0-dev" ;;
	alpine:pack) echo "imagemagick librsvg gettext desktop-file-utils cdrkit" ;;
	alpine:cross) echo "cmake git zlib-dev" ;;

	*) echo "" ;;
	esac
}

install_linux_packages() {
	local family="$1"
	shift
	local pkgs=("$@")
	((${#pkgs[@]})) || return 0

	case "$family" in
	debian)
		run "${SUDO[@]}" apt-get update
		run "${SUDO[@]}" apt-get install -y --no-install-recommends "${pkgs[@]}"
		;;
	suse) run "${SUDO[@]}" zypper --non-interactive install --no-recommends "${pkgs[@]}" ;;
	fedora) run "${SUDO[@]}" dnf install -y "${pkgs[@]}" ;;
	arch) run "${SUDO[@]}" pacman -S --needed --noconfirm "${pkgs[@]}" ;;
	alpine) run "${SUDO[@]}" apk add "${pkgs[@]}" ;;
	esac
}

# Rust и zig берём из .tool-versions через asdf: версии тогда одинаковы у всех
# и ничего не ставится в систему глобально. Без asdf откатываемся на rustup и
# пакет zig из дистрибутива.
install_asdf_tools() {
	have asdf || return 1

	local tool installed
	installed="$(asdf plugin list 2>/dev/null || true)"
	for tool in rust zig; do
		grep -qx "$tool" <<<"$installed" || run asdf plugin add "$tool"
		log "Installing $tool from .tool-versions"
		run asdf install "$tool"
	done
}

ensure_zig() {
	have asdf && return 0

	local family="$1"
	warn "asdf not found — installing zig from the distro instead of .tool-versions"
	install_linux_packages "$family" zig
}

install_nfpm() {
	have_tool nfpm && return 0

	local arch
	case "$(uname -m)" in
	x86_64) arch=x86_64 ;;
	aarch64 | arm64) arch=arm64 ;;
	*)
		warn "no nfpm release for $(uname -m); install it manually: https://github.com/goreleaser/nfpm/releases"
		return 0
		;;
	esac

	local os url
	os="$([[ "$(uname -s)" == Darwin ]] && echo Darwin || echo Linux)"
	url="https://github.com/goreleaser/nfpm/releases/download/v${NFPM_VERSION}/nfpm_${NFPM_VERSION}_${os}_${arch}.tar.gz"

	log "Installing nfpm ${NFPM_VERSION} into .cache/tools"
	run mkdir -p "$TOOLS_DIR"
	if ((DRY_RUN)); then
		printf '  curl -fsSL %s -o .cache/tools/nfpm.tar.gz (sha256-verified) && tar -xz nfpm\n' "$url"
		return 0
	fi
	local archive="$TOOLS_DIR/nfpm.tar.gz"
	fetch_verified "$url" "nfpm-tar:${os}_${arch}" "$archive"
	tar -xzf "$archive" -C "$TOOLS_DIR" nfpm
	rm -f "$archive"
	chmod 0755 "$TOOLS_DIR/nfpm"
}

install_quill() {
	have_tool quill && return 0

	local arch
	case "$(uname -m)" in
	x86_64) arch=amd64 ;;
	aarch64 | arm64) arch=arm64 ;;
	*)
		warn "no quill release for $(uname -m); the macOS bundle will ship unsigned"
		return 0
		;;
	esac

	local url="https://github.com/anchore/quill/releases/download/v${QUILL_VERSION}/quill_${QUILL_VERSION}_linux_${arch}.tar.gz"
	log "Installing quill ${QUILL_VERSION} into .cache/tools (ad-hoc Mach-O signing from Linux)"
	run mkdir -p "$TOOLS_DIR"
	if ((DRY_RUN)); then
		printf '  curl -fsSL %s -o .cache/tools/quill.tar.gz (sha256-verified) && tar -xz quill\n' "$url"
		return 0
	fi
	local archive="$TOOLS_DIR/quill.tar.gz"
	fetch_verified "$url" "quill:${arch}" "$archive"
	tar -xzf "$archive" -C "$TOOLS_DIR" quill
	rm -f "$archive"
	chmod 0755 "$TOOLS_DIR/quill"
}

# libdmg-hfsplus не пакетируется ни одним дистрибутивом, поэтому собираем его
# из исходников — тем же способом, что и образ в Dockerfile. Исполняемый файл
# лежит в CMake-таргете dmg-bin: таргет dmg — это статическая библиотека.
install_libdmg() {
	have_tool dmg && return 0

	if ! have cmake || ! have git; then
		warn "cmake and git are needed to build libdmg-hfsplus; skipping the .dmg tool"
		return 0
	fi

	log "Building libdmg-hfsplus ${LIBDMG_REV:0:12} into .cache/tools (no distro package exists)"
	if ((DRY_RUN)); then
		printf '  git fetch %s %s && cmake && make dmg-bin -> .cache/tools/dmg\n' "$LIBDMG_REPO" "$LIBDMG_REV"
		return 0
	fi

	# Пиннутая ревизия, а не HEAD: исходники компилируются и запускаются, и в
	# образе должен оказаться ровно тот же dmg, что у разработчика.
	local src
	src="$(mktemp -d)"
	git init -q "$src"
	git -C "$src" fetch --depth 1 -q "$LIBDMG_REPO" "$LIBDMG_REV"
	git -C "$src" checkout -q FETCH_HEAD
	cmake -S "$src" -B "$src/build" -DCMAKE_BUILD_TYPE=Release >/dev/null
	make -C "$src/build/dmg" dmg-bin >/dev/null
	mkdir -p "$TOOLS_DIR"
	install -m 0755 "$src/build/dmg/dmg" "$TOOLS_DIR/dmg"
	rm -rf "$src"
}

install_cross_tooling() {
	if have rustup; then
		log "Adding Rust cross targets"
		run rustup target add x86_64-pc-windows-gnu
		[[ "$(uname -s)" == Darwin ]] || run rustup target add x86_64-apple-darwin aarch64-apple-darwin
	else
		warn "rustup not found; add the cross targets manually"
	fi

	# Проверяем и субкомандой: cargo ставит cargo-zigbuild рядом с собой, а при
	# rustup/asdf этот каталог в PATH не попадает — только shim'ы.
	if have cargo-zigbuild || cargo zigbuild --help >/dev/null 2>&1; then
		log "cargo-zigbuild already installed"
	else
		log "Installing cargo-zigbuild (this compiles from source and takes a few minutes)"
		run cargo install cargo-zigbuild --version "$CARGO_ZIGBUILD_VERSION" --locked
	fi

	# Версия проверяется, а не только наличие: shim от asdf/mise без выбранной
	# версии вместо номера печатает подсказку, и cargo-zigbuild падает на semver.
	if ! have zig; then
		warn "zig not found — cargo-zigbuild needs it (package 'zig', asdf, or https://ziglang.org/download/)"
	elif ! zig version 2>/dev/null | grep -qE '^[0-9]+\.[0-9]+'; then
		warn "'$(command -v zig)' does not report a version — looks like an asdf/mise shim with no version selected"
		warn "the Taskfile falls back to /usr/bin/zig; to fix the shim run: asdf set zig <version>"
	fi

	install_quill
	install_libdmg
}

# Набор пакетов покрывает не все дистрибутивы одинаково (в alpine, например, нет
# msitools и icnsutils), поэтому после установки проверяем не имена пакетов, а
# сами инструменты: иначе нехватка всплывёт только на середине сборки.
report_missing_tools() {
	local -a missing=()
	have rsvg-convert || have inkscape || missing+=("rsvg-convert/inkscape (icons)")
	have magick || have convert || missing+=("ImageMagick (.ico)")
	have envsubst || missing+=("envsubst (deb/rpm/arch)")
	have_tool nfpm || missing+=("nfpm (deb/rpm/arch)")
	if [[ "$(uname -s)" == Darwin ]]; then
		have iconutil || missing+=("iconutil (.icns)")
	else
		have png2icns || missing+=("png2icns (.icns)")
	fi
	((WITH_CROSS)) && {
		have wixl || missing+=("wixl (Windows MSI)")
		have genisoimage || have mkisofs || missing+=("genisoimage/mkisofs (.dmg)")
		have_tool dmg || missing+=("libdmg-hfsplus (.dmg)")
	}

	((${#missing[@]})) || return 0
	warn "not installed on this system — the matching targets will fail:"
	printf '     - %s\n' "${missing[@]}" >&2
	warn "see BUILD.md > Prerequisites by distro for the package names"
	return 1
}

prepare_linux() {
	local family
	family="$(detect_linux_family)"
	if [[ "$family" == unknown:* ]]; then
		warn "unsupported distro (${family#unknown:}); install the tools listed in README > Packaging by hand"
		return 1
	fi
	log "Linux, $family family"

	local -a pkgs=()
	read -r -a pkgs <<<"$(packages_for "$family" build) $(packages_for "$family" pack)"
	if ((WITH_CROSS)); then
		local -a cross=()
		read -r -a cross <<<"$(packages_for "$family" cross)"
		pkgs+=("${cross[@]}")
	fi

	log "Installing: ${pkgs[*]}"
	install_linux_packages "$family" "${pkgs[@]}"
	install_asdf_tools || warn "asdf not found — install Rust with rustup: https://rustup.rs"
	install_nfpm
	if ((WITH_CROSS)); then
		ensure_zig "$family"
		install_cross_tooling
	fi
	return 0
}

prepare_darwin() {
	log "macOS"
	if ! xcode-select -p >/dev/null 2>&1; then
		log "Installing the Xcode command line tools (codesign, hdiutil, lipo)"
		run xcode-select --install
	fi

	if have brew; then
		log "Installing packaging tools with Homebrew"
		run brew install librsvg imagemagick libicns gettext
	else
		warn "Homebrew not found; install librsvg, imagemagick and libicns by hand: https://brew.sh"
	fi

	install_asdf_tools || warn "asdf not found — install Rust with rustup: https://rustup.rs"
	install_nfpm
	if have rustup; then
		log "Adding the macOS Rust targets"
		run rustup target add x86_64-apple-darwin aarch64-apple-darwin
	fi
	((WITH_CROSS)) && warn "Linux and Windows artifacts are not cross-built from macOS; use 'task docker:dist'"
	return 0
}

case "$(uname -s)" in
Linux) prepare_linux ;;
Darwin) prepare_darwin ;;
*)
	warn "unsupported OS: $(uname -s)"
	exit 1
	;;
esac

MISSING=0
((DRY_RUN)) || report_missing_tools || MISSING=1
((DRY_RUN)) && log "dry run — nothing was installed"
log "Done. Next: 'task build', 'task package', or 'task docker:dist'."

# Молчаливый успех при недостающих инструментах делает prepare бесполезным как
# гейт: нехватка всплывала бы уже на середине сборки. --no-strict оставлен для
# «поставь что можешь» на дистрибутивах, где части пакетов просто нет.
if ((MISSING)) && ((STRICT)); then
	warn "some tools are still missing; pass --no-strict to ignore"
	exit 1
fi
