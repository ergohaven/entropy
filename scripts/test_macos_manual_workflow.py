#!/usr/bin/env python3
"""Fail-closed source contract for the secret-bearing manual workflow.

No YAML dependency: job boundaries are fixed two-space mapping keys; actionlint
separately validates the YAML and GitHub expressions. Exact gate assertions mean
an accidental OR, prefix match or input checkout ref cannot weaken the policy.
"""
import hashlib
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[1]


class ManualWorkflow(unittest.TestCase):
    def setUp(self):
        self.workflow = (ROOT / '.github/workflows/release.yml').read_text()
        self.jobs = dict(re.findall(
            r'^  ([a-z][a-z_-]*):\n(.*?)(?=^  [a-z][a-z_-]*:\n|\Z)',
            self.workflow.split('\njobs:\n', 1)[1], re.M | re.S))

    def gate(self, job):
        match = re.search(r'^    if: (.+)$', self.jobs[job], re.M)
        self.assertIsNotNone(match, f'{job} must have an explicit gate')
        assert match is not None
        return match.group(1)

    def test_only_opted_in_trusted_branch_can_run_manual_job(self):
        self.assertIn('macos-signing-test', self.jobs)
        self.assertEqual(self.gate('macos-signing-test'),
                         "${{ github.repository == 'ergohaven/entropy' && github.event_name == 'workflow_dispatch' && github.ref == 'refs/heads/ci/macos-signing-validation-pr195' && inputs.macos_signing_test == true }}")
        self.assertIn('      macos_signing_test:\n', self.workflow)
        self.assertIn('        type: boolean\n        default: false', self.workflow)
        self.assertNotIn('pull_request', self.workflow)
        self.assertEqual(set(self.jobs), {'build', 'macos-signing-test', 'release'})
        self.assertEqual(self.gate('build'),
                         "${{ github.repository == 'ergohaven/entropy' && github.event_name == 'push' && startsWith(github.ref, 'refs/tags/v0.') }}")
        self.assertEqual(self.gate('release'),
                         "${{ github.repository == 'ergohaven/entropy' && github.event_name == 'push' && startsWith(github.ref, 'refs/tags/v0.') }}")

    def test_manual_has_read_only_token_exact_checkout_and_two_macos_targets(self):
        self.assertIn('macos-signing-test', self.jobs)
        job = self.jobs['macos-signing-test']
        self.assertIn('    permissions:\n      contents: read\n', job)
        self.assertIn('          ref: ${{ github.sha }}\n          persist-credentials: false', job)
        self.assertNotIn('inputs.', job.split('    steps:', 1)[1])
        self.assertNotIn('rust-cache', job)
        self.assertEqual(re.findall(r'          - os: (.+)', job), ['macos-26', 'macos-26-intel'])
        self.assertEqual(re.findall(r'            target: (.+)', job),
                         ['aarch64-apple-darwin', 'x86_64-apple-darwin'])
        self.assertNotIn('contents: write', job)
        self.assertNotIn('action-gh-release', job)
        self.assertNotIn('gh release', job)
        self.assertIn('permissions:\n  contents: read\n', self.workflow)
        self.assertIn('    permissions:\n      contents: write\n', self.jobs['release'])

    def test_normal_release_stages_a_draft_for_the_publication_owner(self):
        release = self.jobs['release']
        self.assertIn('          draft: true\n', release)
        self.assertNotIn('          draft: false\n', release)

    def test_normal_release_proof_is_secret_free_and_excluded_from_assets(self):
        build = self.jobs['build']
        steps = build.split('\n      - ')
        proof = [s for s in steps if 'run: bash scripts/collect_macos_signing_evidence.sh' in s]
        self.assertEqual(len(proof), 1, 'normal signed releases must verify their mounted DMG')
        self.assertIn("if: matrix.kind == 'macos'", proof[0])
        self.assertIn('TARGET: ${{ matrix.target }}', proof[0])
        self.assertNotIn('secrets.', proof[0])
        secret_steps = [s for s in steps if 'secrets.' in s]
        self.assertEqual(len(secret_steps), 1)
        self.assertIn('run: bash scripts/sign_macos_release.sh', secret_steps[0])
        self.assertLess(build.index('cargo build --locked --release'), build.index('secrets.'))
        self.assertLess(build.index('swiftc scripts/import_macos_certificate.swift'), build.index('secrets.'))
        self.assertLess(build.index('bash scripts/sign_macos_release.sh'), build.index('bash scripts/collect_macos_signing_evidence.sh'))
        uploads = [s for s in steps if 'name: release-signing-evidence-${{ matrix.arch }}' in s]
        self.assertEqual(len(uploads), 1)
        upload = uploads[0]
        self.assertIn("if: matrix.kind == 'macos'", upload)
        self.assertIn('if-no-files-found: error', upload)
        paths = re.search(r'          path: \|\n((?:            .+\n)+)', upload)
        self.assertIsNotNone(paths)
        assert paths is not None
        self.assertEqual([p.strip() for p in paths.group(1).splitlines()], [
            'dist/signing-test/signing-evidence.txt',
            'dist/signing-test/SHA256SUMS',
        ])
        self.assertNotIn('secrets.', upload)
        self.assertNotIn('always()', build)
        release = self.jobs['release']
        download = next(s for s in release.split('\n      - ') if 'actions/download-artifact@' in s)
        self.assertIn('          pattern: release-assets-*\n', download)
        self.assertIn('          path: artifacts/\n', download)
        self.assertIn('          files: artifacts/**/*\n', release)

    def test_release_staging_preserves_the_remaining_workflow_payload(self):
        unchanged = re.sub(
            r'      - name: Verify signed release DMG and collect public evidence\n.*?'
            r'(?=      - name: Collect signed macOS DMG\n)',
            '', self.workflow, count=1, flags=re.S)
        unchanged = unchanged.replace('          pattern: release-assets-*\n', '', 1)
        unchanged = unchanged.replace('          draft: true\n', '          draft: false\n', 1)
        # Frozen pre-release workflow: all gates, jobs, matrix rows, existing
        # build/signing steps, credential boundaries and manual flow are retained.
        self.assertEqual(hashlib.sha256(unchanged.encode()).hexdigest(),
                         '8133cecc2ef4ab7fd3e321ce5496c32ae878d6d2c1d521318b53692047eecf1c')

    def test_secrets_only_in_sign_step_after_compilation_and_allowlisted_upload(self):
        self.assertIn('macos-signing-test', self.jobs)
        job = self.jobs['macos-signing-test']
        steps = job.split('\n      - ')
        secret_steps = [s for s in steps if 'secrets.' in s]
        self.assertEqual(len(secret_steps), 1)
        self.assertIn('run: bash scripts/sign_macos_release.sh', secret_steps[0])
        self.assertIn("MACOS_SIGNING_TEST: '1'", secret_steps[0])
        for name in ['MACOS_CERTIFICATE_P12_BASE64', 'MACOS_CERTIFICATE_PASSWORD']:
            self.assertIn(f'{name}: ${{{{ secrets.{name} }}}}', secret_steps[0])
        self.assertLess(job.index('cargo build --locked --release'), job.index('secrets.'))
        self.assertLess(job.index('swiftc scripts/import_macos_certificate.swift'), job.index('secrets.'))
        self.assertIn('bash scripts/collect_macos_signing_evidence.sh', job)
        self.assertIn('          if-no-files-found: error', job)
        self.assertIn('          retention-days: 7', job)
        self.assertIn('            dist/signing-test/*.dmg\n            dist/signing-test/SHA256SUMS\n            dist/signing-test/signing-evidence.txt', job)
        self.assertNotIn('always()', job)


if __name__ == '__main__':
    unittest.main()
