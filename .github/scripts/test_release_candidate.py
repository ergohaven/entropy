#!/usr/bin/env python3
"""Local TEST fixtures only: never contact GitHub or build release binaries."""
import hashlib
import importlib.util
import io
from contextlib import redirect_stderr, redirect_stdout
import json
import os
from pathlib import Path
import shutil
import sys
import tempfile
import zipfile
import warnings
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
spec = importlib.util.spec_from_file_location('release_candidate', ROOT / 'scripts/release_candidate.py')
rc = None
if Path(spec.origin).exists():
    rc = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(rc)


def digest(data):
    return hashlib.sha256(data).hexdigest()


class CandidateTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix='entropy-candidate-TEST-')
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.sha, self.tree = 'a' * 40, 'b' * 40
        self.tag = 'v0.8'
        self.env = dict(GITHUB_REPOSITORY='ergohaven/entropy', GITHUB_SHA=self.sha,
                        GITHUB_WORKFLOW_REF='ergohaven/entropy/.github/workflows/release.yml@refs/heads/main',
                        GITHUB_EVENT_NAME='workflow_dispatch', GITHUB_REF='refs/heads/main',
                        GITHUB_RUN_ID='31', GITHUB_RUN_ATTEMPT='2', GITHUB_ACTOR='kissetfall',
                        GITHUB_TRIGGERING_ACTOR='kissetfall')
        (self.root / 'scripts').mkdir()
        for name in ('public_version.sh', 'macos_stable_signing.sh'):
            shutil.copyfile(ROOT / 'scripts' / name, self.root / 'scripts' / name)
        (self.root / 'Cargo.toml').write_text('[package]\nversion = "0.8.0"\n[package.metadata.entropy]\npublic-version = "0.8"\n')
        (self.root / 'CHANGELOG.md').write_text('# Changes\n\n## v0.8\n\nTEST fixture notes.\n\n## v0.7\n\nOld notes.\n')
        event = self.root / 'event.json'
        event.write_text(json.dumps({'inputs': {'prepare_release': 'true', 'macos_signing_test': 'false'}}))
        self.env['GITHUB_EVENT_PATH'] = str(event)

    def activate(self):
        self.assertIsNotNone(rc, 'owning release-candidate tooling is missing')
        self.enterContext(patch.object(rc, 'ROOT', self.root))
        def fake_git(*args):
            # Explicit TEST subprocess boundary: no git commits or refs are written.
            return {('rev-parse', 'HEAD'): self.sha,
                    ('rev-parse', 'HEAD^{tree}'): self.tree,
                    ('status', '--porcelain', '--untracked-files=no'): '',
                    ('rev-parse', 'refs/tags/v0.8^{commit}'): self.sha}[args]
        self.enterContext(patch.object(rc, 'git', side_effect=fake_git))

    def fixture(self):
        assets, proofs, signing = [self.root / n for n in ('inputs', 'proof-inputs', 'signing-inputs')]
        assets.mkdir(); proofs.mkdir(); signing.mkdir()
        for suffix in ('x86_64.AppImage', 'amd64.deb', 'x86_64.rpm', 'x86_64.pkg.tar.zst',
                       'windows-x86_64.exe', 'macos-arm64.dmg', 'macos-x86_64.dmg'):
            (assets / f'entropy-{self.tag}-{suffix}').write_bytes(b'TEST fixture bytes\x00' + suffix.encode())
        for profile in ('build', 'nix'):
            folder = proofs / f'ci-proof-{profile}-31-2'
            folder.mkdir()
            (folder / f'{profile}.json').write_text(json.dumps(dict(schema=1, repository='ergohaven/entropy',
                workflow='.github/workflows/release.yml', profile=profile, run_id=31, run_attempt=2,
                checkout_sha=self.sha, tree=self.tree, scope='full')))
        pin = '50B8A2614631D0EE1705213D19D07A52C921D665'
        for arch, target in [('arm64', 'aarch64-apple-darwin'), ('x86_64', 'x86_64-apple-darwin')]:
            folder = signing / f'release-signing-evidence-{arch}'
            folder.mkdir()
            evidence = dict(source_sha=self.sha, target=target, certificate_sha1=pin,
                bundle_id='com.ergohaven.entropy', designated_requirement='=designated => anchor = H"' + pin.lower() + '" and identifier "com.ergohaven.entropy"',
                binary_and_bundle_signature='verified', dmg_integrity='verified', readonly_mounted_dmg='verified',
                applications_symlink='verified', temporary_keychain_deleted='yes', search_list_restored='yes')
            raw = ''.join(f'{k}={v}\n' for k,v in evidence.items()).encode()
            (folder / 'signing-evidence.txt').write_bytes(raw)
            dmg = (assets / f'entropy-{self.tag}-macos-{arch}.dmg').read_bytes()
            (folder / 'SHA256SUMS').write_text(f'{digest(raw)}  signing-evidence.txt\n{digest(dmg)}  entropy-signing-test-{arch}.dmg\n')
        return assets, proofs, signing

    def packed(self):
        assets, proofs, signing = self.fixture()
        output = self.root / 'candidate'
        rc.pack(assets, proofs, signing, output, self.tag, self.env)
        return output

    def test_manual_plan_requires_original_inputs_and_absent_tag_release(self):
        self.activate()
        class AbsentAPI:
            def __init__(self):
                self.reads = []
            def pages(self, path, key=None):
                return []
            def get(self, path):
                self.reads.append(path)
                return None
        api = AbsentAPI()
        self.assertTrue(hasattr(rc, 'plan'), 'release planning is missing')
        result = rc.plan(self.env, api)
        self.assertEqual(result, dict(tag=self.tag, prepare=True, candidate_run_id='', candidate_artifact_id=''))
        self.assertEqual(api.reads, ['repos/ergohaven/entropy/git/ref/tags/v0.8', 'repos/ergohaven/entropy/releases/tags/v0.8'])
        with patch.object(api, 'pages', return_value=[{'id':71,'tag_name':'v0.8','draft':True}]):
            with self.assertRaises(ValueError): rc.plan(self.env, api)
        for inputs in ({'prepare_release': 'false'}, {'prepare_release': 'true', 'macos_signing_test': 'true'}, {}):
            Path(self.env['GITHUB_EVENT_PATH']).write_text(json.dumps({'inputs': inputs}))
            with self.assertRaises(ValueError):
                rc.plan(self.env, api)

    def tag_env(self):
        env = dict(self.env, GITHUB_EVENT_NAME='push', GITHUB_REF='refs/tags/v0.8', GITHUB_RUN_ID='41')
        env['GITHUB_WORKFLOW_REF'] = 'ergohaven/entropy/.github/workflows/release.yml@refs/tags/v0.8'
        return env

    def source_run(self):
        return dict(id=31, run_attempt=2, head_sha=self.sha, head_branch='main',
            event='workflow_dispatch', path='.github/workflows/release.yml', status='completed',
            conclusion='success', repository={'full_name': 'ergohaven/entropy', 'id': 123},
            actor={'login': 'kissetfall'}, triggering_actor={'login': 'kissetfall'},
            created_at='2026-11-28T01:00:00Z')

    def artifact(self):
        return dict(id=99, name='release-candidate', size_in_bytes=5000, expired=False,
            expires_at='2099-01-01T00:00:00Z', digest='sha256:' + 'c' * 64,
            workflow_run={'id': 31, 'repository_id': 123, 'head_repository_id': 123,
                          'head_sha': self.sha, 'head_branch': 'main'})

    def test_tag_plan_requires_one_successful_bound_candidate_and_never_rebuilds(self):
        self.activate()
        run, artifact = self.source_run(), self.artifact()
        class SourceAPI:
            def pages(inner, path, key):
                return [run] if key == 'workflow_runs' else [artifact]
            def get(inner, path):
                return run
        api = SourceAPI()
        result = rc.plan(self.tag_env(), api)
        self.assertEqual(result, dict(tag=self.tag, prepare=False, candidate_run_id=31, candidate_artifact_id=99))
        for key, bad in [('head_sha', 'd'*40), ('path', '.github/workflows/other.yml'),
                         ('event', 'push'), ('head_branch', 'evil'), ('run_attempt', 0),
                         ('repository', {'full_name':'fork/entropy','id':123}),
                         ('actor', {'login':'other'}), ('triggering_actor', {'login':'other'}),
                         ('conclusion', 'failure')]:
            with self.subTest(key=key):
                original = run[key]; run[key] = bad
                with self.assertRaises(ValueError):
                    rc.plan(self.tag_env(), api)
                run[key] = original
        artifact['expired'] = True
        with self.assertRaises(ValueError): rc.plan(self.tag_env(), api)
        artifact['expired'] = False
        with patch.object(api, 'pages', return_value=[]):
            with self.assertRaises(ValueError): rc.plan(self.tag_env(), api)
        with patch.object(api, 'pages', side_effect=lambda path,key: [run] if key=='workflow_runs' else [artifact, artifact]):
            with self.assertRaises(ValueError): rc.plan(self.tag_env(), api)

    def test_repeated_prepare_reuses_candidate_even_after_a_successful_noop_dispatch(self):
        self.activate()
        run, artifact = self.source_run(), self.artifact()
        noop = dict(run, id=32, created_at='2026-11-28T02:00:00Z')
        class ReuseAPI:
            def pages(inner, path, key=None):
                if key == 'workflow_runs': return [noop, run]
                if key == 'artifacts': return [] if '/32/' in path else [artifact]
                if key == 'jobs': return [dict(id=100, run_id=32, name='Resolve frozen release candidate', status='completed', conclusion='success'),
                                          dict(id=101, run_id=32, name='Freeze qualified release candidate', status='completed', conclusion='skipped')]
                return []
            def get(inner, path):
                if '/tags/' in path: return None
                return noop if '/32' in path else run
        api = ReuseAPI()
        env = dict(self.env, GITHUB_RUN_ID='51')
        expected = dict(tag=self.tag, prepare=False, candidate_run_id=31, candidate_artifact_id=99)
        self.assertEqual(rc.plan(env, api), expected, 'a prepared exact-source candidate must not repeat qualification')
        self.assertEqual(rc.plan(self.tag_env(), api), expected, 'successful no-op dispatches must not hide the frozen candidate')
        output, summary = self.root/'reuse-output', self.root/'reuse-summary'
        with patch.object(rc, 'GitHub', return_value=api):
            self.assertEqual(rc.main(['plan'], dict(env, GITHUB_OUTPUT=str(output), GITHUB_STEP_SUMMARY=str(summary))), 0)
        self.assertIn('prepare=false', output.read_text())
        self.assertIn('actions/runs/31/artifacts/99', summary.read_text())
        with patch.object(api, 'pages', side_effect=lambda path,key=None: [run] if key=='workflow_runs' else [artifact,artifact] if key=='artifacts' else []):
            with self.assertRaises(ValueError): rc.plan(env, api)

    def test_expired_candidate_can_only_be_requalified_by_explicit_prepare(self):
        self.activate()
        run, artifact = self.source_run(), self.artifact()
        artifact.update(expired=True, expires_at='2000-01-01T00:00:00Z')
        class ExpiredAPI:
            def get(inner, path): return None if '/tags/' in path else run
            def pages(inner, path, key=None):
                return [run] if key == 'workflow_runs' else [artifact] if key == 'artifacts' else []
        try:
            result = rc.plan(self.env, ExpiredAPI())
        except ValueError:
            self.fail('an explicitly requested preparation must be allowed after normal candidate expiry')
        self.assertEqual(result, dict(tag=self.tag, prepare=True, candidate_run_id='', candidate_artifact_id=''))
        with self.assertRaises(ValueError): rc.plan(self.tag_env(), ExpiredAPI())
        artifact['digest'] = None
        with self.assertRaises(ValueError): rc.plan(self.env, ExpiredAPI())

    def zip_candidate(self, candidate):
        archive = self.root / 'original-api.zip'
        with zipfile.ZipFile(archive, 'w', zipfile.ZIP_DEFLATED) as z:
            for path in sorted(candidate.rglob('*')):
                if path.is_file(): z.write(path, path.relative_to(candidate).as_posix())
        return archive

    def test_fetch_validates_original_api_zip_and_stages_same_bytes(self):
        self.activate()
        candidate = self.packed()
        archive = self.zip_candidate(candidate)
        run, artifact = self.source_run(), self.artifact()
        artifact.update(digest='sha256:' + digest(archive.read_bytes()), size_in_bytes=archive.stat().st_size)
        class DownloadAPI:
            def get(inner, path): return artifact if '/artifacts/' in path else run
            def pages(inner, path, key): return [artifact]
            def download(inner, path, output, limit):
                self.assertEqual(path, 'repos/ergohaven/entropy/actions/artifacts/99/zip')
                shutil.copyfile(archive, output)
        api = DownloadAPI()
        self.assertTrue(hasattr(rc, 'fetch'), 'frozen candidate fetch is missing')
        output = self.root / 'fetched'
        rc.fetch(31, 99, output, self.tag_env(), api)
        for path in candidate.rglob('*'):
            if path.is_file(): self.assertEqual(path.read_bytes(), (output / path.relative_to(candidate)).read_bytes())
        artifact['digest'] = 'sha256:' + '0' * 64
        bad_output = self.root / 'bad-fetch'
        with self.assertRaises(ValueError): rc.fetch(31, 99, bad_output, self.tag_env(), api)
        self.assertFalse(bad_output.exists())

    def test_gh_transport_pages_all_rows_and_redacts_failed_subprocess(self):
        self.activate()
        self.assertTrue(hasattr(rc, 'GitHub'), 'GitHub transport is missing')
        api = rc.GitHub(dict(self.env, GH_TOKEN='TEST-secret-not-a-real-token'))
        calls = []
        def process(argv, **kwargs):
            calls.append(argv)
            self.assertNotIn('TEST-secret-not-a-real-token', ' '.join(argv))
            page = 2 if 'page=2' in argv[2] else 1
            rows = [{'id': n} for n in (range(1,101) if page == 1 else [101])]
            kwargs['stdout'].write(json.dumps({'total_count':101,'artifacts':rows}).encode())
            return type('TESTProcess', (), {'returncode':0, 'stderr':b''})()
        with patch.object(rc.subprocess, 'run', side_effect=process):
            rows = api.pages('repos/ergohaven/entropy/actions/runs/31/artifacts', 'artifacts')
        self.assertEqual(len(rows), 101)
        self.assertEqual(len(calls), 2)
        def failed(argv, **kwargs):
            return type('TESTProcess', (), {'returncode':1, 'stderr':b'bad TEST-secret-not-a-real-token https://signed.invalid/secret'})()
        with patch.object(rc.subprocess, 'run', side_effect=failed):
            with self.assertRaises(ValueError) as caught: api.get('user')
            self.assertNotIn('TEST-secret', str(caught.exception))
            self.assertNotIn('https://', str(caught.exception))
        with self.assertRaises(ValueError): api.get('https://evil.invalid/')

    def publication_api(self, candidate):
        test = self
        class PublicationAPI:
            # In-memory TEST API, intentionally not a successful hosted run.
            def __init__(inner):
                inner.release = None; inner.assets = {}; inner.bytes = {}
                inner.events = []; inner.login = 'kissetfall'; inner.latest = None
            def get(inner, path):
                inner.events.append(('GET', path))
                if path == 'user': return {'login': inner.login}
                if '/git/ref/tags/' in path: return {'ref':'refs/tags/v0.8', 'object':{'type':'commit','sha':test.sha}}
                if path.endswith('/releases/latest'): return inner.latest
                if '/releases/assets/' in path:
                    return next(a for a in inner.assets.values() if str(a['id']) == path.rsplit('/',1)[1])
                if '/releases/tags/' in path and inner.release and inner.release['draft']:
                    return None  # GitHub's tag lookup documents published releases only.
                if '/releases/' in path: return inner.release
                raise AssertionError('unexpected TEST GET ' + path)
            def pages(inner, path, key=None):
                if path == 'repos/ergohaven/entropy/releases':
                    return [inner.release] if inner.release else []
                return list(inner.assets.values())
            def mutate(inner, method, path, payload=None, source=None):
                test.assertEqual(inner.events[-1], ('GET', 'user'), 'identity must be checked before EVERY write')
                inner.events.append((method, path, payload))
                test.assertEqual(inner.login, 'kissetfall')
                if method == 'POST' and path.endswith('/releases'):
                    test.assertEqual(payload, dict(tag_name='v0.8', target_commitish=test.sha, name='Entropy v0.8',
                        body='TEST fixture notes.', draft=True, prerelease=False, make_latest='false', generate_release_notes=False))
                    inner.release = dict(payload, id=71, author={'login':'kissetfall'})
                    return inner.release
                if method == 'POST' and '/assets?name=' in path:
                    name = Path(source).name
                    data = Path(source).read_bytes()
                    row = dict(id=100+len(inner.assets), name=name, size=len(data), state='uploaded', uploader={'login':'kissetfall'},
                        browser_download_url=f'https://github.com/ergohaven/entropy/releases/download/v0.8/{name}')
                    inner.assets[name] = row; inner.bytes[name] = data
                    return row
                if method == 'PATCH':
                    test.assertEqual(payload, dict(tag_name='v0.8', target_commitish=test.sha,
                        name='Entropy v0.8', body='TEST fixture notes.', draft=False, prerelease=False, make_latest='true'))
                    test.assertEqual(len(inner.assets), 8)
                    test.assertEqual({e[1] for e in inner.events if e[0]=='DOWNLOAD'}, set(inner.assets))
                    inner.release = dict(inner.release, draft=False)
                    inner.latest = inner.release
                    return inner.release
                raise AssertionError('unexpected TEST write')
            def download(inner, path, output, limit):
                row = next(a for a in inner.assets.values() if str(a['id']) == path.rsplit('/',1)[1])
                inner.events.append(('DOWNLOAD', row['name']))
                Path(output).write_bytes(inner.bytes[row['name']])
            def anonymous(inner, url, output, limit):
                name = url.rsplit('/',1)[1]
                inner.events.append(('ANONYMOUS', name))
                Path(output).write_bytes(inner.bytes[name])
        return PublicationAPI()

    def test_publish_draft_verify_stored_bytes_then_public_verify_and_idempotent_retry(self):
        self.activate()
        candidate = self.packed()
        api = self.publication_api(candidate)
        self.assertTrue(hasattr(rc, 'publish'), 'candidate publication is missing')
        api.latest = dict(id=70, tag_name='v0.7', draft=False, prerelease=False)
        result = rc.publish(candidate, self.tag_env(), api, self.root/'receipts')
        self.assertEqual(result['status'], 'published-verified')
        self.assertEqual(len(api.assets), 8)
        self.assertEqual(sum(e[0]=='ANONYMOUS' for e in api.events), 8)
        writes = sum(e[0] in ('POST', 'PATCH') for e in api.events)
        api.latest = dict(api.release, id=999, tag_name='v0.9')
        result = rc.publish(candidate, self.tag_env(), api, self.root/'retry-receipts')
        self.assertEqual(result['status'], 'already-published-verified')
        self.assertEqual(result['latest_release_id'], 999)
        self.assertEqual(sum(e[0] in ('POST','PATCH') for e in api.events), writes)
        self.assertTrue(list((self.root/'receipts').glob('*.json')))

    def test_delayed_candidate_never_rolls_latest_back_before_or_during_publication(self):
        self.activate()
        candidate = self.packed()
        newer = dict(id=999, tag_name='v0.9', draft=False, prerelease=False)
        for timing in ('before', 'during'):
            with self.subTest(timing=timing):
                api = self.publication_api(candidate)
                if timing == 'before':
                    api.latest = newer
                else:
                    original = api.download
                    def concurrently_published(path, output, limit):
                        original(path, output, limit)
                        api.latest = newer
                    api.download = concurrently_published
                with self.assertRaises(ValueError):
                    rc.publish(candidate, self.tag_env(), api, self.root/('delayed-'+timing))
                self.assertEqual(api.latest['id'], 999)
                self.assertFalse(any(e[0]=='PATCH' for e in api.events))
                if timing == 'before':
                    self.assertFalse(any(e[0]=='POST' for e in api.events))

    def test_gh_write_boundary_rechecks_actor_and_anonymous_reads_have_no_auth(self):
        self.activate()
        api = rc.GitHub(dict(self.env, GH_TOKEN='TEST-secret-not-a-real-token'))
        self.assertTrue(hasattr(api, 'mutate') and hasattr(api, 'anonymous'), 'write/public transport missing')
        with patch.object(api, 'get', return_value={'login':'not-kissetfall'}), patch.object(api, 'request') as request:
            with self.assertRaises(ValueError): api.mutate('POST', 'repos/ergohaven/entropy/releases', payload={})
            request.assert_not_called()
        with patch.object(api, 'get', return_value={'login':'kissetfall'}), patch.object(api, 'request', return_value={'id':71}) as request:
            self.assertEqual(api.mutate('POST','repos/ergohaven/entropy/releases',payload={'draft':True}), {'id':71})
            self.assertEqual(request.call_args.kwargs['method'], 'POST')
        class Response:
            def __enter__(inner): return inner
            def __exit__(inner, *args): pass
            def read(inner, count):
                value = getattr(inner, 'data', b'TEST public bytes')
                inner.data = b''
                return value
        class Opener:
            def open(inner, request, timeout):
                self.assertIsNone(request.get_header('Authorization'))
                self.assertEqual(timeout, 20)
                return Response()
        with patch.object(rc, 'build_opener', return_value=Opener()):
            output = self.root/'anonymous.bin'
            api.anonymous('https://github.com/ergohaven/entropy/releases/download/v0.8/test', output, 100)
            self.assertEqual(output.read_bytes(), b'TEST public bytes')
        with self.assertRaises(ValueError): api.anonymous('http://evil.invalid/file', self.root/'bad', 10)

    def test_partial_workflow_owned_draft_is_found_even_when_tag_lookup_is_404(self):
        self.activate()
        candidate = self.packed()
        api = self.publication_api(candidate)
        api.release = dict(id=71, tag_name='v0.8', target_commitish=self.sha, name='Entropy v0.8',
            body='TEST fixture notes.', draft=True, prerelease=False, author={'login':'kissetfall'})
        rc.publish(candidate, self.tag_env(), api, self.root/'partial-receipts')
        self.assertFalse(any(e[0]=='POST' and e[1].endswith('/releases') for e in api.events), 'must reuse original draft, not create a second draft')

    def test_cli_plan_emits_contract_and_pack_rejects_tag_context(self):
        self.activate()
        self.assertTrue(hasattr(rc, 'main'), 'command line owner missing')
        output = self.root/'github-output'
        env = dict(self.env, GITHUB_OUTPUT=str(output))
        class AbsentAPI:
            def get(inner, path): return None
            def pages(inner, path, key=None): return []
        with patch.object(rc, 'GitHub', return_value=AbsentAPI()):
            self.assertEqual(rc.main(['plan'], env), 0)
        self.assertEqual(output.read_text(), 'tag=v0.8\nprepare=true\ncandidate_run_id=\ncandidate_artifact_id=\n')
        assets, proofs, signing = self.fixture()
        with self.assertRaises(ValueError):
            rc.pack(assets, proofs, signing, self.root/'not-manual', self.tag, dict(self.tag_env(), GITHUB_RUN_ID='31'))

    def test_candidate_rejects_source_tree_version_inventory_and_byte_corruption(self):
        self.activate()
        original = self.packed()
        cases = ['sha', 'tree', 'version', 'repository', 'workflow', 'asset', 'checksum', 'proof', 'extra', 'empty', 'symlink', 'duplicate-json']
        for index, case in enumerate(cases):
            with self.subTest(case=case):
                candidate = self.root / f'bad-{index}'
                shutil.copytree(original, candidate)
                manifest_path = candidate/'candidate.json'
                manifest = json.loads(manifest_path.read_text())
                if case in ('sha','tree','version','repository','workflow'):
                    key = 'source_sha' if case == 'sha' else case
                    manifest[key] = {'sha':'c'*40, 'tree':'c'*40, 'version':'0.9', 'repository':'fork/entropy', 'workflow':'other.yml'}[case]
                    manifest_path.write_text(json.dumps(manifest))
                elif case == 'asset':
                    next((candidate/'assets').glob('*.AppImage')).write_bytes(b'CORRUPTED TEST binary')
                elif case == 'checksum':
                    (candidate/'assets/SHA256SUMS.txt').write_text('0'*64 + '  wrong\n')
                elif case == 'proof':
                    path = candidate/'proofs/build.json'
                    data = json.loads(path.read_text()); data['scope'] = 'partial'; path.write_text(json.dumps(data))
                elif case == 'extra': (candidate/'unexpected-script.py').write_text('TEST fixture')
                elif case == 'empty':
                    for path in (candidate/'assets').iterdir(): path.unlink()
                elif case == 'symlink':
                    path = candidate/'proofs/nix.json'; path.unlink(); path.symlink_to(original/'proofs/nix.json')
                elif case == 'duplicate-json':
                    path = candidate/'proofs/build.json'; path.write_text(path.read_text().replace('{','{"schema":1,',1))
                output = self.root / f'bad-staging-{index}'
                with self.assertRaises(ValueError): rc.stage(candidate, output, self.env)
                self.assertFalse(output.exists())

    def test_context_rejects_wrong_checkout_workflow_repository_dirty_tree_and_version(self):
        self.activate()
        for key, value in [('GITHUB_SHA','c'*40), ('GITHUB_REPOSITORY','fork/entropy'),
                           ('GITHUB_WORKFLOW_REF','ergohaven/entropy/.github/workflows/other.yml@refs/heads/main')]:
            with self.subTest(key=key):
                with self.assertRaises(ValueError): rc.context(dict(self.env, **{key:value}))
        with patch.object(rc, 'git', side_effect=lambda *a: 'dirty' if a[0]=='status' else self.sha):
            with self.assertRaises(ValueError): rc.context(self.env)
        (self.root/'Cargo.toml').write_text('[package]\nversion = "0.8.0"\n[package.metadata.entropy]\npublic-version = "0.9"\n')
        with self.assertRaises(ValueError): rc.context(self.env)

    def test_native_evidence_markers_and_both_dmg_checksums_are_enforced(self):
        self.activate()
        original = self.packed()
        for index, (arch, field) in enumerate((a,k) for a in ('arm64','x86_64') for k in
            ('source_sha','target','certificate_sha1','bundle_id','designated_requirement','binary_and_bundle_signature',
             'dmg_integrity','readonly_mounted_dmg','applications_symlink','temporary_keychain_deleted','search_list_restored','dmg-checksum')):
            with self.subTest(arch=arch, field=field):
                candidate = self.root/f'bad-signing-{index}'
                shutil.copytree(original, candidate)
                folder = candidate/'signing'/arch
                evidence = folder/'signing-evidence.txt'
                if field != 'dmg-checksum':
                    lines = [line if not line.startswith(field+'=') else field+'=wrong' for line in evidence.read_text().splitlines()]
                    evidence.write_text('\n'.join(lines)+'\n')
                dmg_hash = digest((candidate/'assets'/f'entropy-v0.8-macos-{arch}.dmg').read_bytes()) if field != 'dmg-checksum' else '0'*64
                (folder/'SHA256SUMS').write_text(f'{digest(evidence.read_bytes())}  signing-evidence.txt\n{dmg_hash}  entropy-signing-test-{arch}.dmg\n')
                with self.assertRaises(ValueError): rc.validate_candidate(candidate, self.env)

    def test_pack_rejects_duplicate_proofs_unknown_empty_and_symlink_assets(self):
        self.activate()
        assets, proofs, signing = self.fixture()
        duplicate = proofs/'nested/ci-proof-build-31-2'
        duplicate.mkdir(parents=True)
        shutil.copyfile(proofs/'ci-proof-build-31-2/build.json', duplicate/'build.json')
        with self.assertRaises(ValueError): rc.pack(assets, proofs, signing, self.root/'bad-duplicate', self.tag, self.env)
        shutil.rmtree(proofs/'nested')
        extra = assets/'unexpected.bin'; extra.write_bytes(b'TEST')
        with self.assertRaises(ValueError): rc.pack(assets, proofs, signing, self.root/'bad-extra', self.tag, self.env)
        extra.unlink()
        binary = next(assets.glob('*.AppImage')); raw=binary.read_bytes(); binary.write_bytes(b'')
        with self.assertRaises(ValueError): rc.pack(assets, proofs, signing, self.root/'bad-empty', self.tag, self.env)
        binary.unlink(); target=self.root/'TEST-original'; target.write_bytes(raw); binary.symlink_to(target)
        with self.assertRaises(ValueError): rc.pack(assets, proofs, signing, self.root/'bad-symlink', self.tag, self.env)

    def test_zip_preflight_rejects_traversal_alias_duplicates_symlinks_counts_and_crc(self):
        self.activate()
        candidate = self.packed()
        originals = [(p.relative_to(candidate).as_posix(),p.read_bytes()) for p in sorted(candidate.rglob('*')) if p.is_file()]
        for index, name in enumerate(('../escaped','/absolute','assets/../escaped','assets//alias','assets/./alias','assets\\alias','C:/escaped', 'candidate.json')):
            archive=self.root/f'unsafe-{index}.zip'
            with warnings.catch_warnings():
                warnings.simplefilter('ignore', UserWarning)
                with zipfile.ZipFile(archive,'w') as z:
                    for number,(n,data) in enumerate(originals): z.writestr(name if number==0 else n,data)
            with self.assertRaises((ValueError,zipfile.BadZipFile)): rc.unpack(archive,self.root/f'unsafe-output-{index}')
            self.assertFalse((self.root/'escaped').exists())
        archive=self.root/'symlink.zip'
        with zipfile.ZipFile(archive,'w') as z:
            for number,(n,data) in enumerate(originals):
                entry=zipfile.ZipInfo(n); entry.create_system=3; entry.external_attr=((0o120777 if number==0 else 0o100644)<<16)
                z.writestr(entry,data)
        with self.assertRaises(ValueError): rc.unpack(archive,self.root/'symlink-output')
        archive=self.root/'truncated-inventory.zip'
        with zipfile.ZipFile(archive,'w') as z: z.writestr('candidate.json',b'TEST')
        with self.assertRaises(ValueError): rc.unpack(archive,self.root/'truncated-output')
        archive=self.root/'crc.zip'
        with zipfile.ZipFile(archive,'w',zipfile.ZIP_STORED) as z:
            for name,data in originals: z.writestr(name,data)
        import struct
        data=bytearray(archive.read_bytes()); name_len,extra_len=struct.unpack_from('<HH',data,26)
        data[30+name_len+extra_len] ^= 1; archive.write_bytes(data)
        with self.assertRaises(zipfile.BadZipFile): rc.unpack(archive,self.root/'crc-output')
        clean=self.zip_candidate(candidate)
        with patch.object(rc,'MAX_FILE',1):
            with self.assertRaises(ValueError): rc.unpack(clean,self.root/'size-output')

    def test_artifact_binding_rejects_wrong_digest_expiry_source_repository_and_run(self):
        self.activate()
        run = self.source_run()
        for key, value in [('expired',True), ('expires_at','2000-01-01T00:00:00Z'), ('digest',None),
                           ('name','other'), ('size_in_bytes',0), ('id',0)]:
            artifact=self.artifact(); artifact[key]=value
            with self.subTest(key=key):
                with self.assertRaises(ValueError): rc.check_artifact(artifact,run)
        for key,value in [('id',32),('head_sha','d'*40),('repository_id',456),('head_repository_id',456),('head_branch','other')]:
            artifact=self.artifact(); artifact['workflow_run'][key]=value
            with self.subTest(key=key):
                with self.assertRaises(ValueError): rc.check_artifact(artifact,run)

    def test_publish_rejects_unknown_drafts_partial_conflicts_and_wrong_actor(self):
        self.activate()
        candidate=self.packed()
        original = dict(id=71,tag_name='v0.8',target_commitish=self.sha,name='Entropy v0.8',body='TEST fixture notes.',
                        draft=True,prerelease=False,author={'login':'kissetfall'})
        for index,(key,value) in enumerate([('target_commitish','c'*40),('body','unknown'),('author',{'login':'other'}),('name','unknown')]):
            api=self.publication_api(candidate); api.release=dict(original,**{key:value})
            with self.subTest(key=key):
                with self.assertRaises(ValueError): rc.publish(candidate,self.tag_env(),api,self.root/f'conflict-{index}')
                self.assertFalse(any(e[0] in ('POST','PATCH') for e in api.events))
        api=self.publication_api(candidate); api.login='not-kissetfall'
        with self.assertRaises(ValueError): rc.publish(candidate,self.tag_env(),api,self.root/'wrong-actor')
        self.assertFalse(any(e[0] in ('POST','PATCH') for e in api.events))
        api=self.publication_api(candidate); api.release=original
        name=next((candidate/'assets').iterdir()).name; raw=(candidate/'assets'/name).read_bytes()
        api.assets[name]=dict(id=100,name=name,size=len(raw),state='uploaded',uploader={'login':'kissetfall'})
        api.bytes[name]=b'X'*len(raw)
        with self.assertRaises(ValueError): rc.publish(candidate,self.tag_env(),api,self.root/'wrong-existing-bytes')
        self.assertFalse(any(e[0] in ('POST','PATCH') for e in api.events))
        api.bytes[name]=raw; api.assets[name]['uploader']={'login':'other'}
        with self.assertRaises(ValueError): rc.publish(candidate,self.tag_env(),api,self.root/'wrong-uploader')
        self.assertFalse(any(e[0] in ('POST','PATCH') for e in api.events))

    def test_cli_reports_safe_validation_reason_without_transport_or_secret_details(self):
        self.activate()
        candidate = self.packed()
        path = candidate/'candidate.json'
        manifest = json.loads(path.read_text())
        manifest['version'] = '0.9'
        path.write_text(json.dumps(manifest))
        with redirect_stderr(io.StringIO()) as errors:
            self.assertEqual(rc.main(['stage', '--candidate', str(candidate), '--output', str(self.root/'rejected')], self.env), 1)
        self.assertIn('candidate source/tree/version mismatch', errors.getvalue())
        self.assertIn('details redacted', errors.getvalue())

    def test_missing_candidate_never_constructs_api_or_builds(self):
        self.activate()
        with patch.object(rc,'GitHub') as api, patch.object(rc.subprocess,'run') as process, redirect_stderr(io.StringIO()) as errors:
            self.assertEqual(rc.main(['publish','--candidate',str(self.root/'missing')],self.tag_env()),1)
            self.assertIn('details redacted', errors.getvalue())
            api.assert_not_called(); process.assert_not_called()

    def test_manual_inputs_reject_numeric_booleans_and_signing_rejects_unknown_fields(self):
        self.activate()
        Path(self.env['GITHUB_EVENT_PATH']).write_text(json.dumps({'inputs': {'prepare_release':1,'macos_signing_test':0}}))
        with self.assertRaises(ValueError): rc.manual_context(self.env)
        Path(self.env['GITHUB_EVENT_PATH']).write_text(json.dumps({'inputs': {'prepare_release':True,'macos_signing_test':False}}))
        candidate=self.packed()
        folder=candidate/'signing/arm64'
        evidence=folder/'signing-evidence.txt'
        evidence.write_bytes(evidence.read_bytes()+b'unknown_secret=TEST-not-allowed\n')
        dmg=(candidate/'assets/entropy-v0.8-macos-arm64.dmg').read_bytes()
        (folder/'SHA256SUMS').write_text(f'{digest(evidence.read_bytes())}  signing-evidence.txt\n{digest(dmg)}  entropy-signing-test-arm64.dmg\n')
        with self.assertRaises(ValueError): rc.validate_candidate(candidate,self.env)

    def test_identical_partial_upload_is_reused_and_published_missing_inventory_is_not_repaired(self):
        self.activate()
        candidate=self.packed()
        api=self.publication_api(candidate)
        api.release=dict(id=71,tag_name='v0.8',target_commitish=self.sha,name='Entropy v0.8',body='TEST fixture notes.',
                         draft=True,prerelease=False,author={'login':'kissetfall'})
        asset=next((candidate/'assets').iterdir()); name=asset.name; raw=asset.read_bytes()
        api.assets[name]=dict(id=100,name=name,size=len(raw),state='uploaded',uploader={'login':'kissetfall'},
                             browser_download_url=f'https://github.com/ergohaven/entropy/releases/download/v0.8/{name}')
        api.bytes[name]=raw
        rc.publish(candidate,self.tag_env(),api,self.root/'reuse-receipts')
        uploads=[e for e in api.events if e[0]=='POST' and '/assets?name=' in e[1]]
        self.assertEqual(len(uploads),7)
        self.assertFalse(any(e[1].endswith('name='+name) for e in uploads))
        api.events=[]; api.assets.pop(name)
        with self.assertRaises(ValueError): rc.publish(candidate,self.tag_env(),api,self.root/'incomplete-published')
        self.assertFalse(any(e[0] in ('POST','PATCH') for e in api.events))

    def test_bad_stored_bytes_block_publication_and_bad_public_bytes_never_roll_back(self):
        self.activate()
        candidate=self.packed()
        api=self.publication_api(candidate)
        def corrupted(path,output,limit): Path(output).write_bytes(b'CORRUPTED TEST stored bytes')
        with patch.object(api,'download',side_effect=corrupted):
            with self.assertRaises(ValueError): rc.publish(candidate,self.tag_env(),api,self.root/'stored-corruption')
        self.assertTrue(api.release['draft'])
        self.assertFalse(any(e[0]=='PATCH' for e in api.events))
        api=self.publication_api(candidate)
        with patch.object(api,'anonymous',side_effect=corrupted):
            with self.assertRaises(ValueError): rc.publish(candidate,self.tag_env(),api,self.root/'public-corruption')
        self.assertFalse(api.release['draft'])
        self.assertEqual(sum(e[0]=='PATCH' for e in api.events),1)
        self.assertTrue((self.root/'public-corruption'/'012-failed-no-rollback.json').exists())

    def test_gh_absence_requires_explicit_404_and_pagination_rejects_duplicates_or_truncation(self):
        self.activate()
        api=rc.GitHub(dict(self.env,GH_TOKEN='TEST-secret-not-a-real-token'))
        def not_found(argv,**kwargs):
            return type('TESTProcess',(),{'returncode':1,'stderr':b'gh: Not Found (HTTP 404)\n'})()
        with patch.object(rc.subprocess,'run',side_effect=not_found): self.assertIsNone(api.get('repos/ergohaven/entropy/releases/tags/v0.8'))
        def forbidden(argv,**kwargs):
            return type('TESTProcess',(),{'returncode':1,'stderr':b'gh: Forbidden (HTTP 403)\n'})()
        with patch.object(rc.subprocess,'run',side_effect=forbidden):
            with self.assertRaises(ValueError): api.get('repos/ergohaven/entropy/releases/tags/v0.8')
        for payload in ({'total_count':2,'artifacts':[{'id':1}]}, {'total_count':2,'artifacts':[{'id':1},{'id':1}]}):
            with patch.object(api,'get',return_value=payload):
                with self.assertRaises(ValueError): api.pages('repos/ergohaven/entropy/actions/runs/31/artifacts','artifacts')

    def test_fresh_paths_reject_normalized_overlap_and_manifest_hash_metadata_changes(self):
        self.activate()
        assets, proofs, signing=self.fixture()
        (self.root/'outputs').mkdir()
        with self.assertRaises(ValueError): rc.fresh(self.root/'outputs/../inputs/new-output',assets)
        candidate=self.root/'candidate'
        rc.pack(assets,proofs,signing,candidate,self.tag,self.env)
        path=candidate/'candidate.json'; manifest=json.loads(path.read_text()); manifest['assets'][0]['sha256']='0'*64
        path.write_text(json.dumps(manifest))
        with self.assertRaises(ValueError): rc.stage(candidate,self.root/'bad-manifest-hash',self.env)

    def test_cli_end_to_end_pack_original_zip_fetch_stage_and_publish_is_byte_preserving(self):
        self.activate()
        assets,proofs,signing=self.fixture()
        candidate=self.root/'cli-candidate'
        with redirect_stdout(io.StringIO()):
            self.assertEqual(rc.main(['pack','--assets',str(assets),'--proofs',str(proofs),'--signing',str(signing),
                                      '--output',str(candidate),'--tag',self.tag],self.env),0)
        archive=self.zip_candidate(candidate)
        artifact=self.artifact(); artifact.update(size_in_bytes=archive.stat().st_size,digest='sha256:'+digest(archive.read_bytes()))
        run=self.source_run()
        class OriginalArchiveAPI:
            def get(inner,path): return artifact if '/artifacts/' in path else run
            def pages(inner,path,key=None): return [artifact]
            def download(inner,path,output,limit): shutil.copyfile(archive,output)
        fetched,staged=self.root/'cli-fetched',self.root/'cli-staged'
        with patch.object(rc,'GitHub',return_value=OriginalArchiveAPI()), redirect_stdout(io.StringIO()):
            self.assertEqual(rc.main(['fetch','--run-id','31','--artifact-id','99','--output',str(fetched)],self.tag_env()),0)
            self.assertEqual(rc.main(['stage','--candidate',str(fetched),'--output',str(staged)],self.tag_env()),0)
        api=self.publication_api(fetched)
        with patch.object(rc,'GitHub',return_value=api), redirect_stdout(io.StringIO()):
            self.assertEqual(rc.main(['publish','--candidate',str(fetched),'--output',str(self.root/'cli-receipts')],self.tag_env()),0)
        self.assertEqual(set(api.bytes),{p.name for p in staged.iterdir()})
        for name,data in api.bytes.items():
            self.assertEqual(data,(candidate/'assets'/name).read_bytes())
            self.assertEqual(data,(staged/name).read_bytes())

    def test_latest_qualified_source_without_candidate_fails_instead_of_using_older_bytes(self):
        self.activate()
        older=self.source_run(); newer=dict(older,id=32,run_attempt=1,created_at='2026-11-28T02:00:00Z')
        requested=[]
        class SourceAPI:
            def get(inner,path):
                requested.append(path)
                return newer if '/runs/32' in path else older
            def pages(inner,path,key=None):
                if key=='workflow_runs': return [older,newer]
                if key=='jobs': return [dict(id=100,run_id=32,name='Full release validation / Build (macos-15)',status='completed',conclusion='success')]
                return [] if '/runs/32/' in path else [self.artifact()]
        with self.assertRaises(ValueError): rc.plan(self.tag_env(),SourceAPI())
        self.assertFalse(any('/runs/31' in path for path in requested))

    def test_remote_annotated_tag_is_peeled_and_wrong_commit_is_rejected(self):
        self.activate()
        identity=rc.context(self.tag_env()); tag_sha='d'*40
        class TagAPI:
            def get(inner,path):
                if '/git/ref/' in path: return {'ref':'refs/tags/v0.8','object':{'type':'tag','sha':tag_sha}}
                return {'sha':tag_sha,'object':{'type':'commit','sha':self.sha}}
        api=TagAPI()
        rc.remote_tag(api,identity)
        with patch.object(api,'get',return_value={'ref':'refs/tags/v0.8','object':{'type':'commit','sha':'c'*40}}):
            with self.assertRaises(ValueError): rc.remote_tag(api,identity)

    def test_gh_transport_keeps_zip_bytes_and_typed_mutation_payloads_without_secret_argv(self):
        self.activate()
        api=rc.GitHub(dict(self.env,GH_TOKEN='TEST-secret-not-a-real-token'))
        raw=b'PK\x03\x04\x00\xffTEST binary original ZIP bytes'
        calls=[]
        def process(argv,**kwargs):
            calls.append((argv,kwargs.get('input')))
            self.assertNotIn('TEST-secret-not-a-real-token',' '.join(argv))
            if argv[2]=='user': data=json.dumps({'login':'kissetfall'}).encode()
            elif '/zip' in argv[2]:
                self.assertIn('Accept: application/vnd.github+json', argv, 'Actions archive endpoint returns a redirect, not an octet-stream API resource')
                data=raw
            elif '/releases/assets/100' in argv[2]:
                self.assertIn('Accept: application/octet-stream', argv)
                data=raw
            else: data=json.dumps({'id':71}).encode()
            kwargs['stdout'].write(data)
            return type('TESTProcess',(),{'returncode':0,'stderr':b''})()
        with patch.object(rc.subprocess,'run',side_effect=process):
            destination=self.root/'downloaded.zip'
            api.download('repos/ergohaven/entropy/actions/artifacts/99/zip',destination,len(raw))
            self.assertEqual(destination.read_bytes(),raw)
            public_asset=self.root/'stored-asset.bin'
            api.download('repos/ergohaven/entropy/releases/assets/100',public_asset,len(raw))
            self.assertEqual(public_asset.read_bytes(),raw)
            payload={'draft':True,'prerelease':False,'make_latest':'false'}
            api.mutate('POST','repos/ergohaven/entropy/releases',payload=payload)
            self.assertEqual(json.loads(calls[-1][1]),payload)
            asset=self.root/'TEST.bin'; asset.write_bytes(raw)
            api.mutate('POST','repos/ergohaven/entropy/releases/71/assets?name=TEST.bin',source=asset)
            self.assertEqual(calls[-1][0][2],'https://uploads.github.com/repos/ergohaven/entropy/releases/71/assets?name=TEST.bin')
            self.assertIn(str(asset),calls[-1][0])
            self.assertEqual(sum(argv[2]=='user' for argv,data in calls),2)

    def test_pack_stage_preserves_all_frozen_bytes(self):
        self.activate()
        candidate = self.packed()
        files = {p.relative_to(candidate).as_posix() for p in candidate.rglob('*') if p.is_file()}
        self.assertEqual(len(files), 15)
        manifest = json.loads((candidate / 'candidate.json').read_text())
        self.assertEqual(manifest['source_sha'], self.sha)
        self.assertEqual(manifest['tree'], self.tree)
        self.assertEqual(manifest['run_attempt'], 2)
        staged = self.root / 'public'
        rc.stage(candidate, staged, self.env)
        self.assertEqual(len(list(staged.iterdir())), 8)
        for asset in manifest['assets']:
            self.assertEqual((staged / asset['name']).read_bytes(), (candidate / 'assets' / asset['name']).read_bytes())
            self.assertEqual(digest((staged / asset['name']).read_bytes()), asset['sha256'])


if __name__ == '__main__':
    unittest.main()
