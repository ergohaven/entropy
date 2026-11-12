import unittest
from resident100_diagnostic import classify, COMMAND, TEST


class DiagnosticReceiptTests(unittest.TestCase):
    def sample(self, summary):
        return "PASS " + TEST + "\nSummary [ 96.550s] " + summary + "\n"

    def test_exact_one_pass(self):
        self.assertEqual(classify(self.sample("1 test run: 1 passed, 23 skipped"), 0), "one_test_passed")

    def test_exact_one_failure(self):
        self.assertEqual(classify(self.sample("1 test run: 0 passed, 1 failed, 23 skipped"), 100), "one_test_failed")

    def test_zero_or_broad_selection_rejected(self):
        for summary in ("0 tests run: 0 passed", "2082 tests run: 2080 passed, 2 failed"):
            self.assertEqual(classify(self.sample(summary), 0), "incomplete")

    def test_missing_duplicate_wrong_test_or_exit_rejected(self):
        good = self.sample("1 test run: 1 passed")
        for text, code in (("", 0), (good + good, 0), (good.replace(TEST, "unrelated"), 0), (good, 124)):
            self.assertEqual(classify(text, code), "incomplete")

    def test_original_name_and_process_isolation_retained(self):
        self.assertEqual(COMMAND[-2:], ["-E", "test(=" + TEST + ")"])
        self.assertEqual(COMMAND[COMMAND.index("--test-threads") + 1], "4")
        self.assertIn("--locked", COMMAND)
        self.assertNotIn("--retries", COMMAND)


if __name__ == "__main__":
    unittest.main()
