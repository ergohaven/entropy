# Building and packaging Entropy on Linux

Linux packaging is driven by [go-task](https://taskfile.dev). `task` with no
arguments lists every target with its description; this document explains what
they do and when to reach for each one.

Windows and macOS builds are still the plain `cargo` invocations described in
[README > Development](README.md#development).

## Two ways to build

- **native** — build on the host, with the prerequisites installed there.
  Entry points: `task build`, `task linux:all`.
- **container** — the same artifacts from a pinned toolchain image, on any host
  with Docker and nothing else installed. Entry point: `task docker:linux`.

Both run the same tasks and produce the same files in `dist/linux/`; the
container just fixes the toolchain, so a package built on Tumbleweed and one
built in CI are the same package.

## Quick start

```sh
task prepare        # install the prerequisites for this host
task build          # release binary
task linux:all      # deb, rpm, archlinux and AppImage in dist/linux/
```

Or, with nothing installed but Docker:

```sh
task docker:linux
```

`task prepare` accepts flags after `--`; `task prepare -- --dry-run` prints what
would be installed without touching anything. It exits non-zero when a tool it
needs is still missing afterwards, so it works as a gate in scripts; pass
`-- --no-strict` to install what is available and ignore the rest.

## Prerequisites

`task prepare` installs these for you. It knows the Debian/Ubuntu, openSUSE,
Fedora, Arch and Alpine package managers; on anything else install the
equivalents by hand.

| Tool | Needed for | Where it comes from |
| --- | --- | --- |
| Rust toolchain | everything | [asdf](https://asdf-vm.com) from `.tool-versions`, or [rustup](https://rustup.rs) |
| GUI/HID build headers | `cargo build` | distro packages, same set as CI |
| `nfpm` | deb/rpm/archlinux | downloaded into `.cache/tools/` |
| `appimagetool` | AppImage | downloaded into `.cache/tools/` on first use |
| `bsdtar` | `scripts/test_linux_packages.sh` | `libarchive-tools` / `bsdtar` / `libarchive` |
| ImageMagick | `task icons`, only when the logo changes | distro package |

Downloaded tools live in `.cache/tools/` inside the repository rather than being
installed system-wide. The Taskfile appends that directory to `PATH` for the
duration of a command, so nothing leaks into your shell, and anything already on
your `PATH` wins — a system-wide `nfpm` is used as-is and never downloaded.
Deleting `.cache/` undoes all of it.

Every download is checksummed before use. The nfpm version and digests live in
`scripts/tool_pins.sh`; the appimagetool release and digest live in
`scripts/appimagetool_pin.sh` and are moved by
`bash scripts/update_appimagetool_pin.sh <release-tag>`. An unknown platform is
an error rather than an unverified download.

## Task reference

| Task | What it does |
| --- | --- |
| `task` | list all tasks |
| `task version` | print the version parsed from `Cargo.toml` |
| `task prepare` | install the build and packaging prerequisites |
| `task build` | `cargo build --release --locked` |
| `task clean` | remove `dist/`, `target/appimage`, `target/nfpm` |
| `task icons` | regenerate the hicolor icon set from `assets/entropy.ico` |
| `task linux:deb` | `.deb` in `dist/linux/` |
| `task linux:rpm` | `.rpm` |
| `task linux:arch` | `.pkg.tar.zst` |
| `task linux:pkg` | all three |
| `task linux:appimage` | `.AppImage` |
| `task linux:all` | packages + AppImage |
| `task docker:image` | build the `entropy-build:local` toolchain image |
| `task docker:linux` | `linux:all` inside that image |

`task docker:linux` builds the image first if needed. The container mounts the
repository at `/work` and runs as your own user, so everything it writes —
`dist/`, `target/`, `.task/` — belongs to you and needs no ownership fixup
afterwards. Setting `DOCKER_IMAGE_READY=1` skips the image build, for CI setups
that build it in a separate, layer-cached step.

The image pins its base by digest, not by tag: `rust:1.97-bookworm` is rebuilt
upstream, and without the digest the same `Dockerfile` would give a different
toolchain on different days. `nfpm`, `go-task` and `appimagetool` come from the
same pins the host path uses, so the two never drift apart.

## What ends up in a package

| Path | Source |
| --- | --- |
| `/usr/bin/entropy` | `cargo build --release` |
| `/usr/share/applications/entropy.desktop` | `packaging/linux/entropy.desktop` |
| `/usr/share/metainfo/com.ergohaven.entropy.metainfo.xml` | `packaging/linux/` |
| `/usr/lib/udev/rules.d/59-vial.rules` | `packaging/linux/59-vial.rules` |
| `/usr/share/icons/hicolor/*/apps/entropy.png` | `assets/icons/` |
| `/usr/share/doc/ergohaven-entropy/LICENSE` | `LICENSE` |

The AppImage ships the same desktop entry, metainfo and icons, so an app
installed from a package and one launched from the AppImage describe themselves
identically.

`scripts/test_linux_packages.sh` asserts that every one of those paths is
present in all three package formats — CI runs it on each pull request and then
installs the `.deb` for real.

The packaged udev rule uses `TAG+="uaccess"` and hands the device to the logged
in session, so no group membership is needed. It lands in `/usr/lib/udev`, which
does not collide with the `/etc/udev` copy that `linux/udev/install-vial-rules.sh`
writes for source checkouts; the `/etc` copy keeps taking precedence.

### Dependencies

Package dependencies are declared by hand rather than auto-detected. The binary
has exactly three ELF dependencies — `libc`, `libm` and `libgcc_s` — because the
whole GUI stack (libGL, xkbcommon, X11/xcb, wayland) is loaded through `dlopen`,
hidapi uses its pure-Rust hidraw backend instead of libudev, and file dialogs go
through xdg-desktop-portal rather than GTK. Auto-detection would therefore
declare almost nothing, and the app would install and fail to start.

The rpm dependencies are declared as soname provides (`libX11.so.6()(64bit)`)
rather than package names, because those names differ between rpm distros
(`libX11` on Fedora, `libX11-6` on openSUSE) while the sonames do not.

### Architecture and version

`PKG_ARCH` follows the host (`amd64` or `arm64`) instead of assuming x86_64. To
package a binary built elsewhere, override both:

```sh
task linux:deb PKG_ARCH=arm64 ENTROPY_BIN=path/to/arm64/entropy
```

The label is checked against the binary with `readelf` before packaging, so a
mismatch fails the build instead of shipping a package that installs and then
does not start. `task linux:appimage` is x86_64-only: the pinned `appimagetool`
is an x86_64 build, and it says so rather than producing a mislabelled AppImage.

`VERSION` defaults to the version in `Cargo.toml` and can be overridden the same
way (`task linux:pkg VERSION=0.3.10-rc.1`). nfpm turns a prerelease suffix into a
proper deb/rpm version (`0.3.10~rc.1`), which sorts *before* the stable release.
pacman has no equivalent, so an Arch prerelease becomes `0.3.10rc.1` and sorts
*after* it — worth knowing before publishing Arch packages for a release
candidate.

### Icons

`assets/icons/hicolor/` is generated from `assets/entropy.ico` and committed
alongside it, so packaging needs no image tooling at all. Run `task icons` and
commit the result when the logo changes. The `.ico` carries hand-drawn 16, 32 and
48 pixel variants; those are copied as-is and only the missing sizes are scaled
down from the 256 pixel frame.

## Troubleshooting

**`file does not exist` when running `task docker:linux`.** SELinux (openSUSE,
Fedora) labels the repository `user_home_t`, which the container's `container_t`
domain cannot read. The Taskfile adds `--security-opt label=disable` when
`getenforce` reports SELinux is active; if you invoke `docker run` by hand, add
it yourself.

**Local and container builds keep recompiling.** They share one `target/`
directory. Set `CARGO_TARGET_DIR` to keep them apart.

**`no matching files` from nfpm.** nfpm expands `${...}` in scalar fields but not
in `contents[].src`, so the binary is staged to `target/nfpm/entropy` first. Run
`task linux:pkg` rather than calling nfpm directly.

**`need bsdtar` from the package test.** Only the test script needs it, not the
build: install `libarchive-tools` (Debian/Ubuntu), `bsdtar` (openSUSE/Fedora) or
`libarchive` (Arch).

**Rust version drift.** `.tool-versions` pins the toolchain for asdf users;
without asdf, `prepare` leaves your existing Rust alone and only warns.
