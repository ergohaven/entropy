"""Offline policy contract tests; remote transport is always explicit."""
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
import zipfile
from datetime import datetime, timezone, timedelta
from unittest.mock import patch

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent


class ReceiptTests(unittest.TestCase):
    def test_record_binds_real_checkout_and_full_scope(self):
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / 'new-parent' / 'proof.json'
            env = dict(os.environ, GITHUB_REPOSITORY='owner/repo',
                       GITHUB_WORKFLOW_REF='owner/repo/.github/workflows/build.yml@refs/heads/main',
                       GITHUB_RUN_ID='123', GITHUB_RUN_ATTEMPT='2')
            result = subprocess.run(['python3', str(HERE / 'ci_policy.py'), 'record',
                                     '--profile', 'build', '--output', str(output)],
                                    cwd=ROOT, env=env, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            proof = json.loads(output.read_text())
            git = lambda arg: subprocess.check_output(['git', 'rev-parse', arg], cwd=ROOT, text=True).strip()
            self.assertEqual(proof, dict(schema=1, repository='owner/repo',
                workflow='.github/workflows/build.yml', profile='build', run_id=123,
                run_attempt=2, checkout_sha=git('HEAD'), tree=git('HEAD^{tree}'), scope='full'))


    def test_plan_cli_writes_controlled_outputs_and_summary(self):
        with tempfile.TemporaryDirectory() as tmp:
            event = Path(tmp) / 'event.json'
            event.write_text('{}')
            output, summary = Path(tmp) / 'output', Path(tmp) / 'summary'
            env = dict(os.environ, GITHUB_EVENT_NAME='workflow_dispatch', GITHUB_EVENT_PATH=str(event),
                       GITHUB_OUTPUT=str(output), GITHUB_STEP_SUMMARY=str(summary))
            result = subprocess.run(['python3', str(HERE / 'ci_policy.py'), 'plan', '--profile', 'build'],
                                    cwd=ROOT, env=env, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(output.read_text(), 'run=true\nreason=event-requires-full-validation\nsource_run=\n')
            self.assertIn('event-requires-full-validation', summary.read_text())

    def test_manual_builds_cli_skips_ordinary_source_pr(self):
        with tempfile.TemporaryDirectory() as tmp:
            event = Path(tmp) / 'event.json'
            event.write_text(json.dumps({'pull_request': {
                'base': {'sha': 'a' * 40}, 'head': {'sha': 'b' * 40}}}))
            output, summary = Path(tmp) / 'output', Path(tmp) / 'summary'
            env = dict(os.environ, GITHUB_EVENT_NAME='pull_request',
                       GITHUB_REF='refs/pull/1/merge', GITHUB_EVENT_PATH=str(event),
                       GITHUB_OUTPUT=str(output), GITHUB_STEP_SUMMARY=str(summary))
            result = subprocess.run(['python3', str(HERE / 'ci_policy.py'), 'plan',
                                     '--profile', 'build', '--manual-builds'],
                                    cwd=ROOT, env=env, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(output.read_text(),
                             'run=false\nreason=manual-build-request-required\nsource_run=\n')
            self.assertIn('manual-build-request-required (run=false)', summary.read_text())
            self.assertNotIn('Original full validation:', summary.read_text())
            self.assertNotIn('Proof artifact:', summary.read_text())

    def test_missing_event_cli_fails_closed(self):
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / 'output'
            env = dict(os.environ, GITHUB_EVENT_PATH=str(Path(tmp) / 'missing'), GITHUB_OUTPUT=str(output))
            result = subprocess.run(['python3', str(HERE / 'ci_policy.py'), 'plan', '--profile', 'build'],
                                    cwd=ROOT, env=env, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn('run=true\n', output.read_text())


def load_policy():
    spec = importlib.util.spec_from_file_location('ci_policy', HERE / 'ci_policy.py')
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class SelectionTests(unittest.TestCase):
    def setUp(self):
        self.p = load_policy()
        self.env = dict(GITHUB_REPOSITORY='owner/repo', GITHUB_EVENT_NAME='pull_request',
            GITHUB_REF='refs/pull/1/merge', GITHUB_WORKFLOW_REF='owner/repo/.github/workflows/build.yml@refs/pull/1/merge')
        self.event = {'pull_request': {'base': {'sha': 'a' * 40}, 'head': {'sha': 'b' * 40}}}

    def decision(self, paths, profile='build'):
        self.assertTrue(callable(getattr(self.p, 'plan', None)), 'planner is not implemented')
        with patch.object(self.p, 'changed_paths', return_value=paths):
            return self.p.plan(profile, self.env, self.event, transport=lambda path: self.fail('unexpected API'))

    def test_manual_builds_ordinary_source_events_skip_before_diff_or_api(self):
        for profile in ('build', 'nix'):
            for name, ref, event in (
                    ('pull_request', 'refs/pull/1/merge', self.event),
                    ('pull_request', 'refs/pull/1/head', self.event),
                    ('push', 'refs/heads/main', {'before': 'a' * 40, 'after': 'b' * 40})):
                with self.subTest(profile=profile, event=name, ref=ref):
                    env = dict(self.env, GITHUB_EVENT_NAME=name, GITHUB_REF=ref, GH_TOKEN='offline')
                    with patch.object(self.p, 'changed_paths', return_value=['src/main.rs']) as diff, \
                            patch.object(self.p, 'reuse') as reuse, \
                            patch.object(self.p, 'api_transport') as api:
                        result = self.p.plan(profile, env, event, manual_builds=True)
                    self.assertEqual(result, self.p.decision(False, 'manual-build-request-required'))
                    diff.assert_not_called()
                    reuse.assert_not_called()
                    api.assert_not_called()

    def test_manual_builds_conservative_events_still_require_full_even_docs(self):
        cases = (
            ('workflow_dispatch', 'refs/heads/main', 'event-requires-full-validation'),
            ('workflow_call', 'refs/heads/main', 'event-requires-full-validation'),
            ('push', 'refs/tags/v1', 'event-requires-full-validation'),
            ('release', 'refs/tags/v1', 'event-requires-full-validation'),
            ('unknown', 'refs/heads/main', 'event-requires-full-validation'),
            ('push', 'unknown', 'unknown-ref-requires-full-validation'),
            ('pull_request', 'refs/heads/main', 'unknown-ref-requires-full-validation'),
        )
        for profile in ('build', 'nix'):
            for name, ref, reason in cases:
                with self.subTest(profile=profile, event=name, ref=ref):
                    env = dict(self.env, GITHUB_EVENT_NAME=name, GITHUB_REF=ref)
                    with patch.object(self.p, 'changed_paths', return_value=['README.md']) as diff, \
                            patch.object(self.p, 'reuse') as reuse:
                        result = self.p.plan(profile, env, self.event, manual_builds=True)
                    self.assertEqual(result, self.p.decision(reason=reason))
                    diff.assert_not_called()
                    reuse.assert_not_called()

    def test_manual_builds_checkout_failure_still_fails_closed(self):
        with patch.object(self.p, 'git', side_effect=OSError('missing checkout')):
            self.assertEqual(self.p.plan('build', self.env, self.event, manual_builds=True),
                             self.p.decision(reason='uncertain-requires-full-validation'))

    def test_manual_builds_false_retains_unflagged_pr_selection(self):
        for paths in (['README.md'], ['src/main.rs']):
            with self.subTest(paths=paths), patch.object(self.p, 'changed_paths', return_value=paths):
                self.assertEqual(self.p.plan('build', self.env, self.event, manual_builds=False),
                                 self.decision(paths))

    def test_docs_skip(self):
        self.assertFalse(self.decision(['README.md', 'docs/guide.rst'])['run'])

    def test_source_runs(self):
        self.assertTrue(self.decision(['docs/a.md', 'src/main.rs'])['run'])

    def test_unknown_docs_extension_runs(self):
        self.assertTrue(self.decision(['docs/config.py'])['run'])

    def test_firmware_tests_only_skip(self):
        self.assertFalse(self.decision(['tests/a.c', '.github/scripts/test_ci_policy.py'], 'firmware')['run'])

    def test_test_profiles_run_test_changes(self):
        for profile in ('native', 'tests', 'build', 'nix'):
            with self.subTest(profile=profile):
                self.assertTrue(self.decision(['tests/a.c'], profile)['run'])

    def test_policy_changes_run(self):
        self.assertTrue(self.decision(['.github/scripts/ci_policy.py'], 'firmware')['run'])

    def test_manual_unknown_tags_always_full(self):
        for name, ref in [('workflow_dispatch', 'refs/heads/main'), ('workflow_call', 'refs/heads/main'),
                          ('release', 'refs/tags/v1'), ('push', 'refs/tags/v1'), ('unknown', 'refs/heads/main')]:
            with self.subTest(event=name):
                self.env.update(GITHUB_EVENT_NAME=name, GITHUB_REF=ref)
                self.assertTrue(self.decision(['README.md'])['run'])

    def test_unknown_ref_requires_full_validation(self):
        self.env['GITHUB_REF'] = 'unknown'
        self.assertTrue(self.decision(['README.md'])['run'])

    def test_diff_failure_full(self):
        with patch.object(self.p, 'changed_paths', side_effect=ValueError('shallow')):
            self.assertTrue(self.p.plan('build', self.env, self.event)['run'])

    def test_pr_never_reuses(self):
        self.env['GH_TOKEN'] = 'not-a-real-token'
        self.assertTrue(self.decision(['src/main.rs'])['run'])


class ReuseTests(unittest.TestCase):
    def setUp(self):
        self.p = load_policy()
        self.env = dict(GITHUB_REPOSITORY='owner/repo', GITHUB_EVENT_NAME='push',
            GITHUB_REF='refs/heads/main', GITHUB_WORKFLOW_REF='owner/repo/.github/workflows/build.yml@refs/heads/main',
            GH_TOKEN='offline')
        self.proof = self.p.record('build', dict(self.env, GITHUB_RUN_ID='123', GITHUB_RUN_ATTEMPT='2'))
        self.remote_run = dict(id=123, run_attempt=2, event='pull_request', status='completed', conclusion='success',
                        path='.github/workflows/build.yml', repository=dict(full_name='owner/repo', id=4),
                        head_sha='a' * 40)
        self.artifact = dict(id=456, name='ci-proof-build-123-2', expired=False, size_in_bytes=500,
            expires_at=(datetime.now(timezone.utc) + timedelta(days=1)).isoformat(),
            workflow_run=dict(id=123, repository_id=4, head_sha='a' * 40))
        self.remote_run_total = 1
        self.archive = None
        self.calls = []

    def transport(self, path, raw=False):
        self.calls.append(path)
        if '/actions/workflows/build.yml/runs?' in path:
            return dict(total_count=self.remote_run_total, workflow_runs=[self.remote_run])
        if path.endswith('/actions/runs/123/attempts/2'):
            return self.remote_run
        if path.endswith('/actions/runs/123/artifacts?per_page=100'):
            return dict(total_count=1, artifacts=[self.artifact])
        if path.endswith('/actions/artifacts/456/zip'):
            if self.archive is not None:
                return self.archive
            stream = io.BytesIO()
            with zipfile.ZipFile(stream, 'w') as archive:
                archive.writestr('proof.json', json.dumps(self.proof))
            return stream.getvalue()
        if '/git/commits/' in path:
            return dict(sha=path.rsplit('/', 1)[-1], tree=dict(sha=self.p.git('rev-parse', 'HEAD^{tree}')))
        raise AssertionError('unexpected transport path: ' + path)

    def result(self):
        with patch.object(self.p, 'changed_paths', return_value=['src/main.rs']):
            return self.p.plan('build', self.env, {}, transport=self.transport)

    def test_search_is_bounded_to_workflow_and_recent_window(self):
        self.result()
        self.assertIn('/actions/workflows/build.yml/runs?', self.calls[0])
        self.assertIn('created=', self.calls[0])

    def test_matching_full_proof_reuses_original_run(self):
        result = self.result()
        self.assertFalse(result['run'])
        self.assertEqual(result['source_run'], 'https://github.com/owner/repo/actions/runs/123/attempts/2')
        self.assertEqual(result['artifact_url'], 'https://github.com/owner/repo/actions/runs/123/artifacts/456')
        self.assertEqual(self.result(), result, 'reuse must not produce a new proof chain')

    def test_proof_identity_tampering_fails_closed(self):
        for key, value in dict(schema=2, repository='other/repo', workflow='.github/workflows/other.yml',
                profile='native', run_id=124, run_attempt=1, tree='0' * 40, scope='reused', checkout_sha='bad').items():
            with self.subTest(field=key):
                original = self.proof[key]
                self.proof[key] = value
                self.assertTrue(self.result()['run'])
                self.proof[key] = original

    def test_unsuccessful_or_foreign_runs_fail_closed(self):
        for key, value in dict(status='in_progress', conclusion='failure', event='push',
                path='.github/workflows/other.yml', repository=dict(full_name='other/repo', id=4)).items():
            with self.subTest(field=key):
                original = self.remote_run[key]
                self.remote_run[key] = value
                self.assertTrue(self.result()['run'])
                self.remote_run[key] = original

    def test_expired_or_mismatched_artifact_fails_closed(self):
        for key, value in dict(expired=True, expires_at='2000-01-01T00:00:00Z',
                name='ci-proof-build-123-1', workflow_run=dict(id=124), size_in_bytes=9999999).items():
            with self.subTest(field=key):
                original = self.artifact[key]
                self.artifact[key] = value
                self.assertTrue(self.result()['run'])
                self.artifact[key] = original

    def test_bad_zip_fails_closed(self):
        self.archive = b'not a zip'
        self.assertTrue(self.result()['run'])

    def test_no_token_fails_closed(self):
        del self.env['GH_TOKEN']
        self.assertTrue(self.result()['run'])
        self.assertFalse(self.calls)

    def test_too_many_candidates_fails_closed(self):
        self.remote_run_total = 51
        self.assertTrue(self.result()['run'])

    def test_zip_crc_path_and_size_are_checked(self):
        for filename, payload in [('nested/proof.json', b'{}'), ('proof.json', b'x' * 65537)]:
            stream = io.BytesIO()
            with zipfile.ZipFile(stream, 'w', compression=zipfile.ZIP_DEFLATED) as archive:
                archive.writestr(filename, payload)
            self.archive = stream.getvalue()
            self.assertTrue(self.result()['run'])
        stream = io.BytesIO()
        with zipfile.ZipFile(stream, 'w') as archive:
            info = zipfile.ZipInfo('proof.json')
            info.extra = b'\xff\xff\x01\x00{'
            archive.writestr(info, json.dumps(self.proof))
        corrupt = bytearray(stream.getvalue())
        # A single marker can occur in the header CRC/timestamps/extra fields.
        # Locate the complete stored payload, then corrupt only its data bytes.
        offset = corrupt.index(json.dumps(self.proof).encode())
        self.assertLess(corrupt.index(b'{'), offset)
        corrupt[offset] = ord('[')
        self.archive = bytes(corrupt)
        with self.assertRaisesRegex(zipfile.BadZipFile, 'Bad CRC'):
            self.p.read_proof(self.archive)
        self.assertTrue(self.result()['run'])

    def test_commit_api_tree_must_match(self):
        original = self.transport
        def transport(path, raw=False):
            if path.endswith(f"/git/commits/{self.proof['checkout_sha']}"):
                return dict(sha=self.proof['checkout_sha'], tree=dict(sha='f' * 40))
            return original(path, raw)
        with patch.object(self.p, 'changed_paths', return_value=['src/main.rs']):
            self.assertTrue(self.p.plan('build', self.env, {}, transport=transport)['run'])
        self.assertTrue(any('/actions/artifacts/456/zip' in call for call in self.calls))

    def test_other_pr_cannot_claim_a_foreign_tree_in_its_proof(self):
        original = self.transport
        def transport(path, raw=False):
            if path.endswith(f"/git/commits/{self.remote_run['head_sha']}"):
                return dict(sha=self.remote_run['head_sha'], tree=dict(sha='f' * 40))
            return original(path, raw)
        with patch.object(self.p, 'changed_paths', return_value=['src/main.rs']):
            self.assertTrue(self.p.plan('build', self.env, {}, transport=transport)['run'])

    def test_direct_push_without_proof_runs(self):
        original = self.transport
        def transport(path, raw=False):
            if '/artifacts?' in path:
                return dict(total_count=0, artifacts=[])
            return original(path, raw)
        with patch.object(self.p, 'changed_paths', return_value=['src/main.rs']):
            self.assertTrue(self.p.plan('build', self.env, {}, transport=transport)['run'])

    def test_unsafe_archive_duplicate_json_keys_rejected(self):
        stream = io.BytesIO()
        text = json.dumps(self.proof)
        with zipfile.ZipFile(stream, 'w') as archive:
            archive.writestr('proof.json', text[:-1] + ', "schema": 1}')
        self.archive = stream.getvalue()
        self.assertTrue(self.result()['run'])

    def test_run_list_and_detail_status_must_agree(self):
        original = self.transport
        def transport(path, raw=False):
            if '/attempts/2' in path:
                return dict(self.remote_run, status='completed', conclusion='success')
            return original(path, raw)
        self.remote_run['status'] = 'in_progress'
        with patch.object(self.p, 'changed_paths', return_value=['src/main.rs']):
            self.assertTrue(self.p.plan('build', self.env, {}, transport=transport)['run'])

    def test_transport_error_fails_closed(self):
        with patch.object(self.p, 'changed_paths', return_value=['src/main.rs']):
            self.assertTrue(self.p.plan('build', self.env, {}, transport=lambda *a, **k: (_ for _ in ()).throw(OSError('offline')))['run'])


class GitAndTransportTests(unittest.TestCase):
    def setUp(self):
        self.p = load_policy()

    def test_pr_diff_is_merge_base_and_nul_safe(self):
        env = dict(GITHUB_EVENT_NAME='pull_request')
        event = dict(pull_request=dict(base=dict(sha='a' * 40), head=dict(sha='b' * 40)))
        with patch.object(self.p, 'git', return_value='false'), patch.object(self.p.subprocess, 'check_output', return_value=b'docs/line\nbreak.md\0src/a.rs\0') as call:
            self.assertEqual(self.p.changed_paths(env, event), ['docs/line\nbreak.md', 'src/a.rs'])
            self.assertIn('a' * 40 + '...' + 'b' * 40, call.call_args.args[0])
            self.assertIn('-z', call.call_args.args[0])
            self.assertIn('--no-renames', call.call_args.args[0])

    def test_push_diff_is_before_after(self):
        with patch.object(self.p, 'git', return_value='false'), patch.object(self.p.subprocess, 'check_output', return_value=b'README.md\0') as call:
            self.p.changed_paths(dict(GITHUB_EVENT_NAME='push'), dict(before='a' * 40, after='b' * 40))
            self.assertIn('a' * 40 + '..' + 'b' * 40, call.call_args.args[0])

    def test_shallow_history_rejected(self):
        with patch.object(self.p, 'git', return_value='true'):
            with self.assertRaises(ValueError):
                self.p.changed_paths(dict(GITHUB_EVENT_NAME='push'), {})

    def test_new_branch_deletion_and_bad_sha_rejected(self):
        for event in [dict(created=True), dict(deleted=True), dict(before='0' * 40, after='b' * 40), dict(before='bad', after='b' * 40)]:
            with self.subTest(event=event), patch.object(self.p, 'git', return_value='false'):
                with self.assertRaises(ValueError):
                    self.p.changed_paths(dict(GITHUB_EVENT_NAME='push'), event)

    def test_real_git_diff_is_exercised(self):
        head = self.p.git('rev-parse', 'HEAD')
        paths = self.p.changed_paths(dict(GITHUB_EVENT_NAME='push'), dict(before=head, after=head))
        self.assertEqual(paths, [])

    def test_signed_archive_redirect_drops_authorization(self):
        from unittest.mock import Mock
        from urllib.error import HTTPError
        from email.message import Message
        opener = Mock()
        headers = Message()
        headers['Location'] = 'https://signed.example/archive'
        redirect = HTTPError('https://api.github.com/a', 302, 'redirect', headers, io.BytesIO())
        opener.open.side_effect = redirect
        with patch.object(self.p, 'build_opener', return_value=opener), patch.object(self.p, 'urlopen', return_value=io.BytesIO(b'zip')) as download:
            self.assertEqual(self.p.api_transport('offline-secret')('/repos/o/r/actions/artifacts/1/zip', raw=True), b'zip')
            self.assertEqual(opener.open.call_args.args[0].get_header('Authorization'), 'Bearer offline-secret')
            self.assertIsNone(download.call_args.args[0].get_header('Authorization'))
            self.assertTrue(redirect.closed, 'redirect response must be closed')

    def test_transport_response_size_is_bounded(self):
        from unittest.mock import Mock
        opener = Mock()
        opener.open.return_value = io.BytesIO(b'x' * 524289)
        with patch.object(self.p, 'build_opener', return_value=opener):
            with self.assertRaises(ValueError):
                self.p.api_transport('offline')('/repos/o/r/actions/runs')


if __name__ == '__main__':
    unittest.main()
