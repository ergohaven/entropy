#!/usr/bin/env python3
"""Cheap release identity contracts; no application build or credentials."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[1]


class ReleaseIdentity(unittest.TestCase):
    def test_manifest_and_packagers_share_public_version(self):
        manifest = tomllib.loads((ROOT / 'Cargo.toml').read_text())
        package = manifest['package']
        public = package['metadata']['entropy']['public-version']
        self.assertRegex(public, r'^\d+\.\d+(?:\.\d+)?$')
        internal = public + '.0' if public.count('.') == 1 else public
        self.assertEqual(package['version'], internal)
        result = subprocess.run(['sh', 'scripts/public_version.sh'], cwd=ROOT,
                                capture_output=True, text=True, check=True)
        self.assertEqual(result.stdout.strip(), public)
        for path in ('Taskfile.yml', 'scripts/build_macos_app.sh',
                     'scripts/build_linux_appimage.sh'):
            self.assertIn('scripts/public_version.sh', (ROOT / path).read_text())
        self.assertIn('cargoToml.package.metadata.entropy.public-version',
                      (ROOT / 'flake.nix').read_text())
        lock = tomllib.loads((ROOT / 'Cargo.lock').read_text())
        self.assertEqual(next(p['version'] for p in lock['package']
                              if p['name'] == package['name']), internal)

    def test_build_identity_and_windows_strings_use_public_version(self):
        build = (ROOT / 'build.rs').read_text()
        self.assertIn('cargo:rustc-env=ENTROPY_PUBLIC_VERSION=', build)
        self.assertIn('record_source_revision(&public_version)', build)
        self.assertIn('format!("v{public_version}")', build)
        self.assertIn('res.set("FileVersion", &public_version)', build)
        self.assertIn('res.set("ProductVersion", &public_version)', build)
        app = (ROOT / 'src/app_update.rs').read_text()
        label = app[app.index('pub(crate) fn current_build_label'):app.index('#[derive')]
        self.assertIn('ENTROPY_PUBLIC_VERSION', label)
        self.assertNotIn('CARGO_PKG_VERSION', label)

    def test_public_version_reader_keeps_historical_manifest_fallback(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'Cargo.toml'
            path.write_text('[package]\nname = "entropy"\nversion = "0.6.0"\n')
            result = subprocess.run(['sh', str(ROOT / 'scripts/public_version.sh'), str(path)],
                                    capture_output=True, text=True, check=True)
            self.assertEqual(result.stdout.strip(), '0.6.0')

    def test_public_version_reader_rejects_mismatched_metadata(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'Cargo.toml'
            path.write_text('[package]\nversion = "0.7.0"\n'
                            '[package.metadata.entropy]\npublic-version = "0.8"\n')
            result = subprocess.run(['sh', str(ROOT / 'scripts/public_version.sh'), str(path)],
                                    capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)


class LinuxVerifierVersions(unittest.TestCase):
    """Run the real verifiers with archive/container boundary fixtures only.

    No packages are built or installed; the arm64 fixture skips the unrelated
    x86_64-only AppImage launch. Paths, modes and metadata still pass through
    the real verifier assertions.
    """

    def run_verifiers(self, internal='0.7.0', public: str | None = '0.7', override=None,
                      expected='0.7', fault=None):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            scripts, tools, dist = (root / name for name in ('scripts', 'bin', 'dist'))
            for path in (scripts, tools, dist):
                path.mkdir()
            for name in ('test_linux_packages.sh', 'test_linux_install.sh',
                         'public_version.sh', 'pkg_version.sh'):
                shutil.copy2(ROOT / 'scripts' / name, scripts / name)
            manifest = f'[package]\nversion = "{internal}"\n'
            if public is not None:
                manifest += f'[package.metadata.entropy]\npublic-version = "{public}"\n'
            (root / 'Cargo.toml').write_text(manifest)
            versions = {fmt: subprocess.check_output(
                [str(scripts / 'pkg_version.sh'), fmt, expected], text=True).strip()
                for fmt in ('deb', 'rpm', 'archlinux')}
            for fmt, suffix in (('deb', '_amd64.deb'), ('rpm', '-1.aarch64.rpm'),
                                ('archlinux', '-1-aarch64.pkg.tar.zst')):
                version = '9.9' if fault == 'version' else versions[fmt]
                separator = '_' if fmt == 'deb' else '-'
                (dist / f'ergohaven-entropy{separator}{version}{suffix}').write_text('archive fixture only')
            paths = [
                'usr/bin/entropy', 'usr/share/applications/entropy.desktop',
                'usr/share/metainfo/com.ergohaven.entropy.metainfo.xml',
                'usr/lib/udev/rules.d/59-vial.rules',
                *(f'usr/share/icons/hicolor/{size}x{size}/apps/entropy.png'
                  for size in (16, 32, 48, 64, 128, 256)),
                'usr/share/doc/ergohaven-entropy/LICENSE',
            ]
            listing = []
            for index, path in enumerate(paths):
                if fault == 'missing' and index == 1:
                    continue
                mode = '-rwxr-xr-x' if index == 0 else '-rw-r--r--'
                if fault == 'mode' and index == 1:
                    mode = '-rw-rw-r--'
                listing.append(f'{mode} 0 0 0 0 {path}\n')
            bsdtar = '''#!/usr/bin/env python3
import os, sys
args = sys.argv[1:]
if '-' in args:
    sys.stdin.buffer.read()
if args == ['-tvf', '-']:
    print(os.environ['FIXTURE_LISTING'], end='')
elif args == ['-xOf', '-', './control']:
    print('Version: ' + os.environ['FIXTURE_DEB'])
elif args == ['-xOf', '-', '*59-vial.rules']:
    print('wrong marker' if os.environ['FIXTURE_FAULT'] == 'marker' else '# Entropy keyboard access v3')
elif args[0] == '-xOf' and args[-1] == '.PKGINFO':
    print('pkgver = ' + os.environ['FIXTURE_ARCH'] + '-1')
elif args[0] == '-xOf' and args[-1] in ('data.tar*', 'control.tar*'):
    print('mock archive stream')
else:
    raise SystemExit('unexpected bsdtar fixture arguments: ' + repr(args))
'''
            docker = '''#!/usr/bin/env python3
import json, os, sys
with open(os.environ['FIXTURE_DOCKER_LOG'], 'a') as stream:
    stream.write(json.dumps(sys.argv[1:]) + '\\n')
'''
            for name, content in (('bsdtar', bsdtar), ('docker', docker),
                                  ('uname', '#!/bin/sh\nprintf "aarch64\\n"\n')):
                tool = tools / name
                tool.write_text(content)
                tool.chmod(0o755)
            env = dict(os.environ, PATH=str(tools) + os.pathsep + os.defpath,
                       FIXTURE_LISTING=''.join(listing), FIXTURE_FAULT=fault or '',
                       FIXTURE_DEB='9.9' if fault == 'version' else versions['deb'],
                       FIXTURE_ARCH='9.9' if fault == 'version' else versions['archlinux'],
                       FIXTURE_DOCKER_LOG=str(root / 'docker.jsonl'))
            env.pop('VERSION', None)
            if override is not None:
                env['VERSION'] = override
            package = subprocess.run(['bash', str(scripts / 'test_linux_packages.sh'), str(dist)],
                                     env=env, text=True, capture_output=True)
            if fault is not None:
                self.assertNotEqual(package.returncode, 0, package.stdout)
                diagnostic = {'version': "version is '9.9'", 'missing': 'missing usr/share/applications/entropy.desktop',
                              'mode': 'has mode -rw-rw-r--', 'marker': 'udev rule lacks'}[fault]
                self.assertEqual(package.stderr.count(diagnostic), 3, package.stderr)
                return
            with self.subTest(verifier='packages'):
                self.assertEqual(package.returncode, 0, package.stdout + package.stderr)
            install = subprocess.run(['bash', str(scripts / 'test_linux_install.sh'), str(dist)],
                                     env=env, text=True, capture_output=True)
            self.assertEqual(install.returncode, 0, install.stdout + install.stderr)
            calls = [json.loads(line) for line in (root / 'docker.jsonl').read_text().splitlines()]
            self.assertEqual(len(calls), 3)
            for fmt, args in zip(('deb', 'rpm', 'archlinux'), calls):
                for key, version in (('EXPECTED', expected), ('RC', expected.split('-')[0] + '-rc.1'),
                                     ('RC2', expected.split('-')[0] + '-rc.2'), ('STABLE', expected.split('-')[0])):
                    mapped = subprocess.check_output(
                        [str(scripts / 'pkg_version.sh'), fmt, version], text=True).strip()
                    self.assertIn(f'PKG_{key}={mapped}', args)

    def test_public_default_versions_match_package_metadata_and_install_expectations(self):
        self.run_verifiers()
        self.run_verifiers(internal='0.7.1', public='0.7.1', expected='0.7.1')

    def test_historical_three_component_manifest_fallback(self):
        self.run_verifiers(internal='0.6.0', public=None, expected='0.6.0')

    def test_explicit_versions_keep_stable_and_prerelease_overrides(self):
        for version in ('0.8', '0.8.1', '0.8-rc.1', '0.8.1-rc.2'):
            with self.subTest(version=version):
                self.run_verifiers(override=version, expected=version)

    def test_incorrect_metadata_missing_paths_modes_and_udev_marker_still_reject(self):
        for fault in ('version', 'missing', 'mode', 'marker'):
            with self.subTest(fault=fault):
                self.run_verifiers(fault=fault)


if __name__ == '__main__':
    unittest.main()
