# Building and packaging Entropy

Everything is driven by [go-task](https://taskfile.dev). `task` with no arguments
lists every target with its description; this document explains what they do and
when to reach for each one.

## Two strategies

- **native** — build for the OS you are on. Best quality: real code signing and a
  native `.dmg` on macOS. Entry points: `task build`, `task package`.
- **docker** — reproducible cross-build of the Linux and Windows artifacts inside
  a pinned toolchain image. Runs from any host with Docker and needs nothing else
  installed. Entry point: `task docker:dist` — this is what CI runs for releases.

What each host can produce:

| Host | Natively | Via Docker |
| --- | --- | --- |
| Linux | deb/rpm/archlinux, AppImage, Windows exe + MSI, universal macOS `.app`/`.dmg` | same, reproducibly |
| macOS | macOS `.app` + `.dmg` with real signing | Linux and Windows artifacts |
| Windows | `.exe` only | everything else |

## Quick start

```sh
task prepare        # install prerequisites for this host
task build          # release binary
task package        # native package(s) for this OS
```

Add the cross toolchain when you also want Windows or macOS artifacts locally:

```sh
task prepare:cross
```

Both accept flags after `--`; `task prepare -- --dry-run` prints what would be
installed without touching anything. `prepare` exits non-zero when a tool it
needs is still missing afterwards, so it works as a gate in scripts; pass
`-- --no-strict` to install what is available and ignore the rest.

## Toolchain versions

Rust and zig come from [asdf](https://asdf-vm.com) and are pinned in
`.tool-versions`, so everyone builds with the same versions and nothing is
installed system-wide:

```
rust 1.97.0
zig 0.16.0
```

The versions of everything that gets downloaded — zig, nfpm, go-task, quill,
lipo, libdmg-hfsplus and the macOS SDK — live in `scripts/tool_pins.sh` together
with their SHA-256 digests, and both the `Dockerfile` and `scripts/prepare_env.sh`
read that one file. Every download is verified against it before use, and an
unknown platform is an error rather than an unchecked download. The
`cargo-zigbuild` line (`~0.23`) is pinned there too: each release targets a
specific zig, so it and `.tool-versions` move together or the local and container
builds drift apart. The base image is pinned by digest for the same reason.

On Windows `prepare` reads `.tool-versions` as well and selects that Rust
toolchain with `rustup override`, since there is no asdf to do it.

`task prepare` adds the `rust` and `zig` plugins if missing and installs those
versions. Without asdf it falls back to the distro package for zig and asks you
to install Rust via [rustup](https://rustup.rs).

Tools that have to be downloaded — `nfpm`, `quill`, `dmg` and `appimagetool` —
are kept in `.cache/tools/` inside the repository, not installed system-wide.
The Taskfile and the build scripts append that directory to `PATH` for the
duration of a command, so nothing leaks into your shell. Anything already on
your `PATH` wins, so a system-wide `nfpm` is used as-is and never downloaded.
Deleting `.cache/` undoes all of it.

`cargo-zigbuild` is the exception: it is a cargo subcommand and is installed
with `cargo install` alongside your toolchain.

## Prerequisites by distro

`task prepare` installs these for you; the table is here for reference and for
unsupported distros.

| Tool | Needed for | Debian/Ubuntu | openSUSE | Fedora | Arch |
| --- | --- | --- | --- | --- | --- |
| `rsvg-convert` (or `inkscape`) | rasterizing `assets/entropy.svg` | `librsvg2-bin` | `rsvg-convert` | `librsvg2-tools` | `librsvg` |
| `magick` / `convert` | `assets/entropy.ico` | `imagemagick` | `ImageMagick` | `ImageMagick` | `imagemagick` |
| `png2icns` | `assets/entropy.icns` | `icnsutils` | `icns-utils` | `libicns-utils` | AUR `libicns` |
| `envsubst` | nfpm config templating | `gettext-base` | `envsubst` | `gettext-envsubst` | `gettext` |
| `wixl` | Windows MSI | `wixl` | `msitools` | `msitools` | `msitools` |
| `genisoimage` / `mkisofs` | HFS hybrid ISO for the `.dmg` | `genisoimage` | `mkisofs` | `genisoimage` | `libisoburn` |

Not packaged by any distro, so `prepare` downloads them into `.cache/tools/`:
[nfpm](https://github.com/goreleaser/nfpm) (deb/rpm/archlinux),
[quill](https://github.com/anchore/quill) (Mach-O signing from Linux) and
[libdmg-hfsplus](https://github.com/fanquake/libdmg-hfsplus) (builds the `.dmg`;
compiled from the revision pinned in `scripts/tool_pins.sh`). `appimagetool`
lands in the same place on first use; its release and SHA-256 are pinned in
`scripts/appimagetool_pin.sh` and verified on every build, whether the tool was
just downloaded or came from the cache — a cached copy that no longer matches the
pin is refetched and verified again. Update the pin with
`bash scripts/update_appimagetool_pin.sh <release-tag>`.

On macOS the native build only needs the Xcode command line tools — `codesign`,
`hdiutil`, `lipo` and `plutil` ship with them.

## Task reference

### Environment

| Task | What it does |
| --- | --- |
| `task` | list all tasks |
| `task version` | print the version parsed from `Cargo.toml` |
| `task prepare` | install build and packaging prerequisites for this host |
| `task prepare:cross` | the same plus the Windows/macOS cross toolchain |
| `task clean` | remove `dist`, `target/appimage`, `assets/icons` |

`clean` deliberately keeps `assets/entropy.ico` and `assets/entropy.icns`: both
are committed. `build.rs` needs the `.ico` for Windows builds, and the `.icns` is
needed by the macOS runners, which have nothing installed to rasterize the SVG.
`task icons` regenerates both — commit the result when the logo changes.

### Assets

`task icons` rasterizes `assets/entropy.svg` into the hicolor PNG set (16–512),
then builds `entropy.ico` for Windows and `entropy.icns` for macOS. It picks
whichever tool is available: `rsvg-convert` or `inkscape`, `magick` or `convert`,
`png2icns` on Linux or `iconutil` on macOS. The packaging targets depend on it,
so you rarely run it directly. It re-runs when the SVG changes or any generated
file goes missing.

### Build

| Task | What it does |
| --- | --- |
| `task build` | `cargo build --release --locked` for this host |
| `task package` | native package(s): Linux → `linux:all`, macOS → `macos:all`, Windows → `windows:native` |

### Linux

| Task | Output in `dist/linux/` |
| --- | --- |
| `task linux:deb` | `.deb` |
| `task linux:rpm` | `.rpm` |
| `task linux:arch` | `.pkg.tar.zst` |
| `task linux:pkg` | all three |
| `task linux:appimage` | `.AppImage` |
| `task linux:all` | packages + AppImage |

Packages are built by nfpm from `packaging/nfpm/nfpm.yaml.tpl`, with `PKG_ARCH`,
`VERSION` and `ENTROPY_BIN` substituted by `envsubst`. `PKG_ARCH` follows the
host (`amd64` or `arm64`) instead of assuming x86_64 — Docker Desktop on Apple
Silicon runs an arm64 container, and a package labelled `amd64` with an arm64
binary inside installs and then fails to start. To package a binary for another
architecture, override both:

```sh
task linux:deb PKG_ARCH=arm64 ENTROPY_BIN=path/to/arm64/entropy
```

The label is checked against the binary before packaging, so a mismatch fails
the build instead of shipping. `task linux:appimage` is still x86_64-only: the
pinned `appimagetool` is an x86_64 build, and it says so rather than producing a
mislabelled AppImage.

`VERSION` defaults to the version in `Cargo.toml` and can be overridden the same
way. The release workflow passes the tag, so a `v1.2.3-rc.1` prerelease produces
`entropy-v1.2.3-rc.1-*` files instead of files named after the future stable
release; nfpm turns the prerelease suffix into a proper deb/rpm version.

The rpm dependencies are declared as soname provides (`libX11.so.6()(64bit)`)
rather than package names, because those names differ between rpm distros.

### Windows

| Task | Where to run | Output |
| --- | --- | --- |
| `task windows:native` | **on Windows** | portable `.exe` in `dist/windows/`, built with the native toolchain |
| `task windows:build` | on Linux | `target/x86_64-pc-windows-gnu/release/entropy.exe` (cargo-zigbuild) |
| `task windows:portable` | on Linux | portable `.exe` in `dist/windows/` |
| `task windows:msi` | on Linux | `.msi` installer built by wixl from `packaging/windows/entropy.wxs` |
| `task windows:all` | on Linux | portable exe + MSI |

`task package` on Windows runs `windows:native`. The MSI cannot be produced
there: `wixl` comes from msitools and has no Windows build, and cargo-zigbuild
is pointless when the host compiler is already the target one. Use
`task docker:windows` for the installer.

The MSI upgrade range deliberately includes the version being installed, so the
stable release replaces an installed release candidate — both carry the same
`ProductVersion`, because MSI does not accept the `-rc.1` suffix.

### macOS

| Task | Where to run | How |
| --- | --- | --- |
| `task macos:app`, `task macos:all` | **on a Mac** | native: both slices built and merged with `lipo`, `codesign`, `hdiutil` — real signature, native universal `.dmg` |
| `task macos:cross` | **on Linux** | zig + macOS SDK + quill (ad-hoc signature) + libdmg-hfsplus |
| `task macos:prepare-sdk` | Linux | download the pinned macOS SDK into `.cache` (the cross-build calls it itself) |

Both paths produce one universal `.dmg` covering Apple Silicon and Intel, so
there is no per-architecture download. The cross-build signs ad-hoc only, and
notarization still requires a Mac with Apple credentials — releases therefore use
the native path on macOS runners, which is also the licensing-safe way to use
Apple's SDK.

The cross-build fails if signing fails or `quill` is missing, rather than
quietly producing a bundle macOS will refuse to open. `ALLOW_UNSIGNED=1 task
macos:cross` opts out on purpose and names the result `-unsigned.dmg`. The SDK
download is always checksummed against `scripts/tool_pins.sh`; overriding
`MACOS_SDK_URL` or `MACOS_SDK_VERSION` requires passing `MACOS_SDK_SHA256`.

### Docker

| Task | What it does |
| --- | --- |
| `task docker:image` | build the `entropy-build:local` toolchain image |
| `task docker:linux` | deb/rpm/arch + AppImage in the container |
| `task docker:windows` | portable exe + MSI in the container |
| `task docker:dist` | Linux + Windows in one go — what the release workflow runs |
| `task docker:macos-cross` | macOS `.app` + `.dmg` cross-built in the container |

All of them build the image first if needed. The container mounts the repository
at `/work` and runs as your own user, so everything it writes — `dist/`,
`target/`, `.task/` and the generated icons — belongs to you and needs no
ownership fixup afterwards. Setting `DOCKER_IMAGE_READY=1` skips the image build
— CI uses it because it builds the image in a separate, layer-cached step.

## CI

- `.github/workflows/build.yml` — builds and tests on pull requests and on
  manual runs. A new push to a PR cancels the previous run. The macOS job builds
  the same universal bundle as the release and asserts both slices are present,
  so the `lipo` path is exercised before a tag depends on it.
- `.github/workflows/release.yml` — runs on semver tags (`v1.2.3`, and
  `v1.2.3-rc.1` for prereleases): `task docker:dist` for the Linux and Windows
  artifacts, `task macos:all` on a macOS runner for the signed universal `.dmg`,
  then publishes a GitHub Release.

Everything that can be checked before building is checked in the `validate` job —
the tag format, that `CHANGELOG.md` has a matching `## <tag>` section to use as
release notes, and that the tag agrees with the version in `Cargo.toml`. The
builds take tens of minutes, so failing on a missing changelog entry afterwards
would throw all of that away.

The release workflow can also be started manually from the Actions tab, passing
an existing tag to publish — useful for re-running a release whose artifacts
failed to upload. Every job checks out that tag rather than the branch, and the
release is published against it. Releases never cancel each other: an interrupted
run would leave a partially uploaded release.

## Troubleshooting

**`file does not exist` when running any `docker:*` task.** SELinux (openSUSE,
Fedora) labels the repository `user_home_t`, which the container's `container_t`
domain cannot read. The Taskfile adds `--security-opt label=disable` when
`getenforce` reports SELinux is active; if you invoke `docker run` by hand, add
it yourself.

**`Failed to find zig` / `empty string, expected a semver version`.** An asdf or
mise shim is first in `PATH`, and shims resolve the version from
`.tool-versions` found upwards from the *current* directory. Cargo runs build
scripts from the crate's directory inside the registry, where there is no
`.tool-versions`, so the shim prints a hint instead of a version. The Taskfile
resolves zig to the real binary (`asdf which zig`, then candidates that still
report a version when run from an unrelated directory) and passes it through
`CARGO_ZIGBUILD_ZIG_PATH`. If it still fails, check `asdf which zig` by hand.

**`zlib-devel conflicts with zlib-ng-compat-devel`.** Tumbleweed and Fedora ship
zlib-ng by default. `prepare` asks for the `pkgconfig(zlib)` capability instead
of the package name, which the installed zlib-ng already satisfies.

**Files in `target/` owned by root.** Left over from an older `docker:*` run:
the container now runs as your own user and no longer creates them. Fix the old
ones without sudo:

```sh
docker run --rm --security-opt label=disable --user 0:0 -v "$PWD":/work -w /work \
  entropy-build:local sh -c 'chown -R "'"$(id -u)"'":"'"$(id -g)"'" target dist .task assets'
```

**`need genisoimage or mkisofs plus the 'dmg' tool`.** The ISO generator comes
from `task prepare`, but libdmg-hfsplus has no distro package — `task
prepare:cross` compiles it, or use `task docker:macos-cross` instead.

**Local and container builds keep recompiling.** They share one `target/`
directory. Set `CARGO_TARGET_DIR` to keep them apart.
