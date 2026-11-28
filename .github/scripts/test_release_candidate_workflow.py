"""Candidate preparation and promotion must not compile the same product twice."""
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import textwrap
import unittest

ROOT = Path(__file__).resolve().parents[2]


def job(text, name):
    match = re.search(r'^  ' + re.escape(name) + r':\n(.*?)(?=^  [a-z][\w-]*:|\Z)', text, re.M | re.S)
    if match is None:
        raise AssertionError('Missing owning job: ' + name)
    return match.group(1)


class ReleaseCandidateWorkflow(unittest.TestCase):
    def setUp(self):
        self.release = (ROOT / '.github/workflows/release.yml').read_text()
        self.validation = (ROOT / '.github/workflows/release-validation.yml').read_text()

    def test_preparation_is_explicit_and_never_a_source_push_side_effect(self):
        self.assertIn('      prepare_release:\n', self.release)
        self.assertIn("github.ref == 'refs/heads/main'", job(self.release, 'plan'))
        self.assertIn('release_candidate.py plan', job(self.release, 'plan'))
        self.assertNotIn('    branches:', self.release)

    def test_tag_promotion_has_no_build_or_test_dependency(self):
        release = job(self.release, 'release')
        self.assertIn('    needs: plan\n', release)
        self.assertIn('release_candidate.py fetch', release)
        self.assertIn('release_candidate.py stage', release)
        self.assertIn('release_candidate.py publish', release)
        for command in ('cargo ', 'rustup ', 'swiftc ', 'task ', 'docker ', 'nix ', 'action-gh-release'):
            self.assertNotIn(command, release)
        self.assertIn('cancel-in-progress: false', release)
        self.assertIn('group: entropy-publish\n', release, 'Latest is one shared destination across all tags')

    def test_signers_reuse_the_qualified_macos_binary(self):
        build = job(self.release, 'build')
        self.assertIn('needs: [plan, validate]', build)
        self.assertIn('entropy-macos-${{ matrix.arch }}', build)
        self.assertIn('scripts/stage_macos_release_binary.py', build)
        self.assertNotIn('cargo ', build)
        self.assertNotIn('rust-toolchain', build)
        self.assertNotIn('rust-cache', build)
        self.assertEqual(re.findall(r'          - os: (.+)', build), ['macos-26', 'macos-26-intel'])
        self.assertLess(build.index('scripts/stage_macos_release_binary.py'), build.index('secrets.'))
        self.assertLess(build.index('swiftc '), build.index('secrets.'))

    def test_linux_and_windows_release_bytes_come_from_full_validation(self):
        self.assertIn('name: Upload qualified Windows release artifact', job(self.validation, 'build'))
        self.assertIn('name: Upload qualified Linux release artifacts', job(self.validation, 'container'))
        self.assertIn('task docker:repro', job(self.validation, 'container'))
        self.assertNotIn('task docker:linux', job(self.release, 'build'))
        self.assertIn('scripts/test_linux_install.sh', job(self.validation, 'packages'))

    def test_candidate_cannot_be_frozen_before_all_qualification(self):
        candidate = job(self.release, 'candidate')
        self.assertIn('needs: [plan, build, validate, nix]', candidate)
        for marker in ('release-assets-*', 'ci-proof-*', 'release-signing-evidence-*', 'release_candidate.py pack'):
            self.assertIn(marker, candidate)
        self.assertIn('name: release-candidate\n', candidate)
        self.assertIn('if-no-files-found: error', candidate)
        self.assertNotIn('contents: write', candidate)
        self.assertNotIn('secrets.', candidate)

    def stage_code(self, name):
        match = re.search(r'      - name: ' + re.escape(name) + r'\n.*?        run: \|\n(.*?)(?=      - name:)', self.validation, re.S)
        self.assertIsNotNone(match)
        assert match is not None
        return textwrap.dedent(match.group(1))

    def test_actual_windows_export_preserves_qualified_executable_bytes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'scripts').mkdir()
            (root / 'scripts/public_version.sh').write_text('printf "0.8\\n"\n')
            binary = root / 'target/release/entropy.exe'
            binary.parent.mkdir(parents=True)
            original = b'MZ-qualified-test-fixture\x00\xff'
            binary.write_bytes(original)
            subprocess.run(['bash', '-euc', self.stage_code('Stage qualified Windows release artifact')], cwd=root, check=True)
            output = root / 'dist/release/entropy-v0.8-windows-x86_64.exe'
            self.assertEqual(output.read_bytes(), original)

    def test_actual_linux_export_preserves_all_four_qualified_package_bytes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'scripts').mkdir()
            (root / 'scripts/public_version.sh').write_text('printf "0.8\\n"\n')
            source = root / 'dist/linux'
            shutil.copy2(ROOT / 'scripts/glibc_baseline.sh', root / 'scripts/glibc_baseline.sh')
            (root / 'target/release').mkdir(parents=True)
            executable = shutil.which('true')
            assert executable is not None
            shutil.copy2(executable, root / 'target/release/entropy')
            source.mkdir(parents=True)
            names = {'entropy-v0.8-x86_64.AppImage': 'entropy-v0.8-x86_64.AppImage',
                     'entropy_0.8_amd64.deb': 'entropy-v0.8-amd64.deb',
                     'entropy-0.8.x86_64.rpm': 'entropy-v0.8-x86_64.rpm',
                     'entropy-0.8-x86_64.pkg.tar.zst': 'entropy-v0.8-x86_64.pkg.tar.zst'}
            for index, name in enumerate(names):
                (source / name).write_bytes(bytes([index]) + b'-qualified-test-fixture')
            command = ['bash', '-euc', self.stage_code('Stage qualified Linux release artifacts')]
            subprocess.run(command, cwd=root, check=True)
            self.assertEqual({p.name for p in (root / 'dist/release').iterdir()}, set(names.values()))
            for before, after in names.items():
                self.assertEqual((source / before).read_bytes(), (root / 'dist/release' / after).read_bytes())
            (source / 'stale.deb').write_bytes(b'not an eligible second package')
            result = subprocess.run(command, cwd=root, capture_output=True)
            self.assertNotEqual(result.returncode, 0, 'ambiguous native package inputs must fail')


if __name__ == '__main__':
    unittest.main()
