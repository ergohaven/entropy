#!/usr/bin/env python3
"""Release Linux packages must use the existing pinned container owner."""
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[2]


def block(text, heading):
    match = re.search(r'^  ' + re.escape(heading) + r':\n(.*?)(?=^  [a-z][a-z:_-]*:\n|\Z)', text, re.M | re.S)
    if match is None:
        raise AssertionError('missing task/job: ' + heading)
    return match.group(1)


class LinuxReleaseContainer(unittest.TestCase):
    def setUp(self):
        self.build = block((ROOT / '.github/workflows/release.yml').read_text().split('\njobs:\n', 1)[1], 'build')
        self.tasks = (ROOT / 'Taskfile.yml').read_text()

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

    def test_linux_release_uses_only_the_pinned_container_packager(self):
        steps = self.build.split('\n      - ')
        installers = [s for s in steps if 'uses: arduino/setup-task@v2' in s]
        self.assertEqual(len(installers), 1)
        self.assertIn("if: matrix.kind == 'linux'", installers[0])
        self.assertIn('version: 3.44.0', installers[0])
        self.assertNotIn('secrets.', installers[0])
        packages = [s for s in steps if 'name: Package Linux artifact' in s]
        self.assertEqual(len(packages), 1)
        self.assertIn("if: matrix.kind == 'linux'", packages[0])
        self.assertIn('VERSION="${GITHUB_REF_NAME#v}" task docker:linux', packages[0])
        self.assertIn('prefix="entropy-${GITHUB_REF_NAME}"', packages[0])
        for copy in [
            'cp "dist/linux/$prefix-x86_64.AppImage" dist/release/',
            'cp dist/linux/*.deb "dist/release/$prefix-amd64.deb"',
            'cp dist/linux/*.rpm "dist/release/$prefix-x86_64.rpm"',
            'cp dist/linux/*.pkg.tar.zst "dist/release/$prefix-x86_64.pkg.tar.zst"',
        ]:
            self.assertIn(copy, packages[0])
        self.assertIn('scripts/glibc_baseline.sh target/release/entropy', packages[0])
        self.assertNotIn('scripts/build_linux_appimage.sh', packages[0])
        self.assertNotIn('Install Linux dependencies', self.build)
        for name in ['Install Rust', 'Cache Rust']:
            owner = next(s for s in steps if 'name: ' + name + '\n' in s)
            self.assertIn("if: matrix.kind != 'linux'", owner)


if __name__ == '__main__':
    unittest.main(verbosity=2)
