# Self-contained Linux build & packaging toolchain for Entropy: deb, rpm and
# archlinux via nfpm, plus the AppImage. Lets `task docker:linux` produce the
# same artifacts on any host with Docker, regardless of what is installed there.
# Базовый образ пиннится дайджестом: тег rust:1.97-bookworm пересобирается
# апстримом, и «тот же Dockerfile» иначе даёт разный тулчейн в разные дни.
# Дайджест индекса, а не конкретной платформы, — образ собирается и на arm64.
ARG RUST_IMAGE=rust:1.97-bookworm@sha256:705e294093973d7c10e83400393dce7b3611f8e03e55a80af7fff6d02ae1affb
FROM ${RUST_IMAGE}

ARG DEBIAN_FRONTEND=noninteractive

# Тот же набор, что ставит scripts/prepare_env.sh для debian, плюс bsdtar из
# libarchive-tools — им проверяет содержимое пакетов scripts/test_linux_packages.sh.
RUN set -eux; \
	apt-get update; \
	apt-get install -y --no-install-recommends \
		libgtk-3-dev \
		libudev-dev \
		libhidapi-dev \
		libxkbcommon-dev \
		libxcb-render0-dev \
		libxcb-shape0-dev \
		libxcb-xfixes0-dev \
		libgl1-mesa-dev \
		libssl-dev \
		libarchive-tools \
		ca-certificates \
		curl; \
	rm -rf /var/lib/apt/lists/*

# Версии и суммы — те же, что у локальной подготовки (scripts/prepare_env.sh),
# чтобы контейнер и машина разработчика не разъезжались по тулчейну.
# Загрузки: сумма проверяется до использования, а таймауты обязательны — без
# --max-time оборванная TCP-сессия вешает сборку до лимита job'а, а не падает.
COPY scripts/tool_pins.sh scripts/verify_sha256.sh /tmp/pins/

# nfpm (deb/rpm/archlinux) и go-task — оба статические Go-бинарники.
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

# Контейнер запускается от UID хозяина каталога, а не от root, поэтому
# оставленный образом root-only кэш реестра ронял бы саму сборку на правах:
# cargo создаст его заново от текущего пользователя, сам CARGO_HOME открыт.
RUN set -eux; \
	rm -rf "$CARGO_HOME/registry" "$CARGO_HOME/git" "$CARGO_HOME/.package-cache"

# appimagetool забайкан в образ, чтобы упаковка не ходила в сеть. Версия и
# контрольная сумма берутся из того же scripts/appimagetool_pin.sh, что и у
# локальной сборки, — иначе в образе оказался бы не тот инструмент, который
# проверяют тесты. Слой идёт последним: обновление пина не должно
# инвалидировать всё, что выше.
# APPIMAGE_EXTRACT_AND_RUN позволяет запускать его без FUSE внутри контейнера.
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
