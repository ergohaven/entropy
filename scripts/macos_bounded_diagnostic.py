#!/usr/bin/env python3
"""One source-bound diagnostic, never a product acceptance/landing controller."""
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import signal
import subprocess
import sys
import time

BASE = '2f875db3534fed9f461efde3d980e1917c131087'
DIAGNOSTIC_PARENT = 'ead646d8a458eea5da38bdfeab39d85e9f0caf30'
BRANCH = 'refs/heads/ci/macos-diagnostic-2f875db-20261114'
CASES = (
    'app::app_lifecycle::tests::settings_disconnect_allows_deferred_exit_after_completed_combo',
    'app::app_lifecycle::tests::settings_worker_channel_failure_allows_deferred_exit_after_completed_combo',
    'app::vial_hid_task::tests::failed_background_key_write_rolls_back_the_optimistic_value',
)
OWNERS = ('src/app_init.rs', 'src/ui/app_lifecycle.rs', 'src/ui/vial_hid_task.rs',
          'src/ui/settings_write_queue.rs', 'src/ui/device_scan.rs', 'src/hid.rs',
          'src/hid_keymap.rs', 'src/smart_input.rs', 'src/app_discovery.rs')
FILTER = ' | '.join(f'test(={name})' for name in CASES)


def sha256(path):
    digest = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(block)
    return digest.hexdigest()


def bounded(command, log, seconds, env=None):
    started = time.monotonic()
    with Path(log).open('w') as output:
        proc = subprocess.Popen(command, stdout=output, stderr=subprocess.STDOUT,
                                env=env, start_new_session=True)
        try:
            code = proc.wait(timeout=seconds)
            expired = False
        except subprocess.TimeoutExpired:
            os.killpg(proc.pid, signal.SIGKILL)
            code = proc.wait()
            expired = True
    return dict(command=command, exit_code=code, timed_out=expired,
                elapsed_seconds=time.monotonic() - started, log=str(log))


def normalize(text):
    return re.sub(r'\x1b\[[0-?]*[ -/]*[@-~]', '', text)


def selected_names(text):
    data = json.loads(text)
    names = []
    for suite in data['rust-suites'].values():
        for name, case in suite.get('testcases', {}).items():
            if case['filter-match']['status'] == 'matches':
                names.append(name)
    if len(names) != len(CASES) or set(names) != set(CASES):
        raise ValueError(f'Unexpected selection: {names}')
    return sorted(names)


def terminal_results(text):
    text = normalize(text)
    rows = {}
    pattern = r'\b(PASS|FAIL|SIGTRAP|SIGILL|SIGABRT|ABORT|TIMEOUT)\s+\[[^\]]+\]\s+\(\s*(\d+)/(\d+)\)\s+\S+\s+(\S+)'
    for status, ordinal, total, name in re.findall(pattern, text):
        row = dict(status=status, name=name, total=int(total))
        key = int(ordinal)
        if key in rows and rows[key] != row:
            raise ValueError('Contradictory test completions')
        rows[key] = row
    summaries = re.findall(r'Summary\s+\[[^\]]+\]\s+(\d+) tests run: (\d+) passed(?: \(\d+ slow\))?(?:, (\d+) failed)?(?:, (\d+) skipped)?', text)
    if len(summaries) != 1:
        raise ValueError(f'Expected one terminal summary, got {summaries}')
    total, passed, failed, skipped = (int(value or 0) for value in summaries[0])
    if (total != len(CASES) or total != passed + failed or
            len(rows) != total or {x['name'] for x in rows.values()} != set(CASES) or
            any(x['total'] != total for x in rows.values()) or
            sum(x['status'] == 'PASS' for x in rows.values()) != passed):
        raise ValueError('Incomplete/incorrect exact-test execution')
    return dict(total=total, passed=passed, failed=failed, skipped=skipped,
                completions=rows)


def verify_history(root, expected_source):
    def git(*args):
        return subprocess.check_output(['git', *args], cwd=root, text=True,
                                       stderr=subprocess.PIPE).strip()
    if git('rev-parse', 'HEAD') != expected_source:
        raise ValueError('Unexpected diagnostic source')
    try:
        parent = git('rev-parse', '--verify', 'HEAD^')
        failing_base = git('rev-parse', '--verify', 'HEAD^^')
    except subprocess.CalledProcessError as error:
        raise ValueError('Diagnostic history unavailable; checkout requires depth 3') from error
    if parent != DIAGNOSTIC_PARENT or failing_base != BASE:
        raise ValueError('Unexpected diagnostic parent or original failing base')
    if git('diff', 'HEAD'):
        raise ValueError('Tracked source differs from the pinned diagnostic commit')
    return parent


def preflight():
    root = Path.cwd()
    evidence = root / 'diagnostic-evidence'
    evidence.mkdir(exist_ok=True)
    assert platform.system() == 'Darwin'
    assert os.environ['GITHUB_REF'] == BRANCH
    assert os.environ['GITHUB_REPOSITORY'] == 'ergohaven/entropy'
    assert os.environ['GITHUB_ACTOR'] == 'kissetfall'
    arch = platform.machine()
    assert arch == os.environ['EXPECTED_ARCH']
    assert os.environ['EXPECTED_RUNNER'] == {'arm64': 'macos-15', 'x86_64': 'macos-15-intel'}[arch]
    source = os.environ['GITHUB_SHA']
    parent = verify_history(root, source)
    receipt: dict = dict(source=source, parent=parent, failing_base=BASE, arch=arch,
                   expected_runner=os.environ['EXPECTED_RUNNER'],
                   run_id=os.environ['GITHUB_RUN_ID'], attempt=os.environ['GITHUB_RUN_ATTEMPT'],
                   actor=os.environ['GITHUB_ACTOR'], cases=CASES, filter=FILTER,
                   subset_not_full_historical_load=True, product_acceptance=False,
                   owner_sha256={name: sha256(root / name) for name in OWNERS})
    (evidence / 'preflight.json').write_text(json.dumps(receipt, indent=2))
    return evidence, receipt


def main():
    root = Path.cwd()
    evidence, receipt = preflight()
    record = evidence / 'receipt.json'
    record.write_text(json.dumps(receipt, indent=2))
    reuse = ['--binaries-metadata', str(evidence / 'binaries-metadata.json'),
             '--cargo-metadata', str(evidence / 'cargo-metadata.json')]
    receipt['metadata_sha256'] = {str(p.name): sha256(p) for p in
        (evidence / 'binaries-metadata.json', evidence / 'cargo-metadata.json')}
    binary_data = json.loads((evidence / 'binaries-metadata.json').read_text())
    candidates = [Path(item['binary-path']) for item in binary_data['rust-binaries'].values()]
    binaries = []
    for path in candidates:
        listing = subprocess.check_output([str(path), '--list', '--format', 'terse'], text=True)
        names = {line.removesuffix(': test') for line in listing.splitlines() if line.endswith(': test')}
        if set(CASES) <= names:
            binaries.append(path)
    assert len(binaries) == 1, binaries
    binary = binaries[0]
    receipt['binary'] = str(binary)
    receipt['binary_sha256_reported'] = sha256(binary)
    select = subprocess.run(['cargo', 'nextest', 'list', *reuse, '-E', FILTER,
                             '--message-format', 'json'], capture_output=True, text=True, timeout=30)
    (evidence / 'selection.json').write_text(select.stdout)
    (evidence / 'selection.stderr').write_text(select.stderr)
    assert select.returncode == 0, select.stderr
    receipt['selected_names'] = selected_names(select.stdout)
    # Exercise real metadata reuse and reject absent/broad selection without executing tests.
    receipt['negative_controls'] = []
    for expression in ('test(=missing_case_control)', 'all()'):
        control = subprocess.run(['cargo', 'nextest', 'list', *reuse, '-E', expression,
                                  '--message-format', 'json'], capture_output=True, text=True, timeout=30)
        assert control.returncode == 0, control.stderr
        try:
            selected_names(control.stdout)
        except ValueError:
            receipt['negative_controls'].append({'filter': expression, 'rejected': True})
        else:
            raise AssertionError('Unsafe selection admitted')
    invalid = evidence / 'invalid-metadata-control.json'
    invalid.write_text('not-json')
    for path in (invalid, evidence / 'absent-metadata-control.json'):
        control = subprocess.run(['cargo', 'nextest', 'list', '--binaries-metadata', str(path),
                                  '--cargo-metadata', str(evidence / 'cargo-metadata.json')],
                                 capture_output=True, text=True, timeout=30)
        assert control.returncode != 0, 'Invalid metadata admitted'
        receipt['negative_controls'].append({'metadata': path.name, 'exit_code': control.returncode})
    record.write_text(json.dumps(receipt, indent=2))
    start_wall = time.time()
    run = bounded(['cargo', 'nextest', 'run', *reuse, '-E', FILTER,
                   '--test-threads', '4', '--no-fail-fast', '--success-output', 'immediate',
                   '--failure-output', 'immediate'], evidence / 'nextest.log', 180)
    receipt['nextest'] = run
    record.write_text(json.dumps(receipt, indent=2))
    errors = []
    runtime_text = (evidence / 'nextest.log').read_text()
    if 'Compiling ' in runtime_text:
        errors.append('Unexpected compilation inside execution budget')
    try:
        receipt['terminal_results'] = terminal_results(runtime_text)
    except ValueError as error:
        errors.append(str(error))
    receipt['telemetry'] = [line.strip() for line in normalize(runtime_text).splitlines()
                            if 'MACDIAG ' in line]
    if not any('MACDIAG request' in line for line in receipt['telemetry']):
        errors.append('No HID request telemetry observed')
    # A debugger changes scheduling: these are diagnostic controls, not reruns for acceptance.
    receipt['debugger_controls'] = []
    for index, name in enumerate(CASES):
        output = evidence / f'lldb-{index}.json'
        env = dict(os.environ, ENTROPY_LLDB_RECEIPT=str(output))
        script = root / 'scripts' / 'macos_lldb_capture.py'
        command = ['lldb', '--batch', '-o',
                   f'script exec(compile(open({str(script)!r}).read(), {str(script)!r}, "exec"))',
                   '--', str(binary), '--exact', name, '--nocapture']
        result = bounded(command, evidence / f'lldb-{index}.log', 90, env=env)
        result['case'] = name
        if output.exists():
            result['capture'] = json.loads(output.read_text())
        else:
            errors.append(f'No LLDB receipt for {name}')
        receipt['debugger_controls'].append(result)
        record.write_text(json.dumps(receipt, indent=2))
    reports = []
    report_root = Path.home() / 'Library/Logs/DiagnosticReports'
    if report_root.exists():
        for path in report_root.glob('entropy*'):
            if path.is_file() and path.stat().st_mtime >= start_wall:
                destination = evidence / ('native-' + path.name)
                shutil.copy2(path, destination)
                reports.append(destination.name)
    receipt['native_crash_reports'] = reports
    receipt['controller_errors'] = errors
    receipt['capture_complete'] = not errors and not run['timed_out']
    record.write_text(json.dumps(receipt, indent=2))
    print(json.dumps(receipt, indent=2))
    return 0 if receipt['capture_complete'] else 1


if __name__ == '__main__':
    if sys.argv[1:] == ['--preflight']:
        _, source_receipt = preflight()
        print(json.dumps(source_receipt, indent=2))
    elif not sys.argv[1:]:
        raise SystemExit(main())
    else:
        raise SystemExit('Usage: macos_bounded_diagnostic.py [--preflight]')
