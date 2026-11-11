"""Source-wiring guard for the accepted non-administrative backup policy.

These checks establish absence of an elevated helper, not native Windows I/O
or power-loss durability. Runtime file/publication checks live in Rust.
"""
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[2]


class BackupPrivilegeContract(unittest.TestCase):
    def test_backup_does_not_register_or_dispatch_an_elevated_helper(self):
        main = (ROOT / 'src/main.rs').read_text()
        self.assertIsNone(re.search(r'\bmod\s+backup_volume\s*;', main))
        self.assertNotIn('run_helper_if_requested', main)

    def test_native_backup_policy_is_exercised_on_windows_and_macos(self):
        workflow = (ROOT / '.github/workflows/build.yml').read_text()
        match = re.search(
            r'      - name: Test native resident backup publication\n(.*?)(?=      - name:|\Z)',
            workflow, re.S)
        if match is None:
            self.fail('Missing native backup-publication test owner')
        step = match.group(1)
        self.assertIn("if: matrix.kind != 'linux'", step)
        self.assertIn(
            'cargo test --all-targets app::entlayout::backup_durability::tests -- --test-threads=1',
            step)

    def test_backup_publication_has_no_volume_flush_or_test_only_bypass(self):
        backup = (ROOT / 'src/entlayout_backup.rs').read_text()
        for removed_boundary in ('crate::backup_volume', 'BackupNamespaceLease',
                                 'portable_test_volume_barrier'):
            with self.subTest(boundary=removed_boundary):
                self.assertNotIn(removed_boundary, backup)
        init = (ROOT / 'src/app_init.rs').read_text()
        self.assertNotIn('with_portable_app_test_volume_barrier', init)


if __name__ == '__main__':
    unittest.main()
