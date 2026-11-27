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

    def test_ordinary_build_only_dispatches_the_artifact_matrix(self):
        text = workflow('build.yml')
        self.assertRegex(text, r'(?m)^  pull_request:\n    branches: \[main, rynk\]$')
        self.assertIn('  workflow_dispatch:', text)
        self.assertNotIn('workflow_call:', text)
        owner = job(text, 'build')
        self.assertIn("if: github.event_name == 'workflow_dispatch'", owner)
        self.assertNotIn('needs:', owner)
        for command in ('cargo test', 'cargo nextest', 'docker:', 'nix ', 'test_macos',
                        'ci_policy.py', 'setup-task', 'secrets.'):
            self.assertNotIn(command, owner)
        self.assertEqual(re.findall(r'            artifact: (.+)', owner), [
            'entropy-linux', 'entropy-windows', 'entropy-macos-arm64', 'entropy-macos-x86_64'])
        self.assertIn('binary: dist/test/*.AppImage', owner)
        self.assertIn('scripts/build_linux_appimage.sh "test-${GITHUB_SHA:0:8}"', owner)
        self.assertIn('cargo build --release --locked --bin entropy', owner)
        self.assertIn('TARGET="${{ matrix.target }}" scripts/build_macos_app.sh', owner)
        self.assertIn('if-no-files-found: error', owner)
        self.assertEqual(owner.count('cargo build'), 1)

    def test_source_events_keep_only_cheap_workflow_checks(self):
        text = workflow('build.yml')
        self.assertEqual(set(re.findall(r'^  ([a-z-]+):$', text.split('jobs:\n')[1], re.M)),
                         {'contracts', 'build'})
        owner = job(text, 'contracts')
        self.assertIn("if: github.event_name != 'workflow_dispatch'", owner)
        self.assertIn("python3 -m unittest discover -s .github/scripts -p 'test_*.py' -v", owner)
        for command in ('cargo ', 'nix ', 'docker:', 'ci_policy.py'):
            self.assertNotIn(command, owner)

    def test_source_contract_checkout_keeps_full_history_without_credentials(self):
        for filename, name in (('build.yml', 'contracts'), ('release-validation.yml', 'plan'), ('nix.yml', 'plan')):
            with self.subTest(workflow=filename, job=name):
                owner = job(workflow(filename), name)
                checkout = re.search(r'      - uses: actions/checkout@[^\n]+\n(.*?)(?=      - |\Z)', owner, re.S)
                if checkout is None:
                    self.fail('Missing source-contract checkout')
                self.assertIn('          fetch-depth: 0\n', checkout.group(1))
                self.assertIn('          persist-credentials: false\n', checkout.group(1))

    def test_full_validation_and_nix_are_release_only_reusable_workflows(self):
        gate = "if: github.event_name == 'push' && startsWith(github.ref, 'refs/tags/v0.')"
        for filename in ('release-validation.yml', 'nix.yml'):
            text = workflow(filename)
            self.assertIn('on:\n  workflow_call:\n', text)
            for trigger in ('workflow_dispatch:', 'pull_request:', '  push:'):
                self.assertNotIn(trigger, text)
            owner = job(text, 'plan')
            self.assertIn(gate, owner)
            self.assertIn("run: 'true'", owner)
            self.assertIn('unittest discover', owner)
            self.assertNotIn('ci_policy.py plan', owner)
            self.assertNotIn('secrets.', owner)

    def test_release_requires_full_validation_and_nix_before_publication(self):
        release = workflow('release.yml')
        gate = "${{ github.repository == 'ergohaven/entropy' && github.event_name == 'push' && startsWith(github.ref, 'refs/tags/v0.') }}"
        for name, filename in [('validate', 'release-validation.yml'), ('nix', 'nix.yml')]:
            owner = job(release, name)
            self.assertIn('if: ' + gate, owner)
            self.assertIn('uses: ./.github/workflows/' + filename, owner)
            self.assertNotIn('secrets:', owner)
        self.assertIn('needs: [build, validate, nix]', job(release, 'release'))

    def test_release_workers_are_preserved_byte_for_byte(self):
        baseline = json.loads((ROOT / '.github/scripts/fixtures/release_validation_workers.json').read_text())
        self.assertEqual(set(baseline['workers']), {'release-validation.yml', 'nix.yml'})
        self.assertEqual(set(baseline['workers']['release-validation.yml']), {'build', 'container', 'packages'})
        self.assertEqual(set(baseline['workers']['nix.yml']), {'flake'})
        import hashlib
        for filename, jobs in baseline['workers'].items():
            for name, digest in jobs.items():
                with self.subTest(workflow=filename, job=name):
                    self.assertEqual(hashlib.sha256(job(workflow(filename), name).encode()).hexdigest(), digest)

    def test_build_gates_all_expensive_owners(self):
        text = workflow('release-validation.yml')
        for name in ('build', 'container', 'packages'):
            with self.subTest(job=name):
                owner = job(text, name)
                self.assertIn("needs.plan.outputs.run == 'true'", owner)
                self.assertIn('needs:', owner)
        self.assertIn('needs: [plan, container]', job(text, 'packages'))

    def test_build_retains_platform_and_packaging_coverage(self):
        text = workflow('release-validation.yml')
        for marker in ('windows-latest', 'macos-15-intel', 'macos-15', 'ubuntu-latest',
                       'cargo test --all-targets', 'task docker:repro', 'scripts/test_linux_install.sh',
                       'test_macos_stable_signing.sh'):
            with self.subTest(marker=marker):
                self.assertIn(marker, text)

    def test_macos_executes_native_baseline_regressions(self):
        owner = job(workflow('release-validation.yml'), 'build')
        match = re.search(
            r'      - name: Test macOS baseline regressions\n(.*?)(?=      - name:|\Z)',
            owner, re.S)
        if match is None:
            self.fail('Missing macOS baseline regression step')
        step = match.group(1)
        self.assertIn("if: matrix.kind == 'macos'", step)
        required = (
            'application_layouts::tests::catalog_upgrade_adds_photoshop_and_only_fills_untouched_second_layers',
            'application_layouts::tests::catalog_revision_five_removes_only_untouched_retired_layouts',
            'application_layouts::tests::catalog_revision_three_adds_third_layer_only_to_stock_profiles',
            'application_layouts::tests::changed_preset_command_loses_its_old_icon_and_visuals_are_transmitted',
            'qmk_hid_host::fresh_dedicated_owner_never_receives_previous_profile_snapshot',
            'app::app_lifecycle::usb_hotplug_tests::tray_hidden_startup_discovers_and_connects_without_restoring_window',
            'i18n::tests::embedded_catalogs_have_unique_supported_keys_and_readable_values',
            'i18n::tests::indexed_catalog_keeps_sections_and_quoted_values',
            'i18n::tests::indexed_catalog_preserves_borrowed_escapes_and_decodes_owned_strings',
        )
        commands = [line.strip() for line in step.splitlines()
                    if line.strip().startswith('cargo test ')]
        self.assertEqual(commands, [
            f'cargo test --all-targets {name} -- --exact --test-threads=1'
            for name in required])

    def test_macos_runs_the_complete_native_suite_in_parallel(self):
        owner = job(workflow('release-validation.yml'), 'build')
        self.assertIn('uses: taiki-e/install-action@v2', owner)
        self.assertIn('tool: cargo-nextest@0.9.150', owner)
        match = re.search(
            r'      - name: Test full macOS suite in parallel\n(.*?)(?=      - name:|\Z)',
            owner, re.S)
        if match is None:
            self.fail('Missing full parallel macOS suite')
        self.assertEqual(textwrap.dedent(match.group(1)).strip(),
                         "if: matrix.kind == 'macos'\n"
                         'run: cargo nextest run --locked --all-targets --test-threads 4 --no-fail-fast')

    def test_build_proof_requires_complete_full_graph(self):
        owner = job(workflow('release-validation.yml'), 'complete')
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
        self.assertIn("run: 'true'", owner)
        self.assertNotIn('ci_policy.py plan', owner)
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
        for name in ('build.yml', 'release-validation.yml', 'nix.yml'):
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
        for filename, names in [('release-validation.yml', ('build', 'container', 'packages')), ('nix.yml', ('flake',))]:
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
        for filename, names in [('release-validation.yml', ('build', 'container', 'packages')), ('nix.yml', ('flake',))]:
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
