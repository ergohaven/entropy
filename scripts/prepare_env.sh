#!/usr/bin/env bash
# Installs what the Linux packaging targets need on this machine: detects the
# distro family, installs the build prerequisites with the native package
# manager, and downloads nfpm into the project cache.
#
#   --tools-only skip the distro packages, only fetch nfpm (used by CI)
#   --dry-run    print the commands instead of running them
#   --no-strict  exit 0 even if some tools are still missing
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck disable=SC1091
source scripts/tool_pins.sh
# Скачиваемые инструменты кладём в кэш проекта, а не в систему: их подхватывают
# Taskfile и скрипты сборки, дополняя PATH только на время запуска.
TOOLS_DIR="$ROOT/.cache/tools"
TOOLS_ONLY=0
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
	--tools-only) TOOLS_ONLY=1 ;;
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

if [[ "$(uname -s)" != Linux ]]; then
	warn "this script only covers the Linux packaging targets"
	warn "to build the app itself on $(uname -s), see README > Development"
	exit 1
fi

run() {
	if ((DRY_RUN)); then
		printf '  %s\n' "$*"
		return 0
	fi
	"$@"
}

# sudo only when it is actually needed: a root shell skips it.
SUDO=()
if [[ "$(id -u)" -ne 0 ]] && command -v sudo >/dev/null 2>&1; then
	SUDO=(sudo)
fi

have() { command -v "$1" >/dev/null 2>&1; }

# Загружаемые инструменты ищем и в кэше проекта: он не в PATH, но сборка берёт
# их именно оттуда, так что повторно качать не нужно.
have_tool() { have "$1" || [[ -x "$TOOLS_DIR/$1" ]]; }

detect_linux_family() {
	local id="" like=""
	if [[ -r /etc/os-release ]]; then
		# shellcheck disable=SC1091
		{
			id="$(. /etc/os-release && echo "${ID:-}")"
			like="$(. /etc/os-release && echo "${ID_LIKE:-}")"
		}
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

# Что нужно `cargo build`. Упаковка сверх этого требует только nfpm, который
# ставится ниже, — иконки лежат в репозитории готовыми.
build_packages_for() {
	case "$1" in
	debian) echo "build-essential pkg-config libhidapi-dev libudev-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev libssl-dev libgtk-3-dev" ;;
	suse) echo "gcc gcc-c++ pkgconf-pkg-config libhidapi-devel systemd-devel libxcb-devel libxkbcommon-devel libopenssl-devel gtk3-devel" ;;
	fedora) echo "gcc gcc-c++ pkgconf-pkg-config hidapi-devel systemd-devel libxcb-devel libxkbcommon-devel openssl-devel gtk3-devel" ;;
	arch) echo "base-devel pkgconf hidapi systemd-libs libxcb libxkbcommon openssl gtk3" ;;
	alpine) echo "build-base pkgconf hidapi-dev eudev-dev libxcb-dev libxkbcommon-dev openssl-dev gtk+3.0-dev" ;;
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

# Rust берём из .tool-versions через asdf: версия тогда одинакова у всех и
# ничего не ставится в систему глобально.
install_rust() {
	if ! have asdf; then
		have cargo || warn "neither asdf nor cargo found — install Rust with rustup: https://rustup.rs"
		return 0
	fi

	asdf plugin list 2>/dev/null | grep -qx rust || run asdf plugin add rust
	log "Installing Rust from .tool-versions"
	run asdf install rust
}

# Сумма сверяется до распаковки, поэтому качаем в файл, а не в pipe. Таймауты
# обязательны: без --max-time оборванное соединение висит без ограничения.
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

	local url="https://github.com/goreleaser/nfpm/releases/download/v${NFPM_VERSION}/nfpm_${NFPM_VERSION}_Linux_${arch}.tar.gz"
	log "Installing nfpm ${NFPM_VERSION} into .cache/tools"
	if ((DRY_RUN)); then
		printf '  curl -fsSL %s -o .cache/tools/nfpm.tar.gz (sha256-verified) && tar -xz nfpm\n' "$url"
		return 0
	fi

	mkdir -p "$TOOLS_DIR"
	local archive="$TOOLS_DIR/nfpm.tar.gz"
	rm -f "$archive"
	curl -fsSL --retry 3 --retry-all-errors --connect-timeout 15 --max-time 300 "$url" -o "$archive"
	"$ROOT/scripts/verify_sha256.sh" "$archive" "$(tool_sha256 "nfpm-tar:Linux_${arch}")"
	tar -xzf "$archive" -C "$TOOLS_DIR" nfpm
	rm -f "$archive"
	chmod 0755 "$TOOLS_DIR/nfpm"
}

# Проверяем не имена пакетов, а сами инструменты: набор пакетов покрывает не все
# дистрибутивы одинаково, и нехватка иначе всплыла бы на середине сборки.
report_missing_tools() {
	local -a missing=()
	have cargo || missing+=("cargo (build)")
	have_tool nfpm || missing+=("nfpm (deb/rpm/arch)")

	((${#missing[@]})) || return 0
	warn "not installed on this system — the matching targets will fail:"
	printf '     - %s\n' "${missing[@]}" >&2
	warn "see BUILD.md > Prerequisites for the package names"
	return 1
}

if ((TOOLS_ONLY)); then
	log "Tools only — skipping the distro packages"
else
	FAMILY="$(detect_linux_family)"
	if [[ "$FAMILY" == unknown:* ]]; then
		warn "unsupported distro (${FAMILY#unknown:}); install the tools listed in BUILD.md by hand"
		exit 1
	fi
	log "Linux, $FAMILY family"

	read -r -a PKGS <<<"$(build_packages_for "$FAMILY")"
	log "Installing: ${PKGS[*]}"
	install_linux_packages "$FAMILY" "${PKGS[@]}"
	install_rust
fi
install_nfpm

MISSING=0
((DRY_RUN)) || report_missing_tools || MISSING=1
((DRY_RUN)) && log "dry run — nothing was installed"
log "Done. Next: 'task build' or 'task linux:all'."

# Молчаливый успех при недостающих инструментах делает prepare бесполезным как
# гейт: нехватка всплывала бы уже на середине сборки. --no-strict оставлен для
# «поставь что можешь» на дистрибутивах, где части пакетов просто нет.
if ((MISSING)) && ((STRICT)); then
	warn "some tools are still missing; pass --no-strict to ignore"
	exit 1
fi
