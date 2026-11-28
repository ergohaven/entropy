#!/usr/bin/env python3
"""Qualify a frozen Entropy candidate; promote its original bytes, never rebuild.

Only publish mutates GitHub. All transport errors are deliberately redacted.
The pack/fetch/stage outputs must be fresh, disjoint directories.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import stat
import subprocess
import sys
import tempfile
import time
from urllib.parse import quote, urlparse
from urllib.request import HTTPRedirectHandler, Request, build_opener
import zipfile

from prepare_release_assets import collect

ROOT = Path(__file__).resolve().parents[1]
REPOSITORY = 'ergohaven/entropy'
WORKFLOW = '.github/workflows/release.yml'
BASE = 'repos/' + REPOSITORY
ARCHES = {'arm64': 'aarch64-apple-darwin', 'x86_64': 'x86_64-apple-darwin'}
MAX_FILE = 2 * 1024**3
MAX_TOTAL = 8 * 1024**3
PUBLIC_VERSION = re.compile(r'0\.(?:0|[1-9][0-9]*)(?:\.(?:0|[1-9][0-9]*))?')


class CandidateError(ValueError):
    """A fixed, public validation reason, never raw transport or payload text."""


class GitHub:
    """gh owns API authentication and archive redirects; never log stderr/URLs."""
    def __init__(self, env=None):
        env = os.environ if env is None else env
        token = env.get('GH_TOKEN') or env.get('GITHUB_TOKEN')
        require(bool(token), 'missing GitHub token')
        self.env = dict(os.environ, GH_TOKEN=token, GH_HOST='github.com', GH_PROMPT_DISABLED='1')
        self.env.pop('GH_DEBUG', None)
        self.env.pop('GITHUB_TOKEN', None)

    def request(self, path, *, method='GET', payload=None, source=None, output=None, limit=16*1024**2):
        require(path == 'user' or path.startswith(BASE + '/'), 'untrusted API path')
        require(not any(c in path for c in ('\n', '\r', '#', '..', '\\')), 'invalid API path')
        argv = ['gh', 'api', path, '--hostname', 'github.com', '--method', method,
                '-H', 'X-GitHub-Api-Version: 2022-11-28']
        data = None
        if payload is not None:
            argv += ['--input', '-', '-H', 'Content-Type: application/json']
            data = json.dumps(payload).encode()
        elif source is not None:
            argv[2] = 'https://uploads.github.com/' + path
            argv += ['--input', str(source), '-H', 'Content-Type: application/octet-stream']
        else:
            # Actions ZIP downloads are JSON-API redirects. Octet-stream is
            # required only for the distinct release-asset binary endpoint.
            binary_asset = output and path.startswith(BASE + '/releases/assets/')
            argv += ['-H', 'Accept: application/octet-stream' if binary_asset else 'Accept: application/vnd.github+json']
        with tempfile.TemporaryFile() as stream:
            try:
                process = subprocess.run(argv, input=data, stdout=stream, stderr=subprocess.PIPE,
                                         env=self.env, timeout=300, check=False)
            except (subprocess.SubprocessError, OSError):
                raise ValueError('GitHub transport failed (details redacted)') from None
            if process.returncode:
                # gh reports the HTTP status itself; only an explicit GET 404
                # is absence, never an auth/network/parse error.
                if method == 'GET' and not output and re.search(rb'\(HTTP 404\)(?:\s|$)', process.stderr or b''):
                    return None
                raise ValueError('GitHub request failed (details redacted)')
            size = stream.tell()
            require(0 < size <= limit, 'GitHub response size limit')
            stream.seek(0)
            if output:
                with Path(output).open('xb') as destination:
                    shutil.copyfileobj(stream, destination, 1024*1024)
                return None
            return load_json(stream.read())

    def get(self, path):
        return self.request(path)

    def download(self, path, output, limit):
        self.request(path, output=output, limit=limit)

    def mutate(self, method, path, payload=None, source=None):
        require(method in ('POST', 'PATCH'), 'unsupported mutation')
        require(self.get('user').get('login') == 'kissetfall', 'mutation actor must be kissetfall')
        return self.request(path, method=method, payload=payload, source=source)

    def anonymous(self, url, output, limit):
        parsed = urlparse(url)
        require(parsed.scheme == 'https' and parsed.netloc == 'github.com' and
                parsed.path.startswith('/' + REPOSITORY + '/releases/download/') and
                not parsed.query and not parsed.fragment, 'untrusted public download URL')
        class SecureRedirect(HTTPRedirectHandler):
            def redirect_request(self, req, fp, code, msg, headers, newurl):
                require(urlparse(newurl).scheme == 'https', 'insecure anonymous redirect')
                return super().redirect_request(req, fp, code, msg, headers, newurl)
        try:
            started, count = time.monotonic(), 0
            request = Request(url, headers={'User-Agent': 'entropy-release-candidate'})
            with build_opener(SecureRedirect()).open(request, timeout=20) as response, Path(output).open('xb') as stream:
                while chunk := response.read(1024*1024):
                    count += len(chunk)
                    require(count <= limit and time.monotonic() - started < 300, 'public download size/time limit')
                    stream.write(chunk)
        except Exception:
            raise ValueError('anonymous release verification failed (details redacted)') from None

    def pages(self, path, key=None):
        rows, ids, total = [], set(), None
        separator = '&' if '?' in path else '?'
        for page in range(1, 102):
            payload = self.get(path + separator + f'per_page=100&page={page}')
            require(payload is not None, 'missing paginated inventory')
            batch = payload.get(key) if key else payload
            require(isinstance(batch, list) and len(batch) <= 100, 'invalid page')
            if key:
                count = payload.get('total_count')
                require(type(count) is int and 0 <= count <= 10000 and (total is None or count == total),
                        'unstable/oversized inventory')
                total = count
            for row in batch:
                require(isinstance(row, dict) and positive(row.get('id')) and row['id'] not in ids, 'duplicate/invalid inventory ID')
                ids.add(row['id']); rows.append(row)
            require(len(rows) <= 10000, 'oversized inventory')
            if total is not None and len(rows) == total:
                return rows
            if len(batch) < 100:
                require(total is None, 'incomplete paginated inventory')
                return rows
            require(total is None or len(rows) < total, 'inventory total mismatch')
        raise ValueError('pagination limit exceeded')


def require(condition, message):
    if not condition:
        raise CandidateError(message)


def positive(value):
    return type(value) is int and value > 0


def unique(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, 'duplicate JSON key')
        result[key] = value
    return result


def load_json(data):
    return json.loads(data, object_pairs_hook=unique)


def git(*args):
    try:
        return subprocess.check_output(['git', *args], cwd=ROOT, stderr=subprocess.DEVNULL,
                                       text=True, timeout=30).strip()
    except (subprocess.SubprocessError, OSError):
        raise ValueError('git identity lookup failed') from None


def context(env=None):
    env = os.environ if env is None else env
    require(env.get('GITHUB_REPOSITORY') == REPOSITORY, 'wrong repository')
    sha, tree = git('rev-parse', 'HEAD'), git('rev-parse', 'HEAD^{tree}')
    require(re.fullmatch('[0-9a-f]{40}', sha) and re.fullmatch('[0-9a-f]{40}', tree), 'invalid git identity')
    require(env.get('GITHUB_SHA') == sha, 'checkout SHA mismatch')
    require(not git('status', '--porcelain', '--untracked-files=no'), 'dirty tracked worktree')
    require(env.get('GITHUB_WORKFLOW_REF') == REPOSITORY + '/' + WORKFLOW + '@' + env.get('GITHUB_REF', ''),
            'wrong workflow/ref identity')
    try:
        version = subprocess.check_output(['sh', str(ROOT / 'scripts/public_version.sh'), str(ROOT / 'Cargo.toml')],
                                          text=True, stderr=subprocess.DEVNULL, timeout=30).strip()
    except (subprocess.SubprocessError, OSError):
        raise ValueError('public version lookup failed') from None
    require(PUBLIC_VERSION.fullmatch(version), 'unsupported public version')
    return dict(repository=REPOSITORY, workflow=WORKFLOW, source_sha=sha, tree=tree, version=version, tag='v' + version)


def run_identity(env):
    try:
        run_id, attempt = int(env['GITHUB_RUN_ID']), int(env['GITHUB_RUN_ATTEMPT'])
    except (ValueError, KeyError):
        raise ValueError('missing run identity') from None
    require(positive(run_id) and positive(attempt), 'invalid run identity')
    return dict(run_id=run_id, run_attempt=attempt)


def checked_path(value):
    path = Path(value).absolute()
    require(not any(p.is_symlink() for p in (path, *path.parents)), 'symlink path')
    return path.resolve()


def files(root):
    root = checked_path(root)
    require(root.is_dir(), 'missing input directory')
    result = {}
    total = 0
    for path in root.rglob('*'):
        require(not path.is_symlink(), 'symlink input')
        if path.is_dir():
            continue
        require(path.is_file(), 'non-regular input')
        size = path.stat().st_size
        require(0 < size <= MAX_FILE, 'empty or oversized input')
        total += size
        require(total <= MAX_TOTAL and len(result) < 100, 'input limits exceeded')
        result[path.relative_to(root).as_posix()] = path
    return result


def fresh(output, *inputs):
    output = checked_path(output)
    require(not output.exists(), 'output already exists')
    for value in inputs:
        value = checked_path(value)
        require(output != value and output not in value.parents and value not in output.parents,
                'overlapping input/output')
    output.parent.mkdir(parents=True, exist_ok=True)
    return output


def sha256(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def small(path):
    require(path.stat().st_size <= 65536, 'oversized proof or manifest')
    return path.read_bytes()


def json_write(path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open('x', encoding='utf-8') as stream:
        json.dump(data, stream, sort_keys=True, indent=2)
        stream.write('\n')


def check_proof(path, profile, identity):
    expected = dict(schema=1, repository=REPOSITORY, workflow=WORKFLOW, profile=profile,
                    run_id=identity['run_id'], run_attempt=identity['run_attempt'],
                    checkout_sha=identity['source_sha'], tree=identity['tree'], scope='full')
    proof = load_json(small(path))
    require(proof == expected and all(type(proof[k]) is type(v) for k, v in expected.items()),
            'full-validation proof mismatch')


def certificate_pin():
    pins = re.findall(r'^MACOS_RELEASE_CERTIFICATE_SHA1=([0-9A-F]{40})$',
                      (ROOT / 'scripts/macos_stable_signing.sh').read_text(), re.M)
    require(len(pins) == 1, 'missing/ambiguous certificate pin')
    return pins[0]


def check_signing(signing, assets, identity):
    # Same evidence format and algorithm as the independently delivered native
    # qualification tooling. Bind both text and actual release DMG bytes.
    pin = certificate_pin()
    for arch, target in ARCHES.items():
        folder = signing[arch]
        raw = small(folder['signing-evidence.txt'])
        lines = [line.split('=', 1) for line in raw.decode().splitlines()]
        require(all(len(p) == 2 for p in lines), 'malformed signing fields')
        details = unique(lines)
        expected = dict(source_sha=identity['source_sha'], target=target, certificate_sha1=pin,
            bundle_id='com.ergohaven.entropy',
            designated_requirement='=designated => anchor = H"' + pin.lower() + '" and identifier "com.ergohaven.entropy"',
            binary_and_bundle_signature='verified', dmg_integrity='verified', readonly_mounted_dmg='verified',
            applications_symlink='verified', temporary_keychain_deleted='yes', search_list_restored='yes')
        require(details == expected, 'native signing evidence mismatch')
        pairs = [line.split('  ', 1) for line in small(folder['SHA256SUMS']).decode().splitlines()]
        require(all(len(p) == 2 and re.fullmatch('[0-9a-f]{64}', p[0]) for p in pairs), 'malformed signing checksum')
        sums = unique([(name, digest) for digest, name in pairs])
        require(set(sums) == {'signing-evidence.txt', f'entropy-signing-test-{arch}.dmg'}, 'signing checksum inventory')
        require(sums['signing-evidence.txt'] == hashlib.sha256(raw).hexdigest(), 'signing text checksum mismatch')
        require(sums[f'entropy-signing-test-{arch}.dmg'] == sha256(assets / f'entropy-{identity["tag"]}-macos-{arch}.dmg'),
                'native DMG checksum mismatch')


def check_assets(assets, tag):
    inventory = files(assets)
    require(len(inventory) == 8 and 'SHA256SUMS.txt' in inventory and all('/' not in n for n in inventory),
            'public asset inventory mismatch')
    # Reuse the inventory owner, not a second suffix list. This scratch collection
    # is validation only: promotion always copies the original frozen checksum.
    with tempfile.TemporaryDirectory(prefix='entropy-inventory-') as tmp:
        source, output = Path(tmp) / 'source', Path(tmp) / 'verified'
        source.mkdir()
        for name, path in inventory.items():
            if name != 'SHA256SUMS.txt':
                try:
                    os.link(path, source / name)
                except OSError:
                    shutil.copyfile(path, source / name)
        names = collect(source, output, tag)
        require((output / 'SHA256SUMS.txt').read_bytes() == small(inventory['SHA256SUMS.txt']), 'public checksum mismatch')
    return [dict(name=name, size=inventory[name].stat().st_size, sha256=sha256(inventory[name])) for name in names]


def validate_candidate(candidate, env=None, expected_run=None):
    candidate = Path(candidate)
    inventory = files(candidate)
    require('candidate.json' in inventory, 'missing candidate manifest')
    identity = context(env)
    manifest = load_json(small(inventory['candidate.json']))
    expected_keys = set(identity) | {'schema', 'run_id', 'run_attempt', 'assets'}
    require(isinstance(manifest, dict) and set(manifest) == expected_keys, 'manifest schema mismatch')
    require(type(manifest['schema']) is int and manifest['schema'] == 1, 'manifest schema mismatch')
    require(all(manifest.get(k) == v for k, v in identity.items()), 'candidate source/tree/version mismatch')
    require(positive(manifest['run_id']) and positive(manifest['run_attempt']), 'invalid candidate run')
    if expected_run:
        require(all(manifest.get(k) == v for k,v in expected_run.items()), 'candidate run mismatch')
    assets = check_assets(candidate / 'assets', identity['tag'])
    require(manifest['assets'] == assets and all(type(row['size']) is int for row in manifest['assets']), 'manifest asset hash mismatch')
    expected_files = {'candidate.json'} | {'assets/' + a['name'] for a in assets}
    for profile in ('build', 'nix'):
        name = f'proofs/{profile}.json'
        expected_files.add(name)
        require(name in inventory, 'missing full-validation proof')
        check_proof(inventory[name], profile, manifest)
    signing = {}
    for arch in ARCHES:
        signing[arch] = {}
        for name in ('signing-evidence.txt', 'SHA256SUMS'):
            key = f'signing/{arch}/{name}'
            expected_files.add(key)
            require(key in inventory, 'missing signing proof')
            signing[arch][name] = inventory[key]
    require(set(inventory) == expected_files and len(inventory) == 15, 'candidate inventory mismatch')
    check_signing(signing, candidate / 'assets', identity)
    return manifest


def pack(assets, proofs, signing, output, tag, env=None):
    env = os.environ if env is None else env
    manual_context(env)
    identity = dict(context(env), **run_identity(env))
    require(tag == identity['tag'], 'tag/version mismatch')
    output = fresh(output, assets, proofs, signing)
    proof_files, signing_files = files(proofs), files(signing)
    # Artifact download may be nested, but exactly one original artifact path
    # per profile is accepted, with no unrelated inputs.
    located = {}
    for profile in ('build', 'nix'):
        suffix = f'ci-proof-{profile}-{identity["run_id"]}-{identity["run_attempt"]}/{profile}.json'
        matches = [p for n,p in proof_files.items() if n == suffix or n.endswith('/' + suffix)]
        require(len(matches) == 1, 'missing/duplicate full-validation proof')
        located[profile] = matches[0]
        check_proof(matches[0], profile, identity)
    require(set(located.values()) == set(proof_files.values()), 'unexpected proof input')
    native = {}
    for arch in ARCHES:
        native[arch] = {}
        for name in ('signing-evidence.txt', 'SHA256SUMS'):
            suffix = f'release-signing-evidence-{arch}/{name}'
            matches = [p for n,p in signing_files.items() if n == suffix or n.endswith('/' + suffix)]
            require(len(matches) == 1, 'missing/duplicate signing proof')
            native[arch][name] = matches[0]
    require({p for group in native.values() for p in group.values()} == set(signing_files.values()), 'unexpected signing input')
    checked_path(assets)
    files(assets)
    with tempfile.TemporaryDirectory(prefix='.candidate-pack-', dir=output.parent) as tmp:
        work = Path(tmp) / 'candidate'
        work.mkdir()
        names = collect(Path(assets), work / 'assets', tag)
        check_signing(native, work / 'assets', identity)
        for profile, path in located.items():
            target = work / 'proofs' / (profile + '.json')
            target.parent.mkdir(exist_ok=True)
            shutil.copyfile(path, target)
        for arch, group in native.items():
            folder = work / 'signing' / arch
            folder.mkdir(parents=True)
            for name, path in group.items():
                shutil.copyfile(path, folder / name)
        manifest = dict(schema=1, **identity, assets=[dict(name=n, size=(work/'assets'/n).stat().st_size,
                         sha256=sha256(work/'assets'/n)) for n in names])
        json_write(work / 'candidate.json', manifest)
        validate_candidate(work, env)
        work.rename(output)
    return manifest


def changelog(tag):
    text = (ROOT / 'CHANGELOG.md').read_text(encoding='utf-8')
    headings = list(re.finditer(r'^## ([^\n]+)\n', text, re.M))
    matches = [(i, m) for i,m in enumerate(headings) if m.group(1) == tag]
    require(len(matches) == 1, 'missing/duplicate changelog heading')
    index, match = matches[0]
    end = headings[index + 1].start() if index + 1 < len(headings) else len(text)
    body = text[match.end():end].strip()
    require(bool(body), 'empty changelog section')
    return body


def manual_context(env):
    require(env.get('GITHUB_EVENT_NAME') == 'workflow_dispatch' and env.get('GITHUB_REF') == 'refs/heads/main',
            'candidate preparation requires manual main dispatch')
    require(env.get('GITHUB_ACTOR') == env.get('GITHUB_TRIGGERING_ACTOR') == 'kissetfall', 'wrong dispatch actor')
    event = load_json(small(checked_path(env['GITHUB_EVENT_PATH'])))
    inputs = event.get('inputs', {})
    require(type(inputs.get('prepare_release')) in (str, bool) and type(inputs.get('macos_signing_test')) in (str, bool) and
            inputs.get('prepare_release') in ('true', True) and inputs.get('macos_signing_test') in ('false', False),
            'explicit original prepare-release input required; signing-test mode forbidden')


def tag_context(env, identity):
    require(env.get('GITHUB_EVENT_NAME') == 'push' and env.get('GITHUB_REF') == 'refs/tags/' + identity['tag'],
            'promotion requires the exact public tag push')
    require(git('rev-parse', 'refs/tags/' + identity['tag'] + '^{commit}') == identity['source_sha'],
            'peeled tag commit mismatch')


def check_run(run, identity, run_id):
    require(isinstance(run, dict), 'missing source run')
    expected = dict(id=run_id, path=WORKFLOW, event='workflow_dispatch', head_branch='main',
                    head_sha=identity['source_sha'], status='completed', conclusion='success')
    require(positive(run_id) and all(run.get(k) == v for k,v in expected.items()), 'source run identity mismatch')
    require(positive(run.get('run_attempt')) and run.get('repository', {}).get('full_name') == REPOSITORY and
            positive(run['repository'].get('id')), 'source repository/attempt mismatch')
    require(run.get('actor', {}).get('login') == run.get('triggering_actor', {}).get('login') == 'kissetfall',
            'source run actor mismatch')
    return dict(run_id=run_id, run_attempt=run['run_attempt'])


def timestamp(value):
    try:
        parsed = datetime.fromisoformat(value.replace('Z', '+00:00'))
        require(parsed.tzinfo is not None, 'timestamp must have timezone')
        return parsed
    except (ValueError, AttributeError, TypeError):
        raise ValueError('invalid artifact/run timestamp') from None


def check_artifact(artifact, run, allow_expired=False):
    require(isinstance(artifact, dict) and artifact.get('name') == 'release-candidate', 'wrong candidate artifact')
    require(positive(artifact.get('id')) and positive(artifact.get('size_in_bytes')) and
            artifact['size_in_bytes'] <= MAX_TOTAL, 'invalid candidate artifact identity/size')
    require(type(artifact.get('expired')) is bool, 'invalid artifact expiry marker')
    expires_at = timestamp(artifact.get('expires_at'))
    available = not artifact['expired'] and expires_at > datetime.now(timezone.utc)
    require(isinstance(artifact.get('digest'), str) and re.fullmatch('sha256:[0-9a-f]{64}', artifact['digest']),
            'missing/invalid candidate API digest')
    origin = artifact.get('workflow_run', {})
    require(origin.get('id') == run['id'] and origin.get('repository_id') == run['repository']['id'] and
            origin.get('head_repository_id') == run['repository']['id'] and origin.get('head_sha') == run['head_sha'] and
            origin.get('head_branch') == 'main', 'candidate artifact source/repository/run mismatch')
    require(available or allow_expired, 'expired candidate artifact')
    return available


def source_candidate(api, identity, run_id, allow_missing=False, allow_expired=False):
    run = api.get(f'{BASE}/actions/runs/{run_id}')
    expected = check_run(run, identity, run_id)
    attempt = api.get(f'{BASE}/actions/runs/{run_id}/attempts/{expected["run_attempt"]}')
    check_run(attempt, identity, run_id)
    require(attempt['run_attempt'] == run['run_attempt'], 'source attempt mismatch')
    artifacts = api.pages(f'{BASE}/actions/runs/{run_id}/artifacts', 'artifacts')
    matches = [a for a in artifacts if a.get('name') == 'release-candidate']
    if not matches and allow_missing:
        if allow_expired:
            return None  # Only explicit preparation may replace unavailable bytes.
        jobs = api.pages(f'{BASE}/actions/runs/{run_id}/attempts/{run["run_attempt"]}/jobs', 'jobs')
        require(all(job.get('run_id') == run_id and job.get('status') == 'completed' and
                    (job.get('conclusion') == 'skipped' or
                     job.get('name') == 'Resolve frozen release candidate' and job.get('conclusion') == 'success')
                    for job in jobs), 'missing candidate after executed qualification; refusing older fallback')
        return None
    require(len(matches) == 1, 'missing/ambiguous release-candidate; never rebuild on promotion')
    if not check_artifact(matches[0], run, allow_expired=allow_expired):
        return None
    return run, matches[0]


def reusable_candidate(api, identity, allow_expired=False):
    runs = api.pages(BASE + '/actions/workflows/release.yml/runs?event=workflow_dispatch&branch=main&head_sha=' + identity['source_sha'], 'workflow_runs')
    eligible = [run for run in runs if run.get('status') == 'completed' and run.get('conclusion') == 'success']
    for run in eligible:
        check_run(run, identity, run.get('id'))
    for listed in sorted(eligible, key=lambda run: (timestamp(run.get('created_at')), run['id']), reverse=True):
        found = source_candidate(api, identity, listed['id'], allow_missing=True, allow_expired=allow_expired)
        if found is None:
            continue  # A previous successful reuse dispatch has no new artifact.
        run, artifact = found
        require(run['run_attempt'] == listed['run_attempt'], 'listed source attempt mismatch')
        return dict(tag=identity['tag'], prepare=False, candidate_run_id=run['id'], candidate_artifact_id=artifact['id'])
    return None


def plan(env=None, api=None):
    env = os.environ if env is None else env
    identity = context(env)
    if env.get('GITHUB_EVENT_NAME') == 'workflow_dispatch':
        manual_context(env)
        changelog(identity['tag'])
        require(api.get(BASE + '/git/ref/tags/' + identity['tag']) is None, 'proposed tag already exists')
        require(api.get(BASE + '/releases/tags/' + identity['tag']) is None, 'proposed release already exists')
        require(not any(r.get('tag_name') == identity['tag'] for r in api.pages(BASE + '/releases')), 'proposed draft already exists')
        return reusable_candidate(api, identity, allow_expired=True) or dict(tag=identity['tag'], prepare=True, candidate_run_id='', candidate_artifact_id='')
    tag_context(env, identity)
    result = reusable_candidate(api, identity)
    if result is None:
        raise ValueError('no successful frozen source candidate; never rebuild on promotion')
    return result


def unpack(archive, output):
    """Preflight every ZIP entry before writing; stream CRC-checked regular files."""
    require(archive.stat().st_size <= MAX_TOTAL, 'oversized ZIP')
    with zipfile.ZipFile(archive) as z:
        entries = z.infolist()
        require(len(entries) == 15, 'candidate ZIP must contain exactly 15 regular files')
        seen, total = set(), 0
        for entry in entries:
            name = entry.filename
            path = PurePosixPath(name)
            mode = entry.external_attr >> 16
            require(name == entry.orig_filename and '\\' not in name and '\x00' not in name and
                    not path.is_absolute() and name == path.as_posix() and
                    all(p not in ('', '.', '..') and ':' not in p for p in path.parts), 'unsafe ZIP path')
            require(not entry.is_dir() and stat.S_IFMT(mode) in (0, stat.S_IFREG) and
                    not entry.flag_bits & 1 and entry.compress_type in (zipfile.ZIP_STORED, zipfile.ZIP_DEFLATED),
                    'non-regular/encrypted ZIP entry')
            require(name not in seen and 0 < entry.file_size <= MAX_FILE, 'duplicate/empty/oversized ZIP entry')
            seen.add(name)
            total += entry.file_size
            require(total <= MAX_TOTAL, 'oversized uncompressed ZIP')
        require(not any(str(parent) in seen for name in seen for parent in PurePosixPath(name).parents if str(parent) != '.'),
                'ZIP file/directory collision')
        output.mkdir()
        for entry in entries:
            target = output / entry.filename
            target.parent.mkdir(parents=True, exist_ok=True)
            copied = 0
            with z.open(entry) as source, target.open('xb') as destination:
                while chunk := source.read(1024 * 1024):
                    copied += len(chunk)
                    require(copied <= entry.file_size, 'ZIP size mismatch')
                    destination.write(chunk)
            require(copied == entry.file_size, 'truncated ZIP entry')


def fetch(run_id, artifact_id, output, env=None, api=None):
    env = os.environ if env is None else env
    identity = context(env)
    tag_context(env, identity)
    require(positive(run_id) and positive(artifact_id), 'invalid candidate IDs')
    output = fresh(output)
    found = source_candidate(api, identity, run_id)
    if found is None:
        raise ValueError('missing release-candidate')
    run, artifact = found
    require(artifact['id'] == artifact_id, 'requested artifact does not match source run')
    actual = api.get(f'{BASE}/actions/artifacts/{artifact_id}')
    check_artifact(actual, run)
    require(actual == artifact, 'artifact inventory/detail mismatch')
    with tempfile.TemporaryDirectory(prefix='.candidate-fetch-', dir=output.parent) as tmp:
        archive, work = Path(tmp) / 'candidate.zip', Path(tmp) / 'candidate'
        api.download(f'{BASE}/actions/artifacts/{artifact_id}/zip', archive, MAX_TOTAL)
        require(archive.stat().st_size == artifact['size_in_bytes'] and
                'sha256:' + sha256(archive) == artifact['digest'], 'candidate API ZIP digest/size mismatch')
        unpack(archive, work)
        manifest = validate_candidate(work, env, dict(run_id=run_id, run_attempt=run['run_attempt']))
        work.rename(output)
    return manifest


def remote_tag(api, identity):
    ref = api.get(BASE + '/git/ref/tags/' + identity['tag'])
    require(isinstance(ref, dict) and ref.get('ref') == 'refs/tags/' + identity['tag'], 'missing/foreign remote tag')
    obj = ref.get('object', {})
    seen = set()
    for _ in range(8):
        sha = obj.get('sha')
        require(isinstance(sha, str) and re.fullmatch('[0-9a-f]{40}', sha) and sha not in seen, 'invalid/cyclic remote tag')
        seen.add(sha)
        if obj.get('type') == 'commit':
            require(sha == identity['source_sha'], 'remote peeled tag mismatch')
            return
        require(obj.get('type') == 'tag', 'unsupported tag object')
        tag = api.get(BASE + '/git/tags/' + sha)
        require(isinstance(tag, dict) and tag.get('sha') == sha, 'annotated tag object mismatch')
        obj = tag.get('object', {})
    raise ValueError('remote tag nesting limit')


def find_release(api, tag):
    by_tag = api.get(BASE + '/releases/tags/' + tag)
    matches = [r for r in api.pages(BASE + '/releases') if r.get('tag_name') == tag]
    require(len(matches) <= 1, 'ambiguous matching releases')
    if not matches:
        require(by_tag is None, 'release inventory/tag lookup mismatch')
        return None
    release = api.get(f'{BASE}/releases/{matches[0]["id"]}')
    require(release is not None and release.get('id') == matches[0]['id'] and release.get('tag_name') == tag,
            'release inventory/detail mismatch')
    require(by_tag is None and release.get('draft') is True or by_tag == release, 'release lookup mismatch')
    return release


def check_release(release, identity, body):
    require(isinstance(release, dict) and positive(release.get('id')), 'missing release')
    expected = dict(tag_name=identity['tag'], target_commitish=identity['source_sha'],
                    name='Entropy ' + identity['tag'], body=body, prerelease=False)
    require(all(release.get(k) == v and type(release.get(k)) is type(v) for k,v in expected.items()) and
            type(release.get('draft')) is bool and release.get('author', {}).get('login') == 'kissetfall',
            'unknown or conflicting release; refusing to adopt')


def asset_inventory(api, release_id, manifest, complete=False):
    rows = api.pages(f'{BASE}/releases/{release_id}/assets')
    expected = {a['name']: a for a in manifest['assets']}
    found, ids = {}, set()
    for row in rows:
        name = row.get('name')
        require(name in expected and name not in found and positive(row.get('id')) and row['id'] not in ids,
                'unexpected/duplicate stored release asset')
        require(row.get('size') == expected[name]['size'] and row.get('state') == 'uploaded' and
                row.get('uploader', {}).get('login') == 'kissetfall', 'stored asset metadata conflict')
        found[name] = row; ids.add(row['id'])
    require(not complete or set(found) == set(expected), 'incomplete published asset inventory')
    return found


def verify_remote_assets(api, inventory, manifest, folder, anonymous=False):
    folder.mkdir()
    expected = {a['name']: a for a in manifest['assets']}
    def verify(item):
        name, row = item
        path = folder / name
        if anonymous:
            url = 'https://github.com/' + REPOSITORY + '/releases/download/' + quote(manifest['tag'], safe='') + '/' + quote(name, safe='')
            require(row.get('browser_download_url') == url, 'unexpected public asset URL')
            api.anonymous(url, path, expected[name]['size'])
        else:
            api.download(f'{BASE}/releases/assets/{row["id"]}', path, expected[name]['size'])
        require(path.stat().st_size == expected[name]['size'] and sha256(path) == expected[name]['sha256'],
                'stored/public release bytes mismatch')
        return name
    with ThreadPoolExecutor(max_workers=3) as pool:
        return sorted(pool.map(verify, inventory.items()))


def mutate(api, method, path, payload=None, source=None):
    require(api.get('user').get('login') == 'kissetfall', 'mutation actor must be kissetfall')
    return api.mutate(method, path, payload=payload, source=source)


def require_latest_forward(api, identity):
    latest = api.get(BASE + '/releases/latest')
    if latest is None:
        return
    require(isinstance(latest, dict) and positive(latest.get('id')) and latest.get('draft') is False and
            latest.get('prerelease') is False, 'invalid Latest release metadata')
    tag = latest.get('tag_name')
    require(isinstance(tag, str) and tag.startswith('v') and PUBLIC_VERSION.fullmatch(tag[1:]),
            'unknown Latest release version; maintainer decision required')
    def number(value):
        return tuple(int(part) for part in (value[1:].split('.') + ['0'])[:3])
    require(number(tag) < number(identity['tag']), 'newer or equivalent Latest already exists; refusing rollback')


def publish(candidate, env=None, api=None, output=None):
    env = os.environ if env is None else env
    manifest = validate_candidate(candidate, env)
    identity = context(env)
    tag_context(env, identity)
    remote_tag(api, identity)
    body = changelog(identity['tag'])
    if output is None:
        base = checked_path(env.get('RUNNER_TEMP') or tempfile.gettempdir())
        output = Path(tempfile.mkdtemp(prefix='entropy-publication-', dir=base))
    else:
        output = fresh(output, candidate)
        output.mkdir()
    sequence = 0
    def receipt(status, **fields):
        nonlocal sequence
        sequence += 1
        result = dict(schema=1, status=status, repository=REPOSITORY, tag=manifest['tag'],
                      source_sha=manifest['source_sha'], candidate_run_id=manifest['run_id'],
                      assets=manifest['assets'], **fields)
        json_write(output / f'{sequence:03d}-{status}.json', result)
        return result
    try:
        receipt('preflight')
        release = find_release(api, identity['tag'])
        if release is None or release.get('draft') is True:
            require_latest_forward(api, identity)
        if release is None:
            payload = dict(tag_name=identity['tag'], target_commitish=identity['source_sha'], name='Entropy ' + identity['tag'],
                           body=body, draft=True, prerelease=False, make_latest='false', generate_release_notes=False)
            remote_tag(api, identity)
            created = mutate(api, 'POST', BASE + '/releases', payload)
            require(isinstance(created, dict) and positive(created.get('id')), 'invalid created release identity')
            release = api.get(f'{BASE}/releases/{created["id"]}')
            require(release is not None and release['id'] == created['id'] and release.get('draft') is True,
                    'created draft readback mismatch')
            receipt('draft-created', release_id=created['id'])
        check_release(release, identity, body)
        release_id, was_draft = release['id'], release['draft']
        inventory = asset_inventory(api, release_id, manifest, complete=not was_draft)
        with tempfile.TemporaryDirectory(prefix='.release-verification-', dir=output) as tmp:
            work = Path(tmp)
            # Fail before adding anything when a partial draft contains a conflict.
            if inventory:
                verify_remote_assets(api, inventory, manifest, work / 'existing')
            if was_draft:
                for asset in manifest['assets']:
                    name = asset['name']
                    if name in inventory:
                        continue
                    path = Path(candidate) / 'assets' / name
                    require(path.stat().st_size == asset['size'] and sha256(path) == asset['sha256'], 'candidate changed before upload')
                    remote_tag(api, identity)
                    result = mutate(api, 'POST', f'{BASE}/releases/{release_id}/assets?name=' + quote(name, safe=''), source=path)
                    require(positive(result.get('id')), 'invalid uploaded asset identity')
                    stored = api.get(f'{BASE}/releases/assets/{result["id"]}')
                    require(stored is not None and stored.get('name') == name and stored.get('id') == result['id'], 'upload readback mismatch')
                    receipt('asset-uploaded', release_id=release_id, asset_name=name, asset_id=result['id'])
                inventory = asset_inventory(api, release_id, manifest, complete=True)
                verify_remote_assets(api, inventory, manifest, work / 'stored')
                before = api.get(f'{BASE}/releases/{release_id}')
                check_release(before, identity, body)
                require(before['draft'] is True, 'draft changed concurrently')
                remote_tag(api, identity)
                require_latest_forward(api, identity)
                receipt('stored-bytes-verified', release_id=release_id)
                mutate(api, 'PATCH', f'{BASE}/releases/{release_id}', dict(tag_name=identity['tag'],
                    target_commitish=identity['source_sha'], name='Entropy ' + identity['tag'],
                    body=body, draft=False, prerelease=False, make_latest='true'))
            release = api.get(f'{BASE}/releases/{release_id}')
            check_release(release, identity, body)
            require(release['draft'] is False, 'release did not become public')
            by_tag = api.get(BASE + '/releases/tags/' + identity['tag'])
            require(by_tag == release, 'release tag readback mismatch')
            latest = api.get(BASE + '/releases/latest')
            require(isinstance(latest, dict) and positive(latest.get('id')) and latest.get('draft') is False and
                    latest.get('prerelease') is False, 'latest release readback missing')
            require(not was_draft or latest['id'] == release_id, 'published release is not Latest')
            remote_tag(api, identity)
            inventory = asset_inventory(api, release_id, manifest, complete=True)
            verified = verify_remote_assets(api, inventory, manifest, work / 'public', anonymous=True)
        return receipt('published-verified' if was_draft else 'already-published-verified',
                       release_id=release_id, latest_release_id=latest['id'], public_verified=verified)
    except Exception:
        receipt('failed-no-rollback')
        raise


def stage(candidate, output, env=None):
    manifest = validate_candidate(candidate, env)
    output = fresh(output, candidate)
    with tempfile.TemporaryDirectory(prefix='.candidate-stage-', dir=output.parent) as tmp:
        work = Path(tmp) / 'assets'
        work.mkdir()
        for asset in manifest['assets']:
            path = work / asset['name']
            shutil.copyfile(Path(candidate) / 'assets' / asset['name'], path)
            require(sha256(path) == asset['sha256'], 'staged copy hash mismatch')
        work.rename(output)
    return manifest


def main(argv=None, env=None):
    env = os.environ if env is None else env
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    commands.add_parser('plan')
    packaging = commands.add_parser('pack')
    for name in ('assets', 'proofs', 'signing', 'output'):
        packaging.add_argument('--' + name, type=Path, required=True)
    packaging.add_argument('--tag', required=True)
    fetching = commands.add_parser('fetch')
    fetching.add_argument('--run-id', type=int, required=True)
    fetching.add_argument('--artifact-id', type=int, required=True)
    fetching.add_argument('--output', type=Path, required=True)
    staging = commands.add_parser('stage')
    staging.add_argument('--candidate', type=Path, required=True)
    staging.add_argument('--output', type=Path, required=True)
    publishing = commands.add_parser('publish')
    publishing.add_argument('--candidate', type=Path, required=True)
    publishing.add_argument('--output', type=Path, help='fresh local publication receipts directory (otherwise RUNNER_TEMP)')
    args = parser.parse_args(argv)
    try:
        if args.command == 'plan':
            result = plan(env, GitHub(env))
            if result['candidate_run_id'] and env.get('GITHUB_STEP_SUMMARY'):
                url = f'https://github.com/{REPOSITORY}/actions/runs/{result["candidate_run_id"]}/artifacts/{result["candidate_artifact_id"]}'
                with Path(env['GITHUB_STEP_SUMMARY']).open('a', encoding='utf-8') as stream:
                    stream.write(f'Using frozen {result["tag"]} candidate: {url}\n\nNo repeated qualification or production builds.\n')
            lines = ''.join(f'{key}={str(value).lower() if type(value) is bool else value}\n' for key,value in result.items())
            if env.get('GITHUB_OUTPUT'):
                with Path(env['GITHUB_OUTPUT']).open('a', encoding='utf-8') as stream:
                    stream.write(lines)
            else:
                print(lines, end='')
        elif args.command == 'pack':
            result = pack(args.assets, args.proofs, args.signing, args.output, args.tag, env)
            print('frozen candidate verified: ' + result['tag'])
        elif args.command == 'fetch':
            result = fetch(args.run_id, args.artifact_id, args.output, env, GitHub(env))
            print('original candidate downloaded and verified: ' + result['tag'])
        elif args.command == 'stage':
            result = stage(args.candidate, args.output, env)
            print('frozen public bytes staged: ' + result['tag'])
        else:
            validate_candidate(args.candidate, env)
            result = publish(args.candidate, env, GitHub(env), args.output)
            print(result['status'] + ': ' + result['tag'])
        return 0
    except CandidateError as error:
        print(f'release-candidate: {error} (details redacted)', file=sys.stderr)
        return 1
    except Exception:
        print('release-candidate: validation/transport failed (details redacted)', file=sys.stderr)
        return 1


if __name__ == '__main__':
    raise SystemExit(main())
