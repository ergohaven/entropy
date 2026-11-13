import copy
import importlib.util
import json
import os
from pathlib import Path
from unittest import mock
import subprocess
import sys
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location('diagnostic', Path(__file__).with_name('macos_bounded_diagnostic.py'))
assert SPEC is not None and SPEC.loader is not None
D = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(D)


class DiagnosticContracts(unittest.TestCase):
    def listing(self, names=D.CASES):
        return {'rust-suites': {'entropy': {'testcases': {
            name: {'filter-match': {'status': 'matches'}} for name in names}}}}

    def test_checkout_history_and_preflight_precede_compilation(self):
        workflow = Path(__file__).parents[1] / '.github/workflows/build.yml'
        text = workflow.read_text()
        self.assertIn('fetch-depth: 3', text)
        self.assertLess(text.index('scripts/macos_bounded_diagnostic.py --preflight'),
                        text.index('dtolnay/rust-toolchain@stable'))
        self.assertLess(text.index('scripts/test_macos_bounded_diagnostic.py -v'),
                        text.index('dtolnay/rust-toolchain@stable'))

    def test_history_pins_source_parent_and_original_base_separately(self):
        with mock.patch.object(D.subprocess, 'check_output', side_effect=[
                'source\n', D.DIAGNOSTIC_PARENT + '\n', D.BASE + '\n', '']):
            self.assertEqual(D.verify_history(Path.cwd(), 'source'), D.DIAGNOSTIC_PARENT)
        for values in [('wrong-source',), ('source', 'wrong-parent', D.BASE),
                       ('source', D.DIAGNOSTIC_PARENT, 'wrong-base'),
                       ('source', D.DIAGNOSTIC_PARENT, D.BASE, 'dirty')]:
            with mock.patch.object(D.subprocess, 'check_output', side_effect=values):
                with self.assertRaises(ValueError):
                    D.verify_history(Path.cwd(), 'source')

    def test_real_shallow_git_rejects_before_source_acceptance(self):
        root = Path(__file__).parents[1]
        head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
        with tempfile.TemporaryDirectory() as directory:
            shallow = Path(directory) / 'shallow-control'
            shallow.write_text(head + '\n')
            with mock.patch.dict(os.environ, {'GIT_SHALLOW_FILE': str(shallow)}):
                with self.assertRaisesRegex(ValueError, 'history unavailable'):
                    D.verify_history(root, head)

    def test_exact_selection(self):
        self.assertEqual(D.selected_names(json.dumps(self.listing())), sorted(D.CASES))

    def test_zero_or_broad_selection_rejects(self):
        for names in [(), D.CASES + ('unrelated',)]:
            with self.assertRaises(ValueError):
                D.selected_names(json.dumps(self.listing(names)))

    def test_missing_or_invalid_metadata_rejects(self):
        for text in ['{}', 'not-json']:
            with self.assertRaises((KeyError, ValueError)):
                D.selected_names(text)

    def sample(self, failed=False):
        rows = [f'{"SIGTRAP" if failed and i == 1 else "PASS"} [ 0.010s] ({i}/3) entropy {name}'
                for i, name in enumerate(D.CASES, 1)]
        summary = '3 tests run: 2 passed, 1 failed, 2108 skipped' if failed else '3 tests run: 3 passed, 2108 skipped'
        return '\n'.join(rows) + '\nSummary [0.020s] ' + summary

    def test_native_all_pass_summary_with_filtered_skips(self):
        self.assertEqual(D.terminal_results(self.sample())['passed'], 3)

    def test_signal_summary_and_repeated_failure_row(self):
        text = self.sample(True)
        text += '\n' + text.splitlines()[0]
        result = D.terminal_results(text)
        self.assertEqual(result['failed'], 1)
        self.assertEqual(result['skipped'], 2108)

    def test_missing_duplicate_summary_or_case_rejects(self):
        text = self.sample(True)
        for changed in [text.split('Summary')[0], text + '\n' + text.splitlines()[-1],
                        text.replace(D.CASES[2], 'other')]:
            with self.assertRaises(ValueError):
                D.terminal_results(changed)

    def test_timed_process_is_reaped(self):
        with tempfile.TemporaryDirectory() as directory:
            result = D.bounded([sys.executable, '-c', 'import time; time.sleep(30)'],
                               Path(directory) / 'timeout.log', 0.1)
            self.assertTrue(result['timed_out'])
            self.assertLess(result['exit_code'], 0)

    def test_nonzero_is_evidence_not_success(self):
        with tempfile.TemporaryDirectory() as directory:
            result = D.bounded([sys.executable, '-c', 'raise SystemExit(7)'],
                               Path(directory) / 'failure.log', 5)
            self.assertFalse(result['timed_out'])
            self.assertEqual(result['exit_code'], 7)


if __name__ == '__main__':
    unittest.main()
