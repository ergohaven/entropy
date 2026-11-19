#!/usr/bin/env python3
"""Conservative CI selection and immutable full-validation receipts (stdlib only)."""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess

import io
import zipfile
from datetime import datetime, timezone, timedelta
from urllib.error import HTTPError
from urllib.parse import urlparse, urlencode
from urllib.request import HTTPRedirectHandler, Request, build_opener, urlopen

PROFILES = ('build', 'nix', 'firmware', 'native', 'tests')


def git(*args):
    return subprocess.check_output(['git', *args], stderr=subprocess.DEVNULL).decode().strip()


def workflow(env):
    prefix = env['GITHUB_REPOSITORY'] + '/'
    ref = env['GITHUB_WORKFLOW_REF']
    path = ref.removeprefix(prefix).split('@', 1)[0]
    if not ref.startswith(prefix) or not re.fullmatch(r'\.github/workflows/[^/@]+\.ya?ml', path):
        raise ValueError('invalid workflow identity')
    return path


def record(profile, env):
    return dict(schema=1, repository=env['GITHUB_REPOSITORY'], workflow=workflow(env),
                profile=profile, run_id=int(env['GITHUB_RUN_ID']),
                run_attempt=int(env['GITHUB_RUN_ATTEMPT']), checkout_sha=git('rev-parse', 'HEAD'),
                tree=git('rev-parse', 'HEAD^{tree}'), scope='full')


def changed_paths(env, event):
    if git('rev-parse', '--is-shallow-repository') != 'false':
        raise ValueError('incomplete history')
    if env['GITHUB_EVENT_NAME'] == 'pull_request':
        base, head = event['pull_request']['base']['sha'], event['pull_request']['head']['sha']
        separator = '...'
    else:
        if event.get('created') or event.get('deleted'):
            raise ValueError('branch creation or deletion')
        base, head = event['before'], event['after']
        separator = '..'
    if not all(re.fullmatch(r'[0-9a-f]{40}', sha) and sha != '0' * 40 for sha in (base, head)):
        raise ValueError('invalid diff identities')
    raw = subprocess.check_output(['git', 'diff', '--name-only', '--no-renames', '-z',
                                   base + separator + head, '--'], stderr=subprocess.DEVNULL)
    return [p.decode('utf-8', errors='strict') for p in raw.split(b'\0') if p]


def decision(run=True, reason='full-validation-required', source_run='', artifact_url=''):
    return dict(run=run, reason=reason, source_run=source_run, artifact_url=artifact_url)


class NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


def api_transport(token):
    """Only GitHub API gets credentials; signed archive redirects never do."""
    def fetch(path, raw=False):
        if not path.startswith('/repos/'):
            raise ValueError('invalid API path')
        request = Request('https://api.github.com' + path, headers={
            'Authorization': 'Bearer ' + token, 'Accept': 'application/vnd.github+json',
            'X-GitHub-Api-Version': '2022-11-28', 'User-Agent': 'ci-policy'})
        try:
            response = build_opener(NoRedirect()).open(request, timeout=20)
        except HTTPError as error:
            with error:
                location = error.headers.get('Location', '')
                if not raw or error.code != 302 or urlparse(location).scheme != 'https':
                    raise
            response = urlopen(Request(location, headers={'User-Agent': 'ci-policy'}), timeout=20)
        with response:
            data = response.read(524289)
        if len(data) > 524288:
            raise ValueError('oversized response')
        return data if raw else json.loads(data)
    return fetch


def positive(value):
    return type(value) is int and value > 0


def bounded_list(payload, key, limit):
    rows, count = payload[key], payload['total_count']
    if type(count) is not int or not 0 <= count <= limit or not isinstance(rows, list) or len(rows) != count:
        raise ValueError('incomplete candidate list')
    return rows


def read_proof(data):
    if len(data) > 262144:
        raise ValueError('oversized archive')
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        entries = archive.infolist()
        if len(entries) != 1:
            raise ValueError('ambiguous archive')
        entry = entries[0]
        if not re.fullmatch(r'[A-Za-z0-9_.-]+\.json', entry.filename) or entry.file_size > 65536 or entry.flag_bits & 1:
            raise ValueError('unsafe archive')
        # read() verifies CRC; never extract entries to the filesystem.
        def unique(pairs):
            result = {}
            for key, value in pairs:
                if key in result:
                    raise ValueError('duplicate proof key')
                result[key] = value
            return result
        return json.loads(archive.read(entry), object_pairs_hook=unique)


def reuse(profile, env, tree, transport):
    repository, path = env['GITHUB_REPOSITORY'], workflow(env)
    if not re.fullmatch(r'[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+', repository):
        raise ValueError('invalid repository')
    base = '/repos/' + repository
    since = (datetime.now(timezone.utc) - timedelta(days=7)).strftime('%Y-%m-%dT%H:%M:%SZ')
    query = urlencode(dict(event='pull_request', per_page=50, created='>=' + since))
    runs = bounded_list(transport(base + '/actions/workflows/' + Path(path).name + '/runs?' + query), 'workflow_runs', 50)
    for listed in runs:
        if listed.get('path') != path or listed.get('event') != 'pull_request':
            continue
        run_id, attempt = listed['id'], listed['run_attempt']
        if not positive(run_id) or not positive(attempt):
            raise ValueError('invalid run identity')
        run = transport(f'{base}/actions/runs/{run_id}/attempts/{attempt}')
        if any(run.get(k) != listed.get(k) for k in ('id', 'run_attempt', 'path', 'event', 'head_sha', 'status', 'conclusion', 'repository')):
            raise ValueError('run identity mismatch')
        if run.get('status') != 'completed' or run.get('conclusion') != 'success':
            continue
        if run['repository']['full_name'] != repository or run['path'] != path or run['event'] != 'pull_request':
            raise ValueError('foreign run')
        # A receipt cannot nominate an arbitrary public tree from another PR.
        # Bind the successful run's immutable source to this exact tracked tree.
        # Behind-head synthetic merges conservatively get a new full validation.
        head_sha = run['head_sha']
        if not isinstance(head_sha, str) or not re.fullmatch(r'[0-9a-f]{40}', head_sha):
            raise ValueError('invalid source identity')
        source = transport(f'{base}/git/commits/{head_sha}')
        if source['sha'] != head_sha:
            raise ValueError('source identity mismatch')
        if source['tree']['sha'] != tree:
            continue
        artifacts = bounded_list(transport(f'{base}/actions/runs/{run_id}/artifacts?per_page=100'), 'artifacts', 100)
        matching = [a for a in artifacts if a.get('name') == f'ci-proof-{profile}-{run_id}-{attempt}']
        if len(matching) > 1:
            raise ValueError('ambiguous proof')
        if not matching:
            continue
        artifact = matching[0]
        if artifact.get('expired') is not False or datetime.fromisoformat(artifact['expires_at'].replace('Z', '+00:00')) <= datetime.now(timezone.utc):
            raise ValueError('expired proof')
        if not positive(artifact['id']) or not positive(artifact['size_in_bytes']) or artifact['size_in_bytes'] > 262144:
            raise ValueError('invalid artifact size or identity')
        origin = artifact['workflow_run']
        if origin['id'] != run_id or origin['repository_id'] != run['repository']['id'] or origin['head_sha'] != run['head_sha']:
            raise ValueError('artifact run mismatch')
        proof = read_proof(transport(f"{base}/actions/artifacts/{artifact['id']}/zip", raw=True))
        expected = dict(schema=1, repository=repository, workflow=path, profile=profile,
                        run_id=run_id, run_attempt=attempt, tree=tree, scope='full')
        if any(proof.get(k) != v or type(proof.get(k)) is not type(v) for k, v in expected.items()):
            raise ValueError('proof identity mismatch')
        sha = proof['checkout_sha']
        if not isinstance(sha, str) or not re.fullmatch(r'[0-9a-f]{40}', sha):
            raise ValueError('invalid checkout identity')
        commit = transport(f'{base}/git/commits/{sha}')
        if commit['sha'] != sha or commit['tree']['sha'] != tree:
            raise ValueError('checkout tree mismatch')
        url = f'https://github.com/{repository}/actions/runs/{run_id}'
        return decision(False, 'identical-tree-full-pr-proof', url + f'/attempts/{attempt}', url + f"/artifacts/{artifact['id']}")
    return decision(reason='no-matching-full-pr-proof')


def plan(profile, env, event, transport=None, *, manual_builds=False):
    try:
        git('rev-parse', 'HEAD^{tree}')
        if env.get('GITHUB_EVENT_NAME') not in ('push', 'pull_request') or env.get('GITHUB_REF', '').startswith('refs/tags/'):
            return decision(reason='event-requires-full-validation')
        ref = env.get('GITHUB_REF', '')
        if (env['GITHUB_EVENT_NAME'] == 'push' and not ref.startswith('refs/heads/')) or (env['GITHUB_EVENT_NAME'] == 'pull_request' and not re.fullmatch(r'refs/pull/[0-9]+/(merge|head)', ref)):
            return decision(reason='unknown-ref-requires-full-validation')
        if manual_builds:
            return decision(False, 'manual-build-request-required')
        paths = changed_paths(env, event)
        def doc(path):
            return path in ('README.md', 'README.rst', 'CHANGELOG.md', 'CHANGELOG.rst', 'AGENTS.md', 'LICENSE') or (path.startswith('docs/') and path.endswith(('.md', '.rst')))
        if paths and all(doc(p) for p in paths):
            return decision(False, 'documentation-only')
        if profile == 'firmware' and paths and all(doc(p) or p.startswith('tests/') or re.fullmatch(r'\.github/scripts/test_[^/]+\.py', p) for p in paths):
            return decision(False, 'firmware-test-or-documentation-only')
        if env['GITHUB_EVENT_NAME'] == 'push':
            token = env.get('GH_TOKEN') or env.get('GITHUB_TOKEN')
            if not token:
                return decision(reason='no-token-full-validation')
            return reuse(profile, env, git('rev-parse', 'HEAD^{tree}'), transport or api_transport(token))
        return decision()
    except Exception:
        return decision(reason='uncertain-requires-full-validation')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    rec = sub.add_parser('record')
    rec.add_argument('--profile', choices=PROFILES, required=True)
    rec.add_argument('--output', required=True)
    planner = sub.add_parser('plan')
    planner.add_argument('--profile', choices=PROFILES, required=True)
    planner.add_argument('--manual-builds', action='store_true')
    args = parser.parse_args()
    if args.command == 'record':
        proof = record(args.profile, os.environ)
        output = Path(args.output)
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(json.dumps(proof, sort_keys=True) + '\n')
        return
    try:
        event = json.loads(Path(os.environ['GITHUB_EVENT_PATH']).read_text())
        result = plan(args.profile, os.environ, event, manual_builds=args.manual_builds)
    except Exception:
        result = decision(reason='uncertain-requires-full-validation')
    lines = f"run={str(result['run']).lower()}\nreason={result['reason']}\nsource_run={result['source_run']}\n"
    if os.environ.get('GITHUB_OUTPUT'):
        with open(os.environ['GITHUB_OUTPUT'], 'a') as output:
            output.write(lines)
    else:
        print(lines, end='')
    if os.environ.get('GITHUB_STEP_SUMMARY'):
        with open(os.environ['GITHUB_STEP_SUMMARY'], 'a') as summary:
            summary.write(f"### CI policy: {args.profile}\n\n{result['reason']} (run={str(result['run']).lower()})\n\n")
            if result['source_run']:
                summary.write(f"Original full validation: {result['source_run']}\n\nProof artifact: {result['artifact_url']}\n\n")


if __name__ == '__main__':
    main()
