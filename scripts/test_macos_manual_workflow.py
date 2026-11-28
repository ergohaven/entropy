#!/usr/bin/env python3
"""Keep signing isolation and immutable payload guards across candidate promotion."""
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
        self.assertEqual(self.gate('macos-signing-test'),
                         "${{ github.repository == 'ergohaven/entropy' && github.event_name == 'workflow_dispatch' && github.ref == 'refs/heads/ci/macos-signing-validation-pr195' && inputs.macos_signing_test == true }}")
        self.assertIn('      macos_signing_test:\n', self.workflow)
        self.assertIn('        type: boolean\n        default: false', self.workflow)
        self.assertNotIn('pull_request', self.workflow)
        self.assertEqual(set(self.jobs), {'plan', 'validate', 'nix', 'build', 'candidate', 'macos-signing-test', 'release'})
        self.assertIn('    needs: [plan, build, validate, nix]\n', self.jobs['candidate'])
        for name, path in [('validate', 'release-validation.yml'), ('nix', 'nix.yml')]:
            self.assertEqual(self.gate(name), "needs.plan.outputs.prepare == 'true'")
            self.assertIn(f'    uses: ./.github/workflows/{path}\n', self.jobs[name])
            self.assertNotIn('secrets:', self.jobs[name])
        self.assertEqual(self.gate('build'), "needs.plan.outputs.prepare == 'true'")
        self.assertEqual(self.gate('release'),
                         "${{ github.repository == 'ergohaven/entropy' && github.event_name == 'push' && startsWith(github.ref, 'refs/tags/v0.') }}")

    def test_manual_has_read_only_token_exact_checkout_and_two_macos_targets(self):
        job = self.jobs['macos-signing-test']
        self.assertIn('    permissions:\n      contents: read\n', job)
        self.assertIn('          ref: ${{ github.sha }}\n          persist-credentials: false', job)
        self.assertNotIn('inputs.', job.split('    steps:', 1)[1])
        self.assertNotIn('rust-cache', job)
        self.assertEqual(re.findall(r'          - os: (.+)', job), ['macos-26', 'macos-26-intel'])
        self.assertEqual(re.findall(r'            target: (.+)', job), ['aarch64-apple-darwin', 'x86_64-apple-darwin'])
        for marker in ('contents: write', 'action-gh-release', 'gh release'):
            self.assertNotIn(marker, job)
        self.assertIn('permissions:\n  contents: read\n', self.workflow)
        self.assertNotIn('contents: write', self.jobs['release'])

    def test_promotion_uses_checked_draft_and_public_byte_readback_owner(self):
        release = self.jobs['release']
        self.assertIn('release_candidate.py fetch', release)
        self.assertIn('release_candidate.py stage', release)
        self.assertIn('release_candidate.py publish', release)
        self.assertNotIn('action-gh-release', release)
        self.assertEqual(release.count('secrets.'), 1)
        self.assertIn('GH_TOKEN: ${{ secrets.ERGOHAVEN_RELEASE_TOKEN }}', release)
        self.assertLess(release.index('release_candidate.py stage'), release.index('secrets.'))

    def test_normal_release_proof_is_secret_free_and_excluded_from_assets(self):
        build = self.jobs['build']
        steps = build.split('\n      - ')
        proof = [s for s in steps if 'run: bash scripts/collect_macos_signing_evidence.sh' in s]
        self.assertEqual(len(proof), 1)
        self.assertIn("if: matrix.kind == 'macos'", proof[0])
        self.assertIn('TARGET: ${{ matrix.target }}', proof[0])
        self.assertNotIn('secrets.', proof[0])
        secret_steps = [s for s in steps if 'secrets.' in s]
        self.assertEqual(len(secret_steps), 1)
        self.assertIn('run: bash scripts/sign_macos_release.sh', secret_steps[0])
        self.assertLess(build.index('scripts/stage_macos_release_binary.py'), build.index('secrets.'))
        self.assertLess(build.index('swiftc scripts/import_macos_certificate.swift'), build.index('secrets.'))
        self.assertLess(build.index('bash scripts/sign_macos_release.sh'), build.index('bash scripts/collect_macos_signing_evidence.sh'))
        upload = next(s for s in steps if 'name: release-signing-evidence-${{ matrix.arch }}' in s)
        paths = re.search(r'          path: \|\n((?:            .+\n)+)', upload)
        self.assertIsNotNone(paths)
        assert paths is not None
        self.assertEqual([p.strip() for p in paths.group(1).splitlines()], ['dist/signing-test/signing-evidence.txt', 'dist/signing-test/SHA256SUMS'])
        self.assertIn('if-no-files-found: error', upload)
        self.assertNotIn('secrets.', upload)
        self.assertNotIn('always()', build)
        candidate = self.jobs['candidate']
        self.assertIn('pattern: release-assets-*', candidate)
        self.assertIn('pattern: ci-proof-*', candidate)
        self.assertIn('pattern: release-signing-evidence-*', candidate)
        self.assertNotIn('secrets.', candidate)
        self.assertLess(candidate.index('actions/download-artifact@'), candidate.index('release_candidate.py pack'))

    def test_release_staging_preserves_the_remaining_workflow_payload(self):
        # Reverse the reviewed publisher delta from 05a0de2d, not the whole
        # release job. Each exact replacement must occur once; the original
        # signing/publication baseline digest below remains immutable.
        workflow = (ROOT / '.github/scripts/fixtures/release_before_candidates.yml').read_text()
        legacy_jobs = dict(re.findall(r'^  ([a-z][a-z_-]*):\n(.*?)(?=^  [a-z][a-z_-]*:\n|\Z)', workflow.split('\njobs:\n', 1)[1], re.M | re.S))
        self.assertEqual(self.jobs['macos-signing-test'], legacy_jobs['macos-signing-test'])
        old_signing = legacy_jobs['build'].split('      - name: Sign macOS app and build DMG', 1)[1].split('      - name: Collect signed macOS DMG', 1)[0]
        new_signing = self.jobs['build'].split('      - name: Sign macOS app and build DMG', 1)[1].split('      - name: Collect signed macOS DMG', 1)[0]
        self.assertEqual(new_signing.replace('retention-days: 14', 'retention-days: 7'), old_signing)

        def reverse_exact(current, previous):
            nonlocal workflow
            self.assertEqual(workflow.count(current), 1)
            workflow = workflow.replace(current, previous, 1)

        reverse_exact('''      - name: Verify release inventory and write checksums
        run: python3 scripts/prepare_release_assets.py --source artifacts --destination dist/release --tag "$GITHUB_REF_NAME"

''', '')
        reverse_exact('''      - name: Verify release publisher
        env:
          GH_TOKEN: ${{ secrets.ERGOHAVEN_RELEASE_TOKEN }}
        run: |
          test -n "$GH_TOKEN"
          test "$(gh api user --jq .login)" = kissetfall
''', '''          # Markdown backticks are literal installation text, not shell commands.
          # shellcheck disable=SC2016
          {
            printf '\\n## Downloads\\n\\n'
            printf '%s\\n' '- Linux: install the `.deb` (Debian/Ubuntu), `.rpm` (Fedora/openSUSE) or `.pkg.tar.zst` (Arch) with your package manager, or make the portable `.AppImage` executable and run it.'
            printf '%s\\n' '- Windows: download and run the portable `.exe`.'
            printf '%s\\n' '- macOS: download the `.dmg` for your Mac (`arm64` for Apple Silicon, `x86_64` for Intel). See README for installation and Open Anyway instructions (self-signed, not notarized).'
          } >> RELEASE_NOTES.md
''')
        reverse_exact('''          token: ${{ secrets.ERGOHAVEN_RELEASE_TOKEN }}
          tag_name: ${{ github.ref_name }}
          target_commitish: ${{ github.sha }}
''', '')
        reverse_exact("          make_latest: 'false'\n",
                      "          make_latest: ${{ contains(github.ref_name, '-') && 'false' || 'true' }}\n")
        reverse_exact('''          fail_on_unmatched_files: true
          files: dist/release/*
''', '          files: artifacts/**/*\n')
        # Reverse only the new release-only validation wiring before comparing
        # the original signing and publication payload with its immutable hash.
        for name in ('validate', 'nix'):
            workflow = workflow.replace(f'  {name}:\n{legacy_jobs[name]}', '', 1)
        workflow = workflow.replace('    needs: [build, validate, nix]\n',
                                    '    needs: build\n', 1)
        unchanged = re.sub(
            r'      - name: Verify signed release DMG and collect public evidence\n.*?'
            r'(?=      - name: Collect signed macOS DMG\n)',
            '', workflow, count=1, flags=re.S)
        unchanged = unchanged.replace('          pattern: release-assets-*\n', '', 1)
        unchanged = unchanged.replace('          draft: true\n', '          draft: false\n', 1)
        # Reverse the exact Linux asset/notes delta, not an arbitrary job body.
        # Keep the original digest so signing, gates and publication stay frozen.
        linux_artifacts = """      - name: Package Linux artifacts
        if: matrix.kind == 'linux'
        shell: bash
        run: |
          prefix="entropy-${GITHUB_REF_NAME}"
          VERSION="${GITHUB_REF_NAME#v}" task docker:linux
          mkdir -p dist/release
          cp "dist/linux/$prefix-x86_64.AppImage" dist/release/
          cp dist/linux/*.deb "dist/release/$prefix-amd64.deb"
          cp dist/linux/*.rpm "dist/release/$prefix-x86_64.rpm"
          cp dist/linux/*.pkg.tar.zst "dist/release/$prefix-x86_64.pkg.tar.zst"
          printf 'Release Linux artifacts glibc requirement: '
          scripts/glibc_baseline.sh target/release/entropy
"""
        linux_appimage = """      - name: Package Linux artifact
        if: matrix.kind == 'linux'
        shell: bash
        run: |
          asset="entropy-${GITHUB_REF_NAME}-x86_64.AppImage"
          VERSION="${GITHUB_REF_NAME#v}" task docker:appimage
          mkdir -p dist/release
          cp "dist/linux/$asset" "dist/release/$asset"
          printf 'Release AppImage glibc requirement: '
          scripts/glibc_baseline.sh target/release/entropy
"""
        self.assertEqual(unchanged.count(linux_artifacts), 1)
        unchanged = unchanged.replace(linux_artifacts, linux_appimage, 1)
        linux_downloads = '- Linux: install the `.deb` (Debian/Ubuntu), `.rpm` (Fedora/openSUSE) or `.pkg.tar.zst` (Arch) with your package manager, or make the portable `.AppImage` executable and run it.'
        self.assertEqual(unchanged.count(linux_downloads), 1)
        unchanged = unchanged.replace(linux_downloads,
                                      '- Linux: download the `.AppImage`, make it executable, and run it.', 1)
        # Frozen workflow after routing Linux through the existing pinned
        # container owner; all non-Linux gates, signing and manual flow remain.
        self.assertEqual(hashlib.sha256(unchanged.encode()).hexdigest(),
                         '775e39de2aeb33e6b1acbaa98f347a8fd7724a7cde4cb5ee5d09ec0a65bff3c9')

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
