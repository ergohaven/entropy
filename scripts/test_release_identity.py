#!/usr/bin/env python3
"""Cheap release identity contracts; no application build or credentials."""
from pathlib import Path
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


if __name__ == '__main__':
    unittest.main()
