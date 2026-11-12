import json
from pathlib import Path
import tempfile
import unittest
from arm_two_case_diagnostic import SIGNAL_TEST, RESIDENT_TEST, stack_command, resident_command, classify_stack, classify_resident, binary_from_metadata


class DiagnosticContracts(unittest.TestCase):
    def test_debugger_captures_every_thread_at_native_stop(self):
        command = stack_command(Path("binary"))
        self.assertIn("thread backtrace all", command)
        self.assertIn("register read", command)
        self.assertEqual(command[-4:], [SIGNAL_TEST, "--exact", "--nocapture", "--test-threads=1"])

    def test_resident_reuses_metadata_and_exact_original_case(self):
        command = resident_command(Path("metadata"))
        self.assertIn("--binaries-metadata", command)
        self.assertIn("--cargo-metadata", command)
        self.assertNotIn("--all-targets", command)
        self.assertNotIn("--retries", command)
        self.assertEqual(command[-2:], ["-E", "test(=" + RESIDENT_TEST + ")"])

    def test_stack_capture_not_product_pass(self):
        self.assertEqual(classify_stack("stop reason = EXC_BREAKPOINT\nframe #0: AppKit", 1), "native_stop_with_stack")
        self.assertEqual(classify_stack("stop reason = SIGTRAP", 1), "incomplete")
        self.assertEqual(classify_stack("could not launch process", 1), "incomplete")

    def test_debugger_pass_requires_executed_original_test(self):
        text = SIGNAL_TEST + "\ntest result: ok. 1 passed; 0 failed;"
        self.assertEqual(classify_stack(text, 0), "one_test_passed_under_debugger")
        self.assertEqual(classify_stack(text.replace("1 passed", "0 passed"), 0), "incomplete")

    def test_resident_terminal_results_and_zero_selection(self):
        for code, summary, expected in [(0, "1 test run: 1 passed", "one_test_passed"), (100, "1 test run: 0 passed, 1 failed", "one_test_failed"), (0, "0 tests run: 0 passed", "incomplete")]:
            self.assertEqual(classify_resident(RESIDENT_TEST + "\nSummary [ 5.0s] " + summary, code), expected)

    def test_metadata_fails_closed(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root / "binaries-metadata.json").write_text(json.dumps({"rust-binaries": {}}))
            with self.assertRaises(ValueError):
                binary_from_metadata(root)


if __name__ == "__main__":
    unittest.main()
