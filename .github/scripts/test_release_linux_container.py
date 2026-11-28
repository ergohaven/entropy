#!/usr/bin/env python3
"""Release assets use the pinned Linux owner and stable public filenames.

The shell fixtures exercise collection/renaming, not package compilation or
installation. Container/package qualification remains in release-validation.
"""
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import textwrap
import unittest

ROOT = Path(__file__).resolve().parents[2]


def block(text, heading):
    match = re.search(r'^  ' + re.escape(heading) + r':\n(.*?)(?=^  [a-z][a-z:_-]*:\n|\Z)', text, re.M | re.S)
    if match is None:
        raise AssertionError('missing task/job: ' + heading)
    return match.group(1)


class LinuxReleaseContainer(unittest.TestCase):
    def setUp(self):
        self.release = (ROOT / '.github/workflows/release.yml').read_text()
        self.build = block((ROOT / '.github/workflows/release-validation.yml').read_text().split('\njobs:\n', 1)[1], 'container')
        self.tasks = (ROOT / 'Taskfile.yml').read_text()
        self.package = next(s for s in self.build.split('\n      - ')
                            if 'name: Stage qualified Linux release artifacts\n' in s)
        self.compile = next(s for s in self.build.split('\n      - ')
                            if 'name: Build Linux artifacts twice in the container\n' in s)
        self.shell = self.compile.split('run: ', 1)[1].strip() + '\n' + textwrap.dedent(self.package.split('run: |\n', 1)[1])

    def test_appimage_container_target_reuses_the_existing_task_owners(self):
        target = block(self.tasks, 'docker:appimage')
        self.assertIn('    deps: [docker:image]\n', target)
        self.assertIn('      - task: docker:run\n', target)
        self.assertIn('        vars: {TASKS: linux:appimage}\n', target)
        self.assertNotIn('docker run', target)
        self.assertNotIn('cargo build', target)
        self.assertNotIn('linux:all', target)
        runner = block(self.tasks, 'docker:run')
        self.assertIn('-e VERSION -e SOURCE_DATE_EPOCH -e DIST', runner)
        self.assertIn('task {{.TASKS}}', runner)

    def test_linux_container_target_builds_packages_and_appimage(self):
        target = block(self.tasks, 'docker:linux')
        self.assertIn('    deps: [docker:image]\n', target)
        self.assertIn('      - task: docker:run\n', target)
        self.assertIn('        vars: {TASKS: linux:all}\n', target)
        self.assertNotIn('docker run', target)
        self.assertNotIn('cargo build', target)

    def test_linux_release_uses_only_the_pinned_container_packager(self):
        steps = self.build.split('\n      - ')
        installers = [s for s in steps if 'uses: arduino/setup-task@v2' in s]
        self.assertEqual(len(installers), 1)
        self.assertIn('version: 3.44.0', installers[0])
        self.assertNotIn('ERGOHAVEN_RELEASE_TOKEN', installers[0])
        self.assertNotIn('MACOS_CERTIFICATE', installers[0])
        packages = [s for s in steps if 'name: Stage qualified Linux release artifacts\n' in s]
        self.assertEqual(len(packages), 1)
        self.assertIn('run: task docker:repro', self.compile)
        self.assertIn('cp target/repro/a/linux/* dist/linux/', self.build)
        self.assertLess(self.build.index('task docker:repro'), self.build.index('Stage qualified Linux release artifacts'))
        self.assertNotIn('if: always()', self.package)
        self.assertIn('prefix="entropy-v$(sh scripts/public_version.sh)"', self.package)
        for copy in [
            'cp "dist/linux/$prefix-x86_64.AppImage" dist/release/',
            'cp dist/linux/*.deb "dist/release/$prefix-amd64.deb"',
            'cp dist/linux/*.rpm "dist/release/$prefix-x86_64.rpm"',
            'cp dist/linux/*.pkg.tar.zst "dist/release/$prefix-x86_64.pkg.tar.zst"',
        ]:
            self.assertIn(copy, self.package)
        self.assertIn('scripts/glibc_baseline.sh target/release/entropy', self.package)
        self.assertNotIn('scripts/build_linux_appimage.sh', self.package)
        self.assertNotIn('Install Linux dependencies', self.build)
        self.assertNotIn('secrets.', self.package)
        self.assertNotIn('cargo build', self.build)
        self.assertNotIn('rust-toolchain', self.build)
        self.assertNotIn('task docker:linux', self.release)

    def run_collection_fixture(self, tag, *, missing=False, duplicate=False, task_exit=0):
        with tempfile.TemporaryDirectory(prefix='release-linux-', dir=os.environ.get('TMPDIR')) as tmp:
            root = Path(tmp)
            for directory in ['dist/linux', 'bin', 'scripts', 'target/release']:
                (root / directory).mkdir(parents=True)
            shutil.copy2(ROOT / 'scripts/glibc_baseline.sh', root / 'scripts/glibc_baseline.sh')
            # Collection-only fixture: keep historical RC filename cases without
            # claiming that the stable candidate planner accepts RC versions.
            (root / 'scripts/public_version.sh').write_text('printf "%s\\n" "$FIXTURE_PUBLIC_VERSION"\n')
            fixture_binary = shutil.which('true')
            self.assertIsNotNone(fixture_binary)
            assert fixture_binary is not None
            shutil.copy2(fixture_binary, root / 'target/release/entropy')
            task = root / 'bin/task'
            task.write_text('#!/bin/sh\nset -eu\n[ "$#" -eq 1 ]\n[ "$1" = docker:repro ]\n'
                            'version="$(sh scripts/public_version.sh)"\n'
                            'printf "%s" "$version" > task-version\nexit "${FIXTURE_TASK_EXIT:-0}"\n')
            task.chmod(0o755)
            version = tag[1:]

            expected = {}
            for fmt, suffix in [('deb', 'amd64.deb'), ('rpm', 'x86_64.rpm'),
                                ('archlinux', 'x86_64.pkg.tar.zst')]:
                mapped = subprocess.check_output([str(ROOT / 'scripts/pkg_version.sh'), fmt, version], text=True).strip()
                extension = suffix.split('.', 1)[1]
                payload = f'collection fixture only: {fmt} {mapped}'.encode()
                path = root / f'dist/linux/ergohaven-entropy_{mapped}_amd64.{extension}'
                if not (missing and fmt == 'deb'):
                    path.write_bytes(payload)
                if duplicate and fmt == 'deb':
                    (path.parent / 'stale.deb').write_bytes(b'ambiguous collection fixture')
                expected[f'entropy-{tag}-{suffix}'] = payload
            appimage = f'entropy-{tag}-x86_64.AppImage'
            expected[appimage] = b'AppImage collection fixture only'
            (root / 'dist/linux' / appimage).write_bytes(expected[appimage])
            env = dict(os.environ, PATH=str(root / 'bin') + os.pathsep + os.environ['PATH'],
                       GITHUB_REF_NAME=tag, FIXTURE_PUBLIC_VERSION=version, FIXTURE_TASK_EXIT=str(task_exit))
            result = subprocess.run(['bash', '-e', '-o', 'pipefail', '-c', self.shell], cwd=root,
                                    env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            if missing or duplicate or task_exit:
                self.assertNotEqual(result.returncode, 0)
                if task_exit:
                    self.assertFalse((root / 'dist/release').exists())
                return
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual((root / 'task-version').read_text(), version)
            files = {f.name: f.read_bytes() for f in (root / 'dist/release').iterdir()}
            self.assertEqual(files, expected)
            self.assertTrue(all('~' not in name for name in files))

    def test_workflow_collection_keeps_bytes_and_public_names(self):
        for tag in ['v0.6', 'v0.6-rc.1', 'v0.5.0', 'v0.5.0-rc.1']:
            with self.subTest(tag=tag):
                self.run_collection_fixture(tag)

    def test_workflow_collection_rejects_missing_or_ambiguous_packages(self):
        for failure in ['missing', 'duplicate']:
            with self.subTest(failure=failure):
                self.run_collection_fixture('v0.6', **{failure: True})

    def test_failed_packager_cannot_collect_stale_assets(self):
        self.run_collection_fixture('v0.6', task_exit=27)


if __name__ == '__main__':
    unittest.main(verbosity=2)
