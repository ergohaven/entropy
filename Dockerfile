# Self-contained cross-build + packaging toolchain for Entropy.
# Builds Linux (native) and Windows (cargo-zigbuild, mingw) targets and packages
# deb/rpm/archlinux (nfpm), AppImage (appimagetool) and MSI (wixl).
# macOS is built on a real Mac (hybrid model); the darwin Rust targets and zig are
# installed here only for the optional/experimental cross-build path.
ARG RUST_VERSION=1.97
FROM rust:${RUST_VERSION}-bookworm

ARG ZIG_VERSION=0.16.0
ARG NFPM_VERSION=2.47.0
ARG TASK_VERSION=3.44.0
ARG DEBIAN_FRONTEND=noninteractive

# Build-time libraries for the native Linux build + packaging/icon tooling.
RUN set -eux; \
	apt-get update; \
	apt-get install -y --no-install-recommends \
		mingw-w64 \
		libgtk-3-dev \
		libudev-dev \
		libhidapi-dev \
		libxkbcommon-dev \
		libxcb-render0-dev \
		libxcb-shape0-dev \
		libxcb-xfixes0-dev \
		libgl1-mesa-dev \
		libssl-dev \
		wixl \
		imagemagick \
		librsvg2-bin \
		icnsutils \
		genisoimage \
		cmake \
		zlib1g-dev \
		fakeroot \
		gettext-base \
		xz-utils \
		ca-certificates \
		curl \
		git; \
	rm -rf /var/lib/apt/lists/*

# Zig (used by cargo-zigbuild as the cross linker / C compiler).
# Имя архива — zig-<arch>-<os>-<version>: с 0.15 порядок arch и os в нём
# обратный прежнему, поэтому старая схема zig-linux-<arch> отдаёт 404.
RUN set -eux; \
	arch="$(uname -m)"; \
	curl -fsSL "https://ziglang.org/download/${ZIG_VERSION}/zig-${arch}-linux-${ZIG_VERSION}.tar.xz" -o /tmp/zig.tar.xz; \
	mkdir -p /opt/zig; \
	tar -xJf /tmp/zig.tar.xz -C /opt/zig --strip-components=1; \
	ln -s /opt/zig/zig /usr/local/bin/zig; \
	rm /tmp/zig.tar.xz; \
	zig version

# nfpm (deb/rpm/archlinux/apk) and go-task, both static Go binaries.
RUN set -eux; \
	arch="$(dpkg --print-architecture)"; \
	curl -fsSL "https://github.com/goreleaser/nfpm/releases/download/v${NFPM_VERSION}/nfpm_${NFPM_VERSION}_${arch}.deb" -o /tmp/nfpm.deb; \
	dpkg -i /tmp/nfpm.deb; \
	rm /tmp/nfpm.deb; \
	curl -fsSL "https://github.com/go-task/task/releases/download/v${TASK_VERSION}/task_linux_${arch}.deb" -o /tmp/task.deb; \
	dpkg -i /tmp/task.deb; \
	rm /tmp/task.deb

# AppImage tooling: appimagetool baked in and pinned so the container needs no
# network at packaging time. build_linux_appimage.sh picks it up via APPIMAGETOOL;
# APPIMAGE_EXTRACT_AND_RUN lets it run without FUSE inside a plain container.
# Runtime тоже кладём в образ: без --runtime-file appimagetool тянет его с
# GitHub на каждой сборке и на оборванном соединении висит без таймаута.
ARG APPIMAGETOOL_VERSION=1.9.1
ARG APPIMAGE_RUNTIME_VERSION=20251108
RUN set -eux; \
	case "$(uname -m)" in x86_64) aia=x86_64 ;; aarch64) aia=aarch64 ;; *) aia="$(uname -m)" ;; esac; \
	curl -fsSL "https://github.com/AppImage/appimagetool/releases/download/${APPIMAGETOOL_VERSION}/appimagetool-${aia}.AppImage" -o /usr/local/bin/appimagetool; \
	chmod +x /usr/local/bin/appimagetool; \
	curl -fsSL "https://github.com/AppImage/type2-runtime/releases/download/${APPIMAGE_RUNTIME_VERSION}/runtime-${aia}" -o /usr/local/lib/appimage-runtime; \
	test -s /usr/local/lib/appimage-runtime
ENV APPIMAGETOOL=/usr/local/bin/appimagetool
ENV APPIMAGE_RUNTIME=/usr/local/lib/appimage-runtime
ENV APPIMAGE_EXTRACT_AND_RUN=1

# macOS-cross tooling (EXPERIMENTAL path): quill (Mach-O signing from Linux),
# konoui lipo (universal binaries) and libdmg-hfsplus (.dmg from Linux).
ARG QUILL_VERSION=0.7.1
ARG LIPO_VERSION=v0.9.4
RUN set -eux; \
	arch="$(dpkg --print-architecture)"; \
	curl -fsSL "https://github.com/anchore/quill/releases/download/v${QUILL_VERSION}/quill_${QUILL_VERSION}_linux_${arch}.tar.gz" -o /tmp/quill.tgz; \
	tar -xzf /tmp/quill.tgz -C /usr/local/bin quill; \
	curl -fsSL "https://github.com/konoui/lipo/releases/download/${LIPO_VERSION}/lipo_linux_${arch}.tar.gz" -o /tmp/lipo.tgz; \
	tar -xzf /tmp/lipo.tgz -C /usr/local/bin lipo; \
	chmod +x /usr/local/bin/quill /usr/local/bin/lipo; \
	rm -f /tmp/quill.tgz /tmp/lipo.tgz

RUN set -eux; \
	git clone --depth 1 https://github.com/fanquake/libdmg-hfsplus /tmp/libdmg; \
	cmake -S /tmp/libdmg -B /tmp/libdmg/build -DCMAKE_BUILD_TYPE=Release; \
	make -C /tmp/libdmg/build/dmg dmg-bin; \
	install -m 0755 /tmp/libdmg/build/dmg/dmg /usr/local/bin/dmg; \
	rm -rf /tmp/libdmg

# Rust cross targets + cargo-zigbuild. Каждая линия cargo-zigbuild рассчитана на
# свою версию zig, поэтому 0.23 и ZIG_VERSION выше меняются только вместе — и
# должны совпадать с .tool-versions, иначе локальная и контейнерная сборки
# разъедутся по тулчейну.
ARG CARGO_ZIGBUILD_VERSION=~0.23
RUN set -eux; \
	rustup target add \
		x86_64-pc-windows-gnu \
		x86_64-apple-darwin \
		aarch64-apple-darwin; \
	cargo install cargo-zigbuild --version "${CARGO_ZIGBUILD_VERSION}" --locked

WORKDIR /work
