# Self-contained cross-build + packaging toolchain for Entropy.
# Builds Linux (native) and Windows (cargo-zigbuild, mingw) targets and packages
# deb/rpm/archlinux (nfpm), AppImage (appimagetool) and MSI (wixl).
# macOS is built on a real Mac (hybrid model); the darwin Rust targets and zig are
# installed here only for the optional/experimental cross-build path.
# Базовый образ пиннится дайджестом: тег rust:1.97-bookworm пересобирается
# апстримом, и «тот же Dockerfile» иначе даёт разный тулчейн в разные дни.
# Дайджест индекса, а не конкретной платформы, — образ собирается и на arm64.
ARG RUST_IMAGE=rust:1.97-bookworm@sha256:705e294093973d7c10e83400393dce7b3611f8e03e55a80af7fff6d02ae1affb
FROM ${RUST_IMAGE}

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

# Версии и суммы — те же, что у локальной подготовки (scripts/prepare_env.sh),
# чтобы контейнер и машина разработчика не разъезжались по тулчейну.
# Загрузки: сумма проверяется до использования, а таймауты обязательны — без
# --max-time оборванная TCP-сессия вешает сборку до лимита job'а, а не падает.
COPY scripts/tool_pins.sh scripts/verify_sha256.sh /tmp/pins/

# Zig (used by cargo-zigbuild as the cross linker / C compiler).
# Имя архива — zig-<arch>-<os>-<version>: с 0.15 порядок arch и os в нём
# обратный прежнему, поэтому старая схема zig-linux-<arch> отдаёт 404.
RUN set -eux; \
	. /tmp/pins/tool_pins.sh; \
	arch="$(uname -m)"; \
	curl -fsSL --retry 3 --retry-all-errors --connect-timeout 15 --max-time 600 \
		"https://ziglang.org/download/${ZIG_VERSION}/zig-${arch}-linux-${ZIG_VERSION}.tar.xz" -o /tmp/zig.tar.xz; \
	bash /tmp/pins/verify_sha256.sh /tmp/zig.tar.xz "$(tool_sha256 "zig:${arch}")"; \
	mkdir -p /opt/zig; \
	tar -xJf /tmp/zig.tar.xz -C /opt/zig --strip-components=1; \
	ln -s /opt/zig/zig /usr/local/bin/zig; \
	rm /tmp/zig.tar.xz; \
	zig version

# nfpm (deb/rpm/archlinux/apk) and go-task, both static Go binaries.
RUN set -eux; \
	. /tmp/pins/tool_pins.sh; \
	arch="$(dpkg --print-architecture)"; \
	curl -fsSL --retry 3 --retry-all-errors --connect-timeout 15 --max-time 300 \
		"https://github.com/goreleaser/nfpm/releases/download/v${NFPM_VERSION}/nfpm_${NFPM_VERSION}_${arch}.deb" -o /tmp/nfpm.deb; \
	bash /tmp/pins/verify_sha256.sh /tmp/nfpm.deb "$(tool_sha256 "nfpm-deb:${arch}")"; \
	dpkg -i /tmp/nfpm.deb; \
	rm /tmp/nfpm.deb; \
	curl -fsSL --retry 3 --retry-all-errors --connect-timeout 15 --max-time 300 \
		"https://github.com/go-task/task/releases/download/v${TASK_VERSION}/task_linux_${arch}.deb" -o /tmp/task.deb; \
	bash /tmp/pins/verify_sha256.sh /tmp/task.deb "$(tool_sha256 "task-deb:${arch}")"; \
	dpkg -i /tmp/task.deb; \
	rm /tmp/task.deb

# macOS-cross tooling (EXPERIMENTAL path): quill (Mach-O signing from Linux),
# konoui lipo (universal binaries) and libdmg-hfsplus (.dmg from Linux).
RUN set -eux; \
	. /tmp/pins/tool_pins.sh; \
	arch="$(dpkg --print-architecture)"; \
	curl -fsSL --retry 3 --retry-all-errors --connect-timeout 15 --max-time 300 \
		"https://github.com/anchore/quill/releases/download/v${QUILL_VERSION}/quill_${QUILL_VERSION}_linux_${arch}.tar.gz" -o /tmp/quill.tgz; \
	bash /tmp/pins/verify_sha256.sh /tmp/quill.tgz "$(tool_sha256 "quill:${arch}")"; \
	tar -xzf /tmp/quill.tgz -C /usr/local/bin quill; \
	curl -fsSL --retry 3 --retry-all-errors --connect-timeout 15 --max-time 300 \
		"https://github.com/konoui/lipo/releases/download/v${LIPO_VERSION}/lipo_linux_${arch}.tar.gz" -o /tmp/lipo.tgz; \
	bash /tmp/pins/verify_sha256.sh /tmp/lipo.tgz "$(tool_sha256 "lipo:${arch}")"; \
	tar -xzf /tmp/lipo.tgz -C /usr/local/bin lipo; \
	chmod +x /usr/local/bin/quill /usr/local/bin/lipo; \
	rm -f /tmp/quill.tgz /tmp/lipo.tgz

RUN set -eux; \
	. /tmp/pins/tool_pins.sh; \
	git init -q /tmp/libdmg; \
	git -C /tmp/libdmg fetch --depth 1 -q "$LIBDMG_REPO" "$LIBDMG_REV"; \
	git -C /tmp/libdmg checkout -q FETCH_HEAD; \
	cmake -S /tmp/libdmg -B /tmp/libdmg/build -DCMAKE_BUILD_TYPE=Release; \
	make -C /tmp/libdmg/build/dmg dmg-bin; \
	install -m 0755 /tmp/libdmg/build/dmg/dmg /usr/local/bin/dmg; \
	rm -rf /tmp/libdmg

RUN set -eux; \
	. /tmp/pins/tool_pins.sh; \
	rustup target add \
		x86_64-pc-windows-gnu \
		x86_64-apple-darwin \
		aarch64-apple-darwin; \
	cargo install cargo-zigbuild --version "${CARGO_ZIGBUILD_VERSION}" --locked; \
	# Контейнер запускается от UID хозяина каталога, а не от root, поэтому
	# оставленный здесь root-only кэш реестра ронял бы саму сборку: cargo
	# создаст его заново от текущего пользователя (сам CARGO_HOME открыт).
	rm -rf "$CARGO_HOME/registry" "$CARGO_HOME/git" "$CARGO_HOME/.package-cache"

# AppImage tooling: appimagetool baked in so the container needs no network at
# packaging time. Версия и контрольная сумма берутся из того же
# scripts/appimagetool_pin.sh, что и у локальной сборки, — иначе в образе
# оказался бы другой инструмент, чем проверяют тесты. Слой идёт последним:
# обновление пина не должно инвалидировать сборку cargo-zigbuild выше.
# APPIMAGE_EXTRACT_AND_RUN lets it run without FUSE inside a plain container.
COPY scripts/appimagetool_pin.sh /tmp/appimage-pin/
RUN set -eux; \
	. /tmp/appimage-pin/appimagetool_pin.sh; \
	curl -fsSL --retry 3 --retry-all-errors --connect-timeout 15 --max-time 300 \
		"$APPIMAGETOOL_PINNED_URL" -o /usr/local/bin/appimagetool; \
	bash /tmp/pins/verify_sha256.sh /usr/local/bin/appimagetool "$APPIMAGETOOL_PINNED_SHA256"; \
	chmod +x /usr/local/bin/appimagetool; \
	rm -rf /tmp/appimage-pin /tmp/pins
ENV APPIMAGETOOL=/usr/local/bin/appimagetool
ENV APPIMAGE_EXTRACT_AND_RUN=1

WORKDIR /work
