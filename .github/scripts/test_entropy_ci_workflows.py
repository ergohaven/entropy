"""Guard the Entropy CI selection/proof wiring without third-party packages."""
from pathlib import Path
import os
import re
import textwrap
from unittest.mock import patch
import unittest
import json

ROOT = Path(__file__).resolve().parents[2]


def workflow(name):
    return (ROOT / '.github/workflows' / name).read_text()


def job(text, name):
    match = re.search(r'^  ' + re.escape(name) + r':\n(.*?)(?=^  [\w-]+:|\Z)', text, re.M | re.S)
    if match is None:
        raise AssertionError('Missing owning job: ' + name)
    return match.group(1)


class EntropyCIContract(unittest.TestCase):
    def test_build_preserves_direct_main_push_validation(self):
        text = workflow('build.yml')
        self.assertIn('push:\n    branches: [main]', text)
        self.assertNotIn('disabled-auto-build', text)

    def test_main_and_rynk_prs_run_without_duplicate_integration_push_builds(self):
        for name in ('build.yml', 'nix.yml'):
            with self.subTest(workflow=name):
                text = workflow(name)
                self.assertRegex(text, r'(?m)^  pull_request:\n    branches: \[main, rynk\]$')
                self.assertRegex(text, r'(?m)^  push:\n    branches: \[main\]$')
                self.assertIn('  workflow_dispatch:', text)

    def test_build_plan_is_cheap_and_checks_policy(self):
        owner = job(workflow('build.yml'), 'plan')
        self.assertIn('fetch-depth: 0', owner)
        self.assertIn('unittest discover', owner)
        self.assertIn('ci_policy.py plan --profile build', owner)
        self.assertIn('actions: read', owner)
        self.assertIn('pull-requests: read', owner)
        self.assertNotIn('cargo ', owner)

    def test_build_gates_all_expensive_owners(self):
        text = workflow('build.yml')
        for name in ('build', 'container', 'packages'):
            with self.subTest(job=name):
                owner = job(text, name)
                self.assertIn("needs.plan.outputs.run == 'true'", owner)
                self.assertIn('needs:', owner)
        self.assertIn('needs: [plan, container]', job(text, 'packages'))

    def test_build_retains_platform_and_packaging_coverage(self):
        text = workflow('build.yml')
        for marker in ('windows-latest', 'macos-15-intel', 'macos-15', 'ubuntu-latest',
                       'cargo test --all-targets', 'task docker:repro', 'scripts/test_linux_install.sh',
                       'test_macos_stable_signing.sh'):
            with self.subTest(marker=marker):
                self.assertIn(marker, text)

    def test_build_proof_requires_complete_full_graph(self):
        owner = job(workflow('build.yml'), 'complete')
        self.assertIn('needs: [plan, build, container, packages]', owner)
        self.assertIn('always()', owner)
        self.assertIn('toJSON(needs)', owner)
        self.assertIn("needs.plan.outputs.run == 'true'", owner)
        self.assertIn('ci_policy.py record --profile build', owner)
        self.assertIn('ci-proof-build-${{ github.run_id }}-${{ github.run_attempt }}', owner)
        self.assertIn("('build', 'container', 'packages')", owner)

    def test_nix_plan_and_both_systems_remain(self):
        text = workflow('nix.yml')
        owner = job(text, 'plan')
        self.assertIn('ci_policy.py plan --profile nix', owner)
        self.assertIn('actions: read', owner)
        self.assertIn('pull-requests: read', owner)
        self.assertIn("needs.plan.outputs.run == 'true'", job(text, 'flake'))
        for marker in ('x86_64-linux', 'aarch64-linux', 'nix flake check', 'module-eval', 'entropy-ibus-engine'):
            self.assertIn(marker, text)

    def test_nix_completion_is_explicit_and_does_not_prove_skips(self):
        owner = job(workflow('nix.yml'), 'complete')
        self.assertIn('needs: [plan, flake]', owner)
        self.assertIn('always()', owner)
        self.assertIn('toJSON(needs)', owner)
        self.assertIn("needs.plan.outputs.run == 'true'", owner)
        self.assertIn('ci_policy.py record --profile nix', owner)
        self.assertIn('ci-proof-nix-${{ github.run_id }}-${{ github.run_attempt }}', owner)

    def test_only_same_event_and_ref_are_cancelled(self):
        for name in ('build.yml', 'nix.yml'):
            with self.subTest(workflow=name):
                text = workflow(name)
                self.assertIn('github.event_name', text)
                self.assertIn('github.ref', text)
                self.assertIn('cancel-in-progress: true', text)
    @staticmethod
    def completion_code(name):
        owner = job(workflow(name), 'complete')
        match = re.search(r"python3 - <<'PY'\n(.*?)^          PY$", owner, re.M | re.S)
        if match is None:
            raise AssertionError('Missing executable completion gate')
        return textwrap.dedent(match.group(1))

    @staticmethod
    def accepts(code, mode, plan, workers):
        needs = {'plan': {'result': plan}, **{name: {'result': result} for name, result in workers.items()}}
        with patch.dict(os.environ, {'NEEDS_JSON': json.dumps(needs), 'FULL_VALIDATION': mode}):
            try:
                exec(compile(code, '<actual-workflow-gate>', 'exec'), {})
            except SystemExit:
                return False
        return True

    def test_actual_completion_gates_reject_incomplete_or_failed_coverage(self):
        for filename, names in [('build.yml', ('build', 'container', 'packages')), ('nix.yml', ('flake',))]:
            code = self.completion_code(filename)
            good = dict.fromkeys(names, 'success')
            skipped = dict.fromkeys(names, 'skipped')
            with self.subTest(workflow=filename):
                self.assertTrue(self.accepts(code, 'true', 'success', good))
                self.assertTrue(self.accepts(code, 'false', 'success', skipped))
                self.assertFalse(self.accepts(code, '', 'success', skipped))
                self.assertFalse(self.accepts(code, 'true', 'failure', good))
                self.assertFalse(self.accepts(code, 'true', 'success', skipped))
                self.assertFalse(self.accepts(code, 'false', 'success', good))
                for name in names:
                    for state in ('failure', 'cancelled', 'skipped'):
                        bad = {**good, name: state}
                        self.assertFalse(self.accepts(code, 'true', 'success', bad))

    def test_gate_assertions_detect_isolated_removal_of_failures(self):
        for filename, names in [('build.yml', ('build', 'container', 'packages')), ('nix.yml', ('flake',))]:
            code = self.completion_code(filename)
            mutant = re.sub(r'(?m)^(\s*)raise SystemExit\(.*\)$', r'\1pass', code)
            self.assertNotEqual(code, mutant)
            bad = dict.fromkeys(names, 'failure')
            self.assertFalse(self.accepts(code, 'true', 'success', bad))
            # The same rejection assertion fails when only its real gate is removed.
            with self.assertRaises(AssertionError):
                self.assertFalse(self.accepts(mutant, 'true', 'success', bad))


if __name__ == '__main__':
    unittest.main()
