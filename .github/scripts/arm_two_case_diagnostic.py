"""Two approved cases only. Stack/telemetry evidence is not product acceptance."""
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

SIGNAL_TEST = "app::app_lifecycle::tests::settings_disconnect_allows_deferred_exit_after_completed_combo"
RESIDENT_TEST = "qmk_hid_host::resident_regressions::resident_three_settings_lease_recovery_and_worker_restart_use_query_select_only"


def binary_from_metadata(directory):
    binaries = json.loads((directory / "binaries-metadata.json").read_text())["rust-binaries"]
    selected = [value for value in binaries.values() if value["binary-name"] == "entropy" and value["kind"] == "bin"]
    if len(selected) != 1:
        raise ValueError("Expected exactly one entropy test binary")
    path = Path(selected[0]["binary-path"])
    if not path.is_file():
        raise FileNotFoundError(path)
    json.loads((directory / "cargo-metadata.json").read_text())
    return path


def stack_command(binary):
    return ["xcrun", "lldb", "--batch", "-o", "settings set target.disable-aslr false", "-o", "run", "-k", "thread backtrace all", "-k", "register read", "--", str(binary), SIGNAL_TEST, "--exact", "--nocapture", "--test-threads=1"]


def resident_command(directory):
    return ["cargo", "nextest", "run", "--binaries-metadata", str(directory / "binaries-metadata.json"), "--cargo-metadata", str(directory / "cargo-metadata.json"), "--test-threads", "4", "--no-fail-fast", "--success-output", "immediate", "--failure-output", "immediate", "-E", "test(=" + RESIDENT_TEST + ")"]


def classify_stack(text, code):
    if "stop reason =" in text and re.search(r"frame #\d+:", text):
        return "native_stop_with_stack"
    if code == 0 and SIGNAL_TEST in text and re.search(r"test result: ok\. 1 passed; 0 failed;", text):
        return "one_test_passed_under_debugger"
    return "incomplete"


def classify_resident(text, code):
    text = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", text)
    summaries = re.findall(r"Summary\s+\[[^]\n]+\]\s+1 test run:([^\n]+)", text)
    if len(summaries) != 1 or RESIDENT_TEST not in text:
        return "incomplete"
    if code == 0 and re.search(r"\b1 passed\b", summaries[0]) and "failed" not in summaries[0]:
        return "one_test_passed"
    if code != 0 and re.search(r"\b1 failed\b", summaries[0]):
        return "one_test_failed"
    return "incomplete"


def execute(command, log, timeout=180):
    started = time.monotonic()
    with log.open("w") as stream:
        with subprocess.Popen(command, stdout=stream, stderr=subprocess.STDOUT, start_new_session=True) as process:
            try:
                code = process.wait(timeout=timeout)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()
                code = 124
    return code, time.monotonic() - started, log.read_text()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--metadata", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    binary = binary_from_metadata(args.metadata)
    metadata = json.loads((args.metadata / "binaries-metadata.json").read_text())
    host = metadata["rust-build-meta"]["platforms"]["host"]
    paths = [str(binary.parent), host["libdir"].get("path", ""), os.environ.get("DYLD_FALLBACK_LIBRARY_PATH", "")]
    os.environ["DYLD_FALLBACK_LIBRARY_PATH"] = os.pathsep.join(path for path in paths if path)
    def output(*command):
        return subprocess.check_output(command, text=True).strip()
    sources = ["src/qmk_hid_host/resident_regressions.rs", "src/qmk_hid_host/resident100.rs", "src/ui/app_lifecycle.rs", "src/app_init.rs"]
    receipt = {
        "scope": "One stack capture and one resident observation, no retries/full matrix/acceptance",
        "head": output("git", "rev-parse", "HEAD"), "tree": output("git", "rev-parse", "HEAD^{tree}"),
        "run_id": os.environ.get("GITHUB_RUN_ID"), "run_attempt": os.environ.get("GITHUB_RUN_ATTEMPT"),
        "platform": platform.platform(), "machine": platform.machine(), "rustc": output("rustc", "--version"),
        "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "source_sha256": {path: hashlib.sha256(Path(path).read_bytes()).hexdigest() for path in sources},
        "compilation_allowed": False, "runs": [],
    }
    target = args.output / "receipt.json"
    target.write_text(json.dumps(receipt, indent=2))
    cases = [("native-stack", stack_command(binary), classify_stack), ("resident-read", resident_command(args.metadata), classify_resident)]
    for name, command, classify in cases:
        code, seconds, text = execute(command, args.output / (name + ".log"))
        result = {"case": name, "command": command, "exit_code": code, "seconds": seconds, "status": classify(text, code), "telemetry": [line.lstrip() for line in text.splitlines() if line.lstrip().startswith("ARM_RESIDENT_")]}
        receipt["runs"].append(result)
        target.write_text(json.dumps(receipt, indent=2))
        print("ARM_DIAGNOSTIC_RESULT", json.dumps({key: result[key] for key in ("case", "status", "exit_code", "seconds")}), flush=True)
    # A captured crash is useful diagnosis, not a passing product test.
    return 0 if all(run["status"] in ("one_test_passed_under_debugger", "one_test_passed") for run in receipt["runs"]) else 1


if __name__ == "__main__":
    raise SystemExit(main())
