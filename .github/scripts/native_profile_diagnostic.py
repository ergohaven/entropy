"""Branch-only native diagnostic. Never publishes or mutates Git refs."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import signal
import subprocess
import time

BASE = '68f06dc66fd5d98a9841dc08e79168421cd1e9bb'
BRANCH = 'ci/entropy-macos-profile-diagnostics'
CASES = [
    'app::application_layout_runtime::tests::remaining_sm62_rebinds_after_first_unplug_without_editor_selection',
    'qmk_hid_host::capability_review_tests::transient_fault_recovers_automatically_on_same_owner',
]
ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / 'dist/native-profile-diagnostic'


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def digest(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def save(name, value):
    OUT.mkdir(parents=True, exist_ok=True)
    (OUT / name).write_text(json.dumps(value, indent=2) + '\n')


def git(*args):
    return subprocess.check_output(['git', '-C', str(ROOT), *args], text=True).strip()


def validate_scope(source, parent, shallow, dirty, repository, branch, event):
    require(re.fullmatch('[0-9a-f]{40}', source) is not None, 'source format')
    require(parent == BASE, 'wrong immediate parent/failing product source')
    require(shallow == 'false', 'complete history required')
    require(not dirty, 'dirty diagnostic source')
    require(repository == 'ergohaven/entropy', 'wrong repository')
    require(branch == 'refs/heads/' + BRANCH, 'wrong diagnostic branch')
    require(event == 'workflow_dispatch', 'manual diagnostic only')


def source_preflight():
    source = git('rev-parse', 'HEAD')
    require(source == os.environ.get('GITHUB_SHA'), 'checkout differs from admitted source')
    validate_scope(source, git('rev-parse', 'HEAD^'), git('rev-parse', '--is-shallow-repository'),
                   git('status', '--porcelain', '--untracked-files=no'),
                   os.environ.get('GITHUB_REPOSITORY'), os.environ.get('GITHUB_REF'),
                   os.environ.get('GITHUB_EVENT_NAME'))
    record = {'source': source, 'tree': git('rev-parse', 'HEAD^{tree}'), 'parent': BASE,
              'repository': 'ergohaven/entropy', 'branch': BRANCH,
              'run_id': os.environ.get('GITHUB_RUN_ID'), 'attempt': os.environ.get('GITHUB_RUN_ATTEMPT'),
              'os': platform.system(), 'architecture': platform.machine(),
              'trace_cases': CASES, 'suite_process_parallelism': 4,
              'source_files': {p: digest(ROOT / p) for p in git('ls-files').splitlines()
                               if (ROOT / p).is_file()}}
    save('source-preflight.json', record)
    return record


def preflight():
    record = source_preflight()
    require(platform.system() == 'Darwin' and platform.machine() == 'arm64', 'native Apple Silicon required')
    require(os.environ.get('GITHUB_RUN_ATTEMPT') == '1', 'unexpected diagnostic retry')
    return record


def logged(command, path, timeout):
    with path.open('w') as stream:
        result = subprocess.run(command, cwd=ROOT, stdout=stream, stderr=subprocess.STDOUT, timeout=timeout)
    require(result.returncode == 0, 'preparation failed: ' + repr(command))


def parse_summary(text):
    text = re.sub(r'\x1b\[[0-?]*[ -/]*[@-~]', '', text)
    lines = [line for line in text.splitlines() if re.search(r'\bSummary\s*\[', line)]
    require(len(lines) == 1, 'missing/duplicate native terminal summary')
    line = lines[0]
    match = re.search(r'(\d+) tests run:\s*(\d+) passed', line)
    require(match is not None, 'unrecognized native summary')
    fail = re.search(r'(\d+) failed', line)
    skip = re.search(r'(\d+) skipped', line)
    result = {'executed': int(match[1]), 'passed': int(match[2]),
              'failed': int(fail[1]) if fail else 0, 'skipped': int(skip[1]) if skip else 0,
              'summary': line}
    require(result['executed'] == result['passed'] + result['failed'], 'native summary counts disagree')
    completions = {}
    for row in text.splitlines():
        item = re.search(r'\b(PASS|FAIL)\s*\[[^]]*\]\s*\(\s*(\d+)/(\d+)\)\s+ergohaven-entropy::bin/entropy\s+(\S+)', row)
        if item:
            name = item[4]
            value = (item[1], int(item[2]))
            require(name not in completions or completions[name] == value, 'conflicting completion')
            completions[name] = value
    require(len(completions) == result['executed'], 'missing/extra qualified completions')
    require(len({v[1] for v in completions.values()}) == result['executed'], 'duplicate test ordinal')
    require(sum(v[0] == 'PASS' for v in completions.values()) == result['passed']
            and sum(v[0] == 'FAIL' for v in completions.values()) == result['failed'], 'completion verdict totals disagree')
    result['trace_case_results'] = {name: completions[name][0] for name in CASES if name in completions}
    require(set(result['trace_case_results']) == set(CASES), 'diagnostic case missing')
    return result


def prepare():
    record = preflight()
    rust = subprocess.check_output(['rustc', '-vV'], text=True)
    runner = subprocess.check_output(['cargo', 'nextest', '--version'], text=True)
    require('release: 1.100.1' in rust and 'host: aarch64-apple-darwin' in rust, 'compiler differs from historical native job')
    require(re.search(r'\b0\.9\.150\b', runner), 'test runner differs from historical job')
    commands = [
        (['cargo', 'nextest', 'list', '--locked', '--all-targets', '--list-type', 'binaries-only', '--message-format', 'json'], 'binaries-metadata.json'),
        (['cargo', 'metadata', '--locked', '--format-version', '1', '--filter-platform', 'aarch64-apple-darwin'], 'cargo-metadata.json'),
    ]
    for command, filename in commands:
        with (OUT / filename).open('w') as stream, (OUT / (filename + '.stderr')).open('w') as error:
            result = subprocess.run(command, cwd=ROOT, stdout=stream, stderr=error, timeout=3600)
        require(result.returncode == 0, 'metadata preparation failed: ' + filename)
        json.loads((OUT / filename).read_text())
    # This is a FULL suite diagnostic, not two tests under a lighter load.
    command = ['cargo', 'nextest', 'list', '--binaries-metadata', str(OUT / 'binaries-metadata.json'),
               '--cargo-metadata', str(OUT / 'cargo-metadata.json'), '--message-format', 'json']
    with (OUT / 'selection.json').open('w') as stream:
        subprocess.run(command, cwd=ROOT, stdout=stream, check=True, timeout=120)
    selected = json.loads((OUT / 'selection.json').read_text())
    cases = {name for suite in selected['rust-suites'].values()
             for name, case in suite['testcases'].items()
             if case['filter-match']['status'] == 'matches' and not case['ignored']}
    require(len(cases) == 2234 and set(CASES) <= cases, 'native workload/case inventory changed')
    binaries = json.loads((OUT / 'binaries-metadata.json').read_text())['rust-binaries']
    require(binaries, 'no compiled native binaries')
    identities = [{'id': key, 'path': value['binary-path'], 'sha256': digest(value['binary-path'])}
                  for key, value in binaries.items()]
    save('prepared.json', {'source': record['source'], 'rustc': rust, 'nextest': runner,
                          'selected_cases': sorted(cases), 'binaries': identities, 'metadata': {
                              name: digest(OUT / name) for name in ['binaries-metadata.json', 'cargo-metadata.json', 'selection.json']}})


def run():
    record = preflight()
    prepared = json.loads((OUT / 'prepared.json').read_text())
    require(prepared['source'] == record['source'], 'stale compiled source')
    for name, wanted in prepared['metadata'].items():
        require(digest(OUT / name) == wanted, 'changed compiled metadata')
    for binary in prepared['binaries']:
        require(digest(binary['path']) == binary['sha256'], 'changed compiled binary')
    trace = OUT / 'trace'
    trace.mkdir(exist_ok=False)
    env = dict(os.environ, ENTROPY_PROFILE_DIAGNOSTICS_DIR=str(trace))
    command = ['cargo', 'nextest', 'run', '--binaries-metadata', str(OUT / 'binaries-metadata.json'),
               '--cargo-metadata', str(OUT / 'cargo-metadata.json'), '--test-threads', '4', '--no-fail-fast']
    start = time.monotonic()
    timed_out = False
    with (OUT / 'native.log').open('w') as stream:
        process = subprocess.Popen(command, cwd=ROOT, env=env, stdout=stream, stderr=subprocess.STDOUT, start_new_session=True)
        try:
            code = process.wait(timeout=1800)
        except subprocess.TimeoutExpired:
            timed_out = True
            os.killpg(process.pid, signal.SIGTERM)
            try:
                code = process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                code = process.wait(timeout=10)
    result = {'source': record['source'], 'tree': record['tree'], 'run_id': record['run_id'],
              'attempt': record['attempt'], 'native_test_exit': code, 'timed_out': timed_out,
              'runtime_seconds': time.monotonic() - start, 'command': command,
              'log_sha256': digest(OUT / 'native.log'), 'native_recovery_claimed': False}
    save('execution.json', result)
    require(not timed_out, 'native diagnostic external deadline expired; evidence is incomplete')
    result['summary'] = parse_summary((OUT / 'native.log').read_text())
    files = sorted(trace.glob('*.jsonl'))
    require(files, 'no native telemetry captured')
    seen = set()
    for file in files:
        rows = [json.loads(line) for line in file.read_text().splitlines() if line.strip()]
        require(rows and all(isinstance(row, dict) and all(key in row for key in
                    ['case', 'event', 'pid', 'seq', 'monotonic_ns', 'data']) for row in rows), 'invalid telemetry')
        require(len({row['pid'] for row in rows}) == 1 and len({row['case'] for row in rows}) == 1,
                'mixed process/case telemetry')
        require({row['seq'] for row in rows} == set(range(len(rows))), 'missing/duplicate telemetry sequence')
        require(sum(row['event'] == 'test_begin' for row in rows) == 1
                and sum(row['event'] == 'test_end' for row in rows) == 1, 'incomplete test trace')
        require(all(isinstance(row['monotonic_ns'], int) and row['monotonic_ns'] >= 0 for row in rows), 'invalid shared clock')
        for row in rows:
            matches = [case for case in CASES if row['case'] == case or case.endswith('::' + row['case'])]
            require(len(matches) == 1, 'unexpected telemetry case')
            seen.add(matches[0])
    require(seen == set(CASES), 'a representative case produced no trace')
    result['trace_files'] = [{'name': f.name, 'sha256': digest(f), 'size': f.stat().st_size} for f in files]
    result['diagnostic_evidence_complete'] = True
    save('execution.json', result)
    print(json.dumps(result, indent=2), flush=True)
    return code


def self_test():
    validate_scope('a' * 40, BASE, 'false', '', 'ergohaven/entropy', 'refs/heads/' + BRANCH, 'workflow_dispatch')
    for field, value in [('source', 'oops'), ('parent', 'b' * 40), ('shallow', 'true'),
                         ('dirty', ' M src/main.rs'), ('repository', 'other/repo'),
                         ('branch', 'refs/heads/main'), ('event', 'push')]:
        args = dict(source='a' * 40, parent=BASE, shallow='false', dirty='', repository='ergohaven/entropy', branch='refs/heads/' + BRANCH, event='workflow_dispatch')
        args[field] = value
        try:
            validate_scope(**args)
        except RuntimeError:
            pass
        else:
            raise AssertionError('scope mutation accepted: ' + field)
    text = '\n'.join(f'PASS [0.001s] ({i}/2) ergohaven-entropy::bin/entropy {name}' for i, name in enumerate(CASES, 1))
    require(parse_summary(text + '\nSummary [1s] 2 tests run: 2 passed')['failed'] == 0, 'all-pass parsing')
    require(parse_summary(text.replace('PASS', 'FAIL', 1) + '\nSummary [1s] 2 tests run: 1 passed, 1 failed, 23 skipped')['failed'] == 1, 'failure parsing')
    for broken in [text, text + '\nSummary [1s] 3 tests run: 3 passed', text + '\nSummary [1s] 2 tests run: 2 passed\nSummary [1s] 2 tests run: 2 passed']:
        try:
            parse_summary(broken)
        except RuntimeError:
            pass
        else:
            raise AssertionError('bad native summary accepted')
    print('scope rejection and native-summary controls passed')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('stage', choices=['source-preflight', 'preflight', 'prepare', 'run', 'self-test'])
    stage = parser.parse_args().stage
    if stage == 'self-test':
        self_test()
    elif stage == 'source-preflight':
        source_preflight()
    elif stage == 'preflight':
        preflight()
    elif stage == 'prepare':
        prepare()
    else:
        raise SystemExit(run())
