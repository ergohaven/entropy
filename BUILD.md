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

Both run the same tasks and produce the same files in `dist/linux/`. The
container is the canonical path: it is the only one with a fully pinned
toolchain, and it is what CI publishes. A native build is for development — its
compiler and system libraries are whatever the host happens to have.

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
| `task clean` | remove `dist/`, `target/appimage`, `target/nfpm`, `target/repro` |
| `task icons` | regenerate the hicolor icon set from `assets/entropy.ico` |
| `task linux:deb` | `.deb` in `dist/linux/` |
| `task linux:rpm` | `.rpm` |
| `task linux:arch` | `.pkg.tar.zst` |
| `task linux:pkg` | all three |
| `task linux:appimage` | `.AppImage` |
| `task linux:all` | packages + AppImage (AppImage only on x86_64) |
| `task linux:test` | assert the contents, modes and versions of what was built |
| `task linux:repro` | build twice from the same tree and compare checksums |
| `task linux:install-test` | clean-install the packages in Debian/Fedora/Arch containers |
| `task docker:image` | build the `entropy-build:local` toolchain image |
| `task docker:linux` | `linux:all` inside that image |
| `task docker:repro` | `linux:repro` inside that image |

`task docker:linux` builds the image first if needed. The container mounts the
repository at `/work` and runs as your own user, so everything it writes —
`dist/`, `target/`, `.task/` — belongs to you and needs no ownership fixup
afterwards. Setting `DOCKER_IMAGE_READY=1` skips the image build, for CI setups
that build it in a separate, layer-cached step.

The image pins its base by digest, not by tag: `rust:1.97-bookworm` is rebuilt
upstream, and without the digest the same `Dockerfile` would give a different
toolchain on different days. The Debian archive is pinned too — `DEBIAN_SNAPSHOT`
points `apt` at a dated snapshot instead of the rolling mirror, so the build
headers and runtime do not change under the same `Dockerfile`. `nfpm`, `go-task`
and `appimagetool` come from the same pins the host path uses;
`scripts/test_toolchain_pins.sh` fails the build if `.tool-versions`, the
`Dockerfile` and the CI `setup-task` version ever disagree.

## Reproducibility

Same tree in, same bytes out — `task linux:repro` (or `task docker:repro`) is
what makes that a checked claim rather than a hope: it builds everything twice
and compares SHA-256 sums. CI runs the container variant on every pull request
and publishes the build it verified.

Three things make it hold. Timestamps come from the commit rather than the clock:
`SOURCE_DATE_EPOCH` is derived from `git log -1 --pretty=%ct` and honoured by
both nfpm and the AppImage (whose `AppDir` mtimes are normalised before
`mksquashfs` sees them). The toolchain is fixed by digest, snapshot and
version pin as described above. And `mksquashfs` runs single-threaded: the
4.3 build inside the pinned appimagetool packs file tails into fragments in
whatever order its threads finish, so the same `AppDir` produced different
images. appimagetool passes no options through, so the build runs an extracted
copy of it whose `mksquashfs` is wrapped with `-processors 1`.

Overriding `SOURCE_DATE_EPOCH` in the environment wins, which is what a release
pipeline building from a tag should do.

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

Two levels of checks cover this, both run by CI on every pull request:

- `scripts/test_linux_packages.sh` (`task linux:test`) reads the built artifacts
  and asserts every path, its mode (`0755` for the binary, `0644` for everything
  else), the udev rule marker and the version each format recorded. It also
  extracts the AppImage and runs it: the executable bit says nothing about
  whether it starts.
- `scripts/test_linux_install.sh` (`task linux:install-test`) installs the
  packages in pinned, minimal Debian, Fedora and Arch images with their own
  package managers, starts the app there, checks the version ordering with the
  distro's own comparator and then removes the package again. Only a clean image
  proves the hand-written dependencies resolve — on a build host everything they
  name is already present as a build dependency.

The packaged udev rule uses `TAG+="uaccess"` and hands the device to the logged
in session, so no group membership is needed. It lands in `/usr/lib/udev`, which
does not collide with the `/etc/udev` copy that `linux/udev/install-vial-rules.sh`
writes for source checkouts; the `/etc` copy keeps taking precedence.

### Dependencies

Package dependencies are declared by hand rather than auto-detected. The binary
has exactly three ELF dependencies — `libc`, `libm` and `libgcc_s` — because the
whole GUI stack is loaded through `dlopen`, hidapi uses its pure-Rust hidraw
backend instead of libudev, and file dialogs go through xdg-desktop-portal rather
than GTK. Auto-detection would therefore declare almost nothing, and the app
would install and fail to start.

The declared set is the closure of what the binary actually loads: `libGL`,
`libEGL`, `libX11`, `libX11-xcb`, `libXcursor`, `libXi`, `libXrender`,
`libxkbcommon`, `libxkbcommon-x11`, `libwayland-client`, `libwayland-egl`, plus
`libc`/`libm`/`libgcc_s`. Both the X11 and the Wayland stack are required, because
which one gets used is decided at runtime. Re-derive the list with:

```sh
strings -a target/release/entropy | grep -oE 'lib[A-Za-z0-9_+-]*\.so[0-9.]*' | sort -u
readelf -d target/release/entropy | grep NEEDED
```

The rpm dependencies are declared as soname provides (`libX11.so.6()(64bit)`)
rather than package names, because those names differ between rpm distros
(`libX11` on Fedora, `libX11-6` on openSUSE) while the sonames do not.

#### glibc baseline

A binary built on a newer distro silently requires newer glibc symbols, and a
bare `libc6` dependency lets it install on a system where it cannot start
(`version 'GLIBC_2.39' not found`). `scripts/glibc_baseline.sh` reads the highest
`GLIBC_*` symbol version out of the binary itself, and the packages declare it
per format — for the current tree that is `libc6 (>= 2.35)`,
`libc.so.6(GLIBC_2.35)(64bit)` and `glibc>=2.35`. The number therefore follows
the binary being packaged, not the machine that packaged it. The published
artifacts come from the pinned Debian bookworm container, which fixes that
baseline for releases.

### Architecture and version

`PKG_ARCH` follows the host (`amd64` or `arm64`) instead of assuming x86_64. To
package a binary built elsewhere, override both:

```sh
task linux:deb PKG_ARCH=arm64 ENTROPY_BIN=path/to/arm64/entropy
```

The label is checked against the binary with `readelf` before packaging, so a
mismatch fails the build instead of shipping a package that installs and then
does not start.

The AppImage is x86_64-only, because the pinned `appimagetool` is an x86_64
build. `task linux:appimage` says so and fails if you ask for it on arm64;
`task linux:all` (and therefore `task docker:linux`) builds the three packages,
prints why the AppImage is missing and succeeds — an arm64 host gets real
packages instead of a build that always fails at the last step.

`VERSION` defaults to the version in `Cargo.toml` and can be overridden the same
way (`task linux:pkg VERSION=0.3.10-rc.1`). Everything that can be overridden —
`VERSION`, `PKG_ARCH`, `DIST`, `ENTROPY_BIN`, `SOURCE_DATE_EPOCH` — is validated
before it reaches a filename or a package, and reaches the shell through the
environment rather than string interpolation. `DIST` has to stay under `dist/`
or `target/`: the AppImage build deletes its previous output recursively, so
`scripts/build_linux_appimage.sh` refuses to remove anything outside `target/`,
`dist/` and `.cache/` whatever `DIST` or `TMPDIR` say.

A prerelease has to sort *before* the stable release it precedes, or the upgrade
from `0.3.10-rc.1` to `0.3.10` never happens. The three formats disagree on how,
so `scripts/pkg_version.sh` maps the semver to each of them:

| Format | `0.3.10-rc.1` | `0.3.10` | Why |
| --- | --- | --- | --- |
| deb | `0.3.10~rc.1` | `0.3.10` | dpkg sorts `~` before the empty string |
| rpm | `0.3.10~rc.1` | `0.3.10` | rpm ≥ 4.10 sorts `~` the same way |
| archlinux | `0.3.10rc.1` | `0.3.10` | pacman has no `~`, but a trailing alphabetic segment loses to none |

nfpm cannot be left to do this: with a semver prerelease it drops the suffix from
the Arch `.PKGINFO` entirely, so `0.3.10-rc.1` and `0.3.10` both become
`pkgver = 0.3.10-1` and pacman sees no upgrade at all.
`scripts/test_pkg_version.sh` covers the mapping, and the clean-install test
compares the resulting strings with `dpkg --compare-versions`, `rpmdev-vercmp`
and `vercmp` — each distro's own comparator, not a re-implementation.

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
