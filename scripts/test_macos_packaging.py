#!/usr/bin/env python3
"""Secret-free packaging regressions; platform tools are stand-ins on Linux."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class Packaging(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        shutil.copytree(ROOT / 'scripts', self.root / 'scripts')
        shutil.copy(ROOT / 'Cargo.toml', self.root)
        self.tools = self.root / 'tools'
        self.tools.mkdir()
        self.env = dict(os.environ, PATH=f'{self.tools}:/usr/bin:/bin',
                        TARGET='aarch64-apple-darwin', SKIP_BUILD='1',
                        REQUIRE_STABLE_SIGNING='1',
                        CODESIGN_IDENTITY='50B8A2614631D0EE1705213D19D07A52C921D665',
                        CODESIGN_KEYCHAIN='/private/keychain')
        binary = self.root / 'target/aarch64-apple-darwin/release/entropy'
        binary.parent.mkdir(parents=True)
        binary.write_text('prebuilt binary')
        binary.chmod(0o755)
        self.tool('cargo', 'exit 99')
        self.tool('lipo', '''if [[ "$#" == 2 && "$1" == -archs && -x "$2" ]]; then
 printf 'arm64\\n'
elif [[ "$#" == 3 && -x "$1" && "$2" == -verify_arch && "$3" == arm64 ]]; then
 exit 0
else
 echo 'lipo: expected input file before -verify_arch and a matching architecture' >&2
 exit 1
fi''')
        self.tool('plutil', 'exit 0')
        self.tool('ditto', 'if [[ "$1" == -c ]]; then touch "${@: -1}"; else cp -R "$1" "$2"; fi')
        self.tool('codesign', '''printf '%s\\n' "$*" >> "$PWD/codesign.log"
if [[ "$*" == *'-dv '* ]]; then echo 'Authority=Entropy Open Source Release Signing' >&2; fi
if [[ "$*" == *'-d -r-'* ]]; then
 echo 'designated => certificate root = H"50b8a2614631d0ee1705213d19d07a52c921d665" and identifier "com.ergohaven.entropy"' >&2
fi''')
        self.tool('hdiutil', '''if [[ "$1" == create ]]; then
 while [[ "$1" != -srcfolder ]]; do shift; done
 shift
 [[ -d "$1/Entropy.app" && -L "$1/Applications" ]]
 [[ "$(readlink "$1/Applications")" == /Applications ]]
 [[ "$(ls -A "$1" | wc -l | tr -d ' ')" == 2 ]]
 touch "${@: -1}"
fi''')

    def tool(self, name, body):
        path = self.tools / name
        path.write_text('#!/usr/bin/env bash\nset -e\n' + body + '\n')
        path.chmod(0o755)

    def run_package(self):
        return subprocess.run(['bash', 'scripts/build_macos_app.sh'], cwd=self.root,
                              env=self.env, text=True, capture_output=True)

    def test_wrong_certificate_or_bundle_id_rejected(self):
        for key, value in [('CODESIGN_IDENTITY', 'A' * 40), ('BUNDLE_ID', 'other.app')]:
            with self.subTest(key=key):
                self.env[key] = value
                result = self.run_package()
                self.assertNotEqual(result.returncode, 0)
                self.assertIn('pinned certificate and bundle ID', result.stderr)
                self.env.pop(key)
                self.env['CODESIGN_IDENTITY'] = '50B8A2614631D0EE1705213D19D07A52C921D665'

    def test_signature_verification_failure_stops_before_dmg(self):
        self.tool('codesign', 'exit 1')
        result = self.run_package()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(list((self.root / 'dist/macos').glob('*.dmg')), [])

    def test_public_evidence_requires_verified_dmg_and_cleanup(self):
        collector = self.root / 'scripts/collect_macos_signing_evidence.sh'
        self.assertTrue(collector.exists(), 'public evidence collector is missing')
        self.assertEqual(self.run_package().returncode, 0)
        self.tool('git', 'printf "%s\\n" "$GITHUB_SHA"')
        self.tool('hdiutil', '''if [[ "$1" == attach ]]; then
 while [[ "$1" != -mountpoint ]]; do shift; done
 shift
 cp -R "$PWD/dist/macos/Entropy.app" "$1/Entropy.app"
 ln -s /Applications "$1/Applications"
fi''')
        self.env.update(RUNNER_TEMP=str(self.root), GITHUB_SHA='a' * 40)
        cleanup = self.root / 'entropy-signing-cleanup.txt'
        cleanup.write_text('temporary_keychain_deleted=yes\nsearch_list_restored=yes\n')
        result = subprocess.run(['bash', str(collector)], cwd=self.root,
                                env=self.env, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        output = self.root / 'dist/signing-test'
        evidence = (output / 'signing-evidence.txt').read_text()
        self.assertIn('source_sha=' + 'a' * 40, evidence)
        self.assertIn('binary_and_bundle_signature=verified', evidence)
        self.assertIn('temporary_keychain_deleted=yes', evidence)
        self.assertNotIn('/path with spaces', evidence)
        self.assertEqual({p.name for p in output.iterdir()},
                         {'entropy-signing-test-arm64.dmg', 'SHA256SUMS', 'signing-evidence.txt'})
        subprocess.run(['shasum', '-a', '256', '-c', 'SHA256SUMS'], cwd=output, check=True,
                       capture_output=True)
        good_codesign = (self.tools / 'codesign').read_text()
        good_hdiutil = (self.tools / 'hdiutil').read_text()
        for failure in ['cleanup', 'signature', 'detach', 'credentials', 'sha', 'target', 'architecture']:
            with self.subTest(failure=failure):
                (self.tools / 'codesign').write_text(good_codesign)
                (self.tools / 'hdiutil').write_text(good_hdiutil)
                cleanup.write_text('temporary_keychain_deleted=yes\nsearch_list_restored=yes\n')
                env = dict(self.env)
                if failure == 'cleanup':
                    cleanup.unlink()
                elif failure == 'signature':
                    self.tool('codesign', 'exit 1')
                elif failure == 'detach':
                    (self.tools / 'hdiutil').write_text(
                        good_hdiutil + '\n[[ "$1" != detach ]]\n')
                elif failure == 'credentials':
                    env['MACOS_CERTIFICATE_PASSWORD'] = 'not-a-secret-fixture'
                elif failure == 'sha':
                    env['GITHUB_SHA'] = 'not-a-sha'
                elif failure == 'target':
                    env['TARGET'] = 'unknown'
                elif failure == 'architecture':
                    env['TARGET'] = 'x86_64-apple-darwin'
                result = subprocess.run(['bash', str(collector)], cwd=self.root,
                                        env=env, capture_output=True, text=True)
                self.assertNotEqual(result.returncode, 0)
                if failure == 'detach':
                    self.assertIn('Failed to detach validation DMG', result.stderr)

    def test_stable_release_cannot_invoke_cargo(self):
        self.env['SKIP_BUILD'] = '0'
        result = self.run_package()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('Build before importing release credentials', result.stderr)

    def test_dmg_creation_failure_is_not_swallowed(self):
        self.tool('sleep', 'exit 0')
        self.tool('hdiutil', 'exit 23')
        result = self.run_package()
        self.assertEqual(result.returncode, 23, result.stdout + result.stderr)
        self.assertFalse((self.root / 'dist/macos/dmg-staging').exists())

    def test_dmg_verification_retries_are_bounded_and_do_not_recreate_image(self):
        create_tool = (self.tools / 'hdiutil').read_text()
        self.tool('sleep', 'printf "%s\\n" "$1" >> "$PWD/sleep.log"')
        self.tool('hdiutil', '''printf '%s\\n' "$1" >> "$PWD/hdiutil.log"
if [[ "$1" == verify ]]; then
 [[ "$#" == 2 && -f "$2" ]]
 count=0
 if [[ -f "$PWD/verify-count" ]]; then read -r count < "$PWD/verify-count"; fi
 count=$((count + 1))
 printf '%s\\n' "$count" > "$PWD/verify-count"
 if ((count <= VERIFY_FAILURES)); then
  echo 'hdiutil: verify failed - Resource temporarily unavailable' >&2
  exit 23
 fi
 exit 0
fi
''' + create_tool)
        for failures, attempts, expected_status in [(0, 1, 0), (2, 3, 0), (3, 3, 23)]:
            with self.subTest(failures=failures):
                for name in ['hdiutil.log', 'sleep.log', 'verify-count']:
                    (self.root / name).unlink(missing_ok=True)
                self.env['VERIFY_FAILURES'] = str(failures)
                result = self.run_package()
                self.assertEqual(result.returncode, expected_status, result.stdout + result.stderr)
                calls = (self.root / 'hdiutil.log').read_text().splitlines()
                self.assertEqual(calls, ['create'] + ['verify'] * attempts)
                sleep_log = self.root / 'sleep.log'
                delays = sleep_log.read_text().splitlines() if sleep_log.exists() else []
                self.assertEqual(delays, [] if attempts == 1 else ['2', '4'])
                self.assertFalse((self.root / 'dist/macos/dmg-staging').exists())
                if expected_status:
                    self.assertIn('hdiutil verify failed after 3 attempts', result.stderr)
                    self.assertNotRegex(result.stdout, r'(?m)^Built .*/Entropy\.app$')

    def test_prebuilt_release_signs_binary_and_bundle_and_stages_applications(self):
        result = self.run_package()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        calls = (self.root / 'codesign.log').read_text()
        for path in ['Entropy.app/Contents/MacOS/entropy', 'Entropy.app']:
            self.assertTrue(any('--requirements =designated => anchor = H"50b8a2614631d0ee1705213d19d07a52c921d665" and identifier "com.ergohaven.entropy"' in line and line.endswith(path)
                                for line in calls.splitlines()))
        self.assertFalse((self.root / 'dist/macos/dmg-staging').exists())


class ReleaseSigning(unittest.TestCase):
    def test_manual_signing_requires_exact_upstream_branch_and_explicit_opt_in(self):
        trusted = dict(GITHUB_REPOSITORY='ergohaven/entropy',
                       GITHUB_EVENT_NAME='workflow_dispatch',
                       GITHUB_REF='refs/heads/ci/macos-signing-validation-pr195',
                       MACOS_SIGNING_TEST='1')
        cases = [({}, True)]
        for key, values in {
            'GITHUB_REPOSITORY': ['D19ital/entropy', 'other/entropy', ''],
            'GITHUB_EVENT_NAME': ['push', 'pull_request', 'pull_request_target', ''],
            'GITHUB_REF': ['refs/heads/main', 'refs/tags/v0.99.0',
                           'refs/heads/ci/macos-signing-validation-pr195-extra', ''],
            'MACOS_SIGNING_TEST': ['', '0', 'true'],
        }.items():
            cases.extend(({key: value}, False) for value in values)
        for overrides, allowed in cases:
            with self.subTest(overrides=overrides):
                env = dict(os.environ, **(trusted | overrides))
                env.pop('MACOS_CERTIFICATE_P12_BASE64', None)
                env.pop('MACOS_CERTIFICATE_PASSWORD', None)
                result = subprocess.run(['bash', str(ROOT / 'scripts/sign_macos_release.sh')],
                                        env=env, capture_output=True, text=True)
                self.assertNotEqual(result.returncode, 0)
                expected = 'Missing signing credentials' if allowed else 'Signing is restricted'
                self.assertIn(expected, result.stderr)

    def test_release_rejects_untrusted_events_even_with_credentials(self):
        for repository, event, ref in [('D19ital/entropy', 'push', 'refs/tags/v0.1.0'),
                                       ('ergohaven/entropy', 'pull_request', 'refs/tags/v0.1.0'),
                                       ('ergohaven/entropy', 'workflow_dispatch', 'refs/heads/main')]:
            env = dict(os.environ, GITHUB_REPOSITORY=repository, GITHUB_EVENT_NAME=event,
                       GITHUB_REF=ref)
            result = subprocess.run(['bash', str(ROOT / 'scripts/sign_macos_release.sh')],
                                    env=env, text=True, capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('restricted to upstream release tags', result.stderr)

    def test_search_list_restored_on_success_and_packaging_failure(self):
        cases = [(event, status, failure) for event in ['push', 'workflow_dispatch']
                 for status in [0, 17] for failure in [0, 1]]
        for event, packaging_status, cleanup_failure in cases:
            with self.subTest(event=event, packaging_status=packaging_status,
                              cleanup_failure=cleanup_failure), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                shutil.copytree(ROOT / 'scripts', root / 'scripts')
                tools = root / 'tools'
                tools.mkdir()
                def tool(path, body):
                    path.write_text('#!/usr/bin/env bash\nset -e\n' + body + '\n')
                    path.chmod(0o755)
                tool(tools / 'uname', 'echo Darwin')
                tool(tools / 'security', '''printf '%s\\n' "$1" >> "$RUNNER_TEMP/operations"
case "$1" in
 list-keychains)
   if [[ "$*" == *' -s'* ]]; then
     shift 4; printf '%s\\n' "$@" > "$RUNNER_TEMP/restored"
   elif [[ -f "$RUNNER_TEMP/restored" && "$CLEANUP_FAILURE" == 1 ]]; then echo wrong-list;
   else printf '    "/Users/test/login.keychain-db"\\n    "/path with spaces/custom.keychain-db"\\n'; fi ;;
 create-keychain) touch "${@: -1}" ;;
 delete-keychain) rm "${@: -1}" ;;
 find-certificate) echo 'SHA-1 hash: 50B8A2614631D0EE1705213D19D07A52C921D665' ;;
esac''')
                tool(root / 'entropy-certificate-import', '''[[ -n "$MACOS_CERTIFICATE_P12_BASE64" && -n "$MACOS_CERTIFICATE_PASSWORD" ]]''')
                tool(root / 'scripts/build_macos_app.sh', '''[[ -z "${MACOS_CERTIFICATE_P12_BASE64:-}" && -z "${MACOS_CERTIFICATE_PASSWORD:-}" ]]
[[ "$SKIP_BUILD" == 1 && "$REQUIRE_STABLE_SIGNING" == 1 ]]
[[ "$CODESIGN_IDENTITY" == 50B8A2614631D0EE1705213D19D07A52C921D665 ]]
exit "$PACKAGING_STATUS"''')
                env = dict(os.environ, PATH=f'{tools}:/usr/bin:/bin', RUNNER_TEMP=tmp,
                           GITHUB_REPOSITORY='ergohaven/entropy', GITHUB_EVENT_NAME=event,
                           GITHUB_REF=('refs/tags/v0.99.0' if event == 'push' else
                                       'refs/heads/ci/macos-signing-validation-pr195'),
                           MACOS_SIGNING_TEST='1', TARGET='aarch64-apple-darwin',
                           CLEANUP_FAILURE=str(cleanup_failure),
                           MACOS_CERTIFICATE_P12_BASE64='not-a-certificate-fixture',
                           MACOS_CERTIFICATE_PASSWORD='not-a-secret-fixture',
                           PACKAGING_STATUS=str(packaging_status))
                result = subprocess.run(['bash', str(root / 'scripts/sign_macos_release.sh')],
                                        env=env, text=True, capture_output=True)
                self.assertEqual(result.returncode, 1 if cleanup_failure else packaging_status,
                                 result.stderr)
                self.assertEqual((root / 'restored').read_text().splitlines(),
                                 ['/Users/test/login.keychain-db', '/path with spaces/custom.keychain-db'])
                self.assertEqual(list(root.glob('entropy-signing.*')), [])
                evidence = root / 'entropy-signing-cleanup.txt'
                if packaging_status == 0 and not cleanup_failure:
                    self.assertEqual(evidence.read_text() if evidence.exists() else '',
                                     'temporary_keychain_deleted=yes\nsearch_list_restored=yes\n')
                else:
                    self.assertFalse(evidence.exists())
                operations = (root / 'operations').read_text().splitlines()
                self.assertEqual(operations[-3:], ['delete-keychain', 'list-keychains', 'list-keychains'])
                self.assertNotIn('default-keychain', operations)
                self.assertNotIn('add-trusted-cert', operations)

    def test_missing_credentials_fail_before_keychain_access(self):
        env = dict(os.environ, GITHUB_REPOSITORY='ergohaven/entropy',
                   GITHUB_EVENT_NAME='push', GITHUB_REF='refs/tags/v0.99.0')
        env.pop('MACOS_CERTIFICATE_P12_BASE64', None)
        env.pop('MACOS_CERTIFICATE_PASSWORD', None)
        result = subprocess.run(['bash', str(ROOT / 'scripts/sign_macos_release.sh')],
                                env=env, capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('Missing signing credentials', result.stderr)


@unittest.skipUnless(os.environ.get('ENTROPY_TEST_DMG'), 'native DMG not requested')
class NativeDMG(unittest.TestCase):
    def test_mounted_dmg_contains_app_and_applications_symlink_only(self):
        with tempfile.TemporaryDirectory() as tmp:
            mount = Path(tmp) / 'mount'
            mount.mkdir()
            subprocess.run(['hdiutil', 'attach', '-readonly', '-nobrowse', '-mountpoint',
                            str(mount), os.environ['ENTROPY_TEST_DMG']], check=True)
            try:
                visible = {p.name for p in mount.iterdir() if not p.name.startswith('.')}
                self.assertEqual(visible, {'Entropy.app', 'Applications'})
                self.assertTrue((mount / 'Applications').is_symlink())
                self.assertEqual(os.readlink(mount / 'Applications'), '/Applications')
                subprocess.run(['codesign', '--verify', '--strict',
                                str(mount / 'Entropy.app')], check=True)
            finally:
                subprocess.run(['hdiutil', 'detach', str(mount)], check=True)


if __name__ == '__main__':
    unittest.main()
