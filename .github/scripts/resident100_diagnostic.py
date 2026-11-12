"""Observe exactly one process-isolated test, never infer a full-suite pass."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import subprocess
import time

TEST = "qmk_hid_host::resident100_tests::resident100_library_publication_admits_all_100_and_syncs_without_select"
COMMAND = ["cargo", "nextest", "run", "--test-threads", "4", "--no-fail-fast", "--success-output", "immediate", "--failure-output", "immediate", "-E", "test(=" + TEST + ")"]


def execution_command(metadata):
    files = [metadata / "binaries-metadata.json", metadata / "cargo-metadata.json"]
    for path in files:
        if not path.is_file():
            raise FileNotFoundError(f"Missing precompiled-test metadata: {path}")
        json.loads(path.read_text())
    return COMMAND + ["--binaries-metadata", str(files[0]), "--cargo-metadata", str(files[1])]


def classify(text, code):
    text = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", text)
    summaries = re.findall(r"Summary\s+\[[^]\n]+\]\s+([^\n]+)", text)
    if len(summaries) != 1 or TEST not in text:
        return "incomplete"
    summary = summaries[0]
    count = re.match(r"(\d+) tests? run:", summary)
    passed = re.search(r"\b(\d+) passed\b", summary)
    failed = re.search(r"\b(\d+) failed\b", summary)
    if count is None or int(count[1]) != 1:
        return "incomplete"
    if code == 0 and passed and int(passed[1]) == 1 and not failed:
        return "one_test_passed"
    if code != 0 and failed and int(failed[1]) == 1:
        return "one_test_failed"
    return "incomplete"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--metadata", type=Path, required=True)
    args = parser.parse_args()
    command = execution_command(args.metadata)
    args.output.mkdir(parents=True, exist_ok=True)
    def output(*command):
        return subprocess.check_output(command, text=True).strip()
    sources = ["src/qmk_hid_host.rs", "src/qmk_hid_host/resident100.rs", "src/qmk_hid_host/resident100_tests.rs", "src/hid.rs"]
    receipt = {
        "scope": "One diagnostic test, not full-suite or hardware acceptance",
        "head": output("git", "rev-parse", "HEAD"),
        "tree": output("git", "rev-parse", "HEAD^{tree}"),
        "platform": platform.platform(), "machine": platform.machine(),
        "rustc": output("rustc", "--version"), "nextest": output("cargo", "nextest", "--version"),
        "command": command, "test": TEST,
        "compilation_allowed": False,
        "build_metadata_sha256": {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in args.metadata.glob("*.json")},
        "run_id": os.environ.get("GITHUB_RUN_ID"), "run_attempt": os.environ.get("GITHUB_RUN_ATTEMPT"),
        "source_sha256": {p: hashlib.sha256(Path(p).read_bytes()).hexdigest() for p in sources},
        "diagnostics_enabled": bool(os.environ.get("ENTROPY_RESIDENT100_DIAGNOSTICS")),
        "status": "running",
    }
    destination = args.output / "receipt.json"
    destination.write_text(json.dumps(receipt, indent=2))
    started = time.monotonic()
    log = args.output / "test.log"
    with log.open("w") as stream:
        with subprocess.Popen(command, stdout=stream, stderr=subprocess.STDOUT, start_new_session=True) as process:
            try:
                code = process.wait(timeout=240)
            except subprocess.TimeoutExpired:
                import signal
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()
                code = 124
                receipt["timeout"] = True
    text = log.read_text()
    receipt.update(exit_code=code, seconds=time.monotonic() - started, status=classify(text, code))
    receipt["metadata"] = [line.lstrip() for line in text.splitlines() if line.lstrip().startswith("RESIDENT100_")]
    destination.write_text(json.dumps(receipt, indent=2))
    print(text, end="")
    print("DIAGNOSTIC_RESULT", json.dumps({k: receipt[k] for k in ("status", "exit_code", "seconds", "head")}))
    return 0 if receipt["status"] == "one_test_passed" else code or 2


if __name__ == "__main__":
    raise SystemExit(main())
