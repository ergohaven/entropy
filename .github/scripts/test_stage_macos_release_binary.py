#!/usr/bin/env python3
"""Secret-free staging tests: tiny inert Mach-O headers, never executed."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import plistlib
import shutil
import stat
import struct
import subprocess
import sys
import tempfile
from unittest import mock
import warnings
import unittest
import zipfile

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts/stage_macos_release_binary.py"
spec = importlib.util.spec_from_file_location("stage_macos_release_binary", SCRIPT)
assert spec is not None and spec.loader is not None
stager = importlib.util.module_from_spec(spec)
spec.loader.exec_module(stager)

ARM = "aarch64-apple-darwin"
INTEL = "x86_64-apple-darwin"
BINARY = "Entropy.app/Contents/MacOS/entropy"
PLIST = "Entropy.app/Contents/Info.plist"


class Staging(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name).resolve()
        env = mock.patch.dict(os.environ, {"CARGO_TARGET_DIR": ""})
        env.start()
        self.addCleanup(env.stop)
        self.source = self.root / "input"
        self.source.mkdir()
        self.archive = self.source / "entropy-v0.8-macos-arm64.app.zip"
        self.info: dict[str, object] = {
            "CFBundleIdentifier": "com.ergohaven.entropy",
            "CFBundleExecutable": "entropy",
            "CFBundleShortVersionString": "0.8",
            "CFBundleVersion": "0.8",
        }
        self.binary = struct.pack("<8I", 0xFEEDFACF, 0x0100000C, 0, 2, 0, 0, 0, 0) + b"inert fixture\0\xff"
        self.destination = self.root / "target" / ARM / "release/entropy"

    def fixture(self, extra=(), mode=stat.S_IFREG | 0o755, plist=None, binary=None,
                compression=zipfile.ZIP_STORED):
        entries = [("Entropy.app/", b"", stat.S_IFDIR | 0o755),
                   (PLIST, plistlib.dumps(self.info) if plist is None else plist, stat.S_IFREG | 0o644),
                   (BINARY, self.binary if binary is None else binary, mode)]
        with zipfile.ZipFile(self.archive, "w") as archive:
            for name, payload, permissions in entries + list(extra):
                item = zipfile.ZipInfo(name)
                item.create_system = 3
                item.external_attr = permissions << 16
                item.compress_type = compression
                archive.writestr(item, payload)

    def stage(self, **kwargs):
        return stager.stage(self.source, ARM, root=self.root, version="0.8", **kwargs)

    def test_stages_only_exact_binary_bytes_with_receipt(self):
        self.fixture(extra=[("__MACOSX/._Entropy.app", b"metadata", stat.S_IFREG | 0o644)])
        receipt = self.stage()
        self.assertEqual(self.destination.read_bytes(), self.binary)
        self.assertEqual(stat.S_IMODE(self.destination.stat().st_mode), 0o755)
        self.assertEqual(receipt["binary_sha256"], hashlib.sha256(self.binary).hexdigest())
        self.assertEqual(receipt["staged_sha256"], receipt["binary_sha256"])
        self.assertEqual(receipt["source_sha256"], hashlib.sha256(self.archive.read_bytes()).hexdigest())
        self.assertEqual(receipt["source"], str(self.archive))
        self.assertEqual(receipt["destination"], str(self.destination))
        self.assertEqual(receipt["target"], ARM)
        self.assertEqual(receipt["version"], "0.8")
        self.assertEqual(receipt["member"], BINARY)
        self.assertEqual(receipt["size"], len(self.binary))
        self.assertEqual([p for p in (self.root / "target").rglob("*") if p.is_file()], [self.destination])

    def test_rejects_wrong_bundle_identity_versions_or_plist(self):
        for key, bad in (("CFBundleIdentifier", "other.app"),
                         ("CFBundleExecutable", "../../other"),
                         ("CFBundleShortVersionString", "0.8.0"),
                         ("CFBundleVersion", "0.7"),
                         ("CFBundleVersion", 0.8),
                         ("CFBundleVersion", None)):
            with self.subTest(key=key, bad=bad):
                good = dict(self.info)
                if bad is None:
                    del self.info[key]
                else:
                    self.info[key] = bad
                self.fixture()
                with self.assertRaisesRegex(ValueError, "plist|bundle"):
                    self.stage()
                self.assertFalse(self.destination.exists())
                self.info = good
        for payload in (b"not a plist", plistlib.dumps(["not a dictionary"])):
            with self.subTest(plist=payload):
                self.fixture(plist=payload)
                with self.assertRaisesRegex(ValueError, "plist|bundle"):
                    self.stage()
                self.assertFalse(self.destination.exists())

    def test_rejects_nonexecutable_non_thin_wrong_cpu_or_bad_macho(self):
        cases = [(0o100644, self.binary), (0o100000, self.binary),
                 (0o100755, self.binary[:4]),
                 (0o100755, b"not Mach-O" * 5),
                 (0o100755, struct.pack(">8I", 0xFEEDFACF, 0x0100000C, 0, 2, 0, 0, 0, 0)),
                 (0o100755, struct.pack(">4I", 0xCAFEBABE, 1, 0x0100000C, 0)),
                 (0o100755, struct.pack("<8I", 0xFEEDFACF, 0x01000007, 0, 2, 0, 0, 0, 0)),
                 (0o100755, struct.pack("<8I", 0xFEEDFACF, 0x0100000C, 0, 6, 0, 0, 0, 0)),
                 (0o100755, struct.pack("<8I", 0xFEEDFACF, 0x0100000C, 0, 2, 1, 100, 0, 0))]
        for mode, payload in cases:
            with self.subTest(mode=oct(mode), payload=payload):
                self.fixture(mode=mode, binary=payload)
                with self.assertRaisesRegex(ValueError, "executable|Mach-O"):
                    self.stage()
                self.assertFalse(self.destination.exists())

    def test_intel_archive_uses_intel_binary_and_fixed_destination(self):
        self.archive = self.source / "entropy-v0.8-macos-x86_64.app.zip"
        self.binary = struct.pack("<8I", 0xFEEDFACF, 0x01000007, 0, 2, 0, 0, 0, 0) + b"intel"
        self.fixture()
        receipt = stager.stage(self.source, INTEL, root=self.root, version="0.8")
        destination = self.root / "target" / INTEL / "release/entropy"
        self.assertEqual(destination.read_bytes(), self.binary)
        self.assertEqual(receipt["target"], INTEL)
        self.assertEqual(receipt["binary_sha256"], hashlib.sha256(self.binary).hexdigest())

    def test_rejects_unsupported_target_before_writing(self):
        self.fixture()
        for target in ("arm64", "../../escape", "aarch64-apple-darwin/../other", ""):
            with self.subTest(target=target), self.assertRaisesRegex(ValueError, "target"):
                stager.stage(self.source, target, root=self.root, version="0.8")
        self.assertFalse((self.root / "target").exists())

    def test_rejects_unsafe_duplicate_and_nonregular_members(self):
        paths = ("../escape", "/absolute", "C:/drive", "Entropy.app/../escape",
                 "Entropy.app/./alias", "Entropy.app//alias", "Entropy.app\\escape",
                 "Entropy.app/Contents/MacOS/ENTROPY", "Entropy.app/", BINARY)
        entries = [(path, b"payload", stat.S_IFREG | 0o644) for path in paths]
        entries += [("link", b"outside", stat.S_IFLNK | 0o777),
                    ("fifo", b"", stat.S_IFIFO | 0o644),
                    ("device", b"", stat.S_IFCHR | 0o644),
                    ("untyped", b"data", 0o644),
                    ("dir/", b"", stat.S_IFREG | 0o755),
                    ("not-dir", b"", stat.S_IFDIR | 0o755),
                    ("nonempty/", b"hidden", stat.S_IFDIR | 0o755),
                    ("Entropy.app/Contents", b"file blocks ancestor", stat.S_IFREG | 0o644)]
        for entry in entries:
            with self.subTest(entry=entry):
                with warnings.catch_warnings():
                    warnings.simplefilter("ignore", UserWarning)
                    self.fixture(extra=[entry])
                with self.assertRaises(ValueError):
                    self.stage()
                self.assertFalse(self.destination.exists())

    def test_rejects_bad_zip_missing_members_and_bad_crc_anywhere(self):
        for payload in (b"not zip", b"PK\x03\x04truncated"):
            self.archive.write_bytes(payload)
            with self.subTest(payload=payload), self.assertRaises(ValueError):
                self.stage()
        for name in (BINARY, PLIST):
            self.fixture()
            with zipfile.ZipFile(self.archive) as archive:
                entries = [(item, archive.read(item)) for item in archive.infolist() if item.filename != name]
            with zipfile.ZipFile(self.archive, "w") as archive:
                for item, data in entries:
                    archive.writestr(item, data)
            with self.subTest(missing=name), self.assertRaises(ValueError):
                self.stage()
        for name in (BINARY, PLIST, "unused-resource"):
            self.fixture(extra=[("unused-resource", b"CRC must be checked here too", stat.S_IFREG | 0o644)])
            data = bytearray(self.archive.read_bytes())
            with zipfile.ZipFile(self.archive) as archive:
                offset = archive.getinfo(name).header_offset
            name_length, extra_length = struct.unpack_from("<HH", data, offset + 26)
            data[offset + 30 + name_length + extra_length] ^= 1
            self.archive.write_bytes(data)
            with self.subTest(crc=name), self.assertRaises(ValueError):
                self.stage()
        self.assertFalse((self.root / "target").exists())

    def test_rejects_size_and_count_limits_without_large_allocations(self):
        for constant, limit in (("MAX_ARCHIVE_BYTES", 16), ("MAX_MEMBER_BYTES", 16),
                                ("MAX_TOTAL_BYTES", 16), ("MAX_ENTRIES", 2),
                                ("MAX_PLIST_BYTES", 16)):
            with self.subTest(constant=constant):
                self.fixture()
                with mock.patch.object(stager, constant, limit, create=True):
                    with self.assertRaisesRegex(ValueError, "limit|large|size"):
                        self.stage()
                self.assertFalse(self.destination.exists())

    def test_source_inventory_requires_one_exact_regular_archive(self):
        self.fixture()
        for name in ("other.app.zip", "entropy-v0.8-macos-x86_64.app.zip", "unexpected.txt", "nested"):
            extra = self.source / name
            if name == "nested":
                extra.mkdir()
            else:
                extra.write_bytes(b"extra")
            with self.subTest(name=name), self.assertRaises(ValueError):
                self.stage()
            if extra.is_dir():
                extra.rmdir()
            else:
                extra.unlink()
        original = self.archive.read_bytes()
        self.archive.unlink()
        with self.assertRaises(ValueError):
            self.stage()
        wrong = self.source / "entropy-v0.8.0-macos-arm64.app.zip"
        wrong.write_bytes(original)
        with self.assertRaises(ValueError):
            self.stage()
        wrong.unlink()
        outside = self.root / "outside.zip"
        outside.write_bytes(original)
        self.archive.symlink_to(outside)
        with self.assertRaises(ValueError):
            self.stage()
        self.assertFalse(self.destination.exists())

    def test_refuses_existing_destination_without_altering_it(self):
        self.fixture()
        self.destination.parent.mkdir(parents=True)
        self.destination.write_bytes(b"keep original")
        self.destination.chmod(0o600)
        with self.assertRaisesRegex(ValueError, "destination|exist"):
            self.stage()
        self.assertEqual(self.destination.read_bytes(), b"keep original")
        self.assertEqual(stat.S_IMODE(self.destination.stat().st_mode), 0o600)

    def test_refuses_symlink_destination_or_any_parent(self):
        self.fixture()
        outside = self.root / "outside"
        outside.mkdir()
        for index, relative in enumerate(("target", "target/" + ARM, "target/" + ARM + "/release",
                                          "target/" + ARM + "/release/entropy")):
            with self.subTest(path=relative):
                root = self.root / ("case-%d" % index)
                path = root / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.symlink_to(outside, target_is_directory=True)
                try:
                    with self.assertRaisesRegex(ValueError, "symlink|destination|directory"):
                        stager.stage(self.source, ARM, root=root, version="0.8")
                    self.assertEqual(list(outside.iterdir()), [])
                finally:
                    path.unlink()
        self.destination.parent.mkdir(parents=True, exist_ok=True)
        self.destination.symlink_to(self.root / "does-not-exist")
        with self.assertRaises(ValueError):
            self.stage()
        self.assertTrue(self.destination.is_symlink())
        self.assertFalse((self.root / "does-not-exist").exists())

    def test_explicit_fixture_target_directory_is_fixed_and_no_overwrite(self):
        self.fixture()
        target_dir = self.root / "fixture-target"
        receipt = self.stage(target_dir=target_dir)
        destination = target_dir / ARM / "release/entropy"
        self.assertEqual(destination.read_bytes(), self.binary)
        self.assertEqual(receipt["destination"], str(destination))
        self.assertFalse((self.root / "target").exists())
        with self.assertRaises(ValueError):
            self.stage(target_dir=target_dir)

    def test_source_directory_and_source_ancestor_symlinks_reject(self):
        self.fixture()
        for path, actual in ((self.root / "source-link", self.source),
                             (self.root / "root-link", self.root)):
            path.symlink_to(actual, target_is_directory=True)
            source = path if actual == self.source else path / "input"
            with self.subTest(source=source), self.assertRaises(ValueError):
                stager.stage(source, ARM, root=self.root, version="0.8")
        self.assertFalse(self.destination.exists())

    def test_partial_destination_is_removed_after_write_failure(self):
        self.fixture()
        with mock.patch.object(os, "fchmod", side_effect=OSError("fixture chmod failure")):
            with self.assertRaisesRegex(ValueError, "destination"):
                self.stage()
        self.assertFalse(self.destination.exists())

    def cli_fixture(self):
        scripts = self.root / "scripts"
        scripts.mkdir()
        shutil.copy2(SCRIPT, scripts)
        shutil.copy2(ROOT / "scripts/public_version.sh", scripts)
        (self.root / "Cargo.toml").write_text(
            '[package]\nversion = "0.8.0"\n[package.metadata.entropy]\npublic-version = "0.8"\n')
        self.fixture()
        return scripts / SCRIPT.name

    def run_cli(self, script, extra=(), env=None):
        return subprocess.run([sys.executable, str(script), "--source", str(self.source),
                               "--target", ARM] + list(extra),
                              cwd=self.source, env=env, capture_output=True, text=True)

    def test_cli_reads_actual_public_version_not_environment_and_prints_json(self):
        script = self.cli_fixture()
        env = dict(os.environ, VERSION="99.99", ROOT="/not/the/repository",
                   ENTROPY_VERSION="99.99", DESTINATION=str(self.root / "forbidden"))
        result = self.run_cli(script, env=env)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stderr, "")
        self.assertTrue(result.stdout.strip(), "CLI must emit a staging receipt")
        receipt = json.loads(result.stdout)
        self.assertEqual(receipt["version"], "0.8")
        self.assertEqual(self.destination.read_bytes(), self.binary)
        self.assertEqual(receipt["staged_sha256"], hashlib.sha256(self.binary).hexdigest())
        self.assertFalse((self.root / "forbidden").exists())

    def test_cli_has_no_root_version_or_destination_override(self):
        script = self.cli_fixture()
        for option in ("--version", "--root", "--destination", "--target-dir"):
            with self.subTest(option=option):
                result = self.run_cli(script, [option, "untrusted"])
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(result.stdout, "")
                self.assertFalse(self.destination.exists())

    def test_public_version_helper_failure_cannot_stage(self):
        script = self.cli_fixture()
        (self.root / "Cargo.toml").write_text(
            '[package]\nversion = "0.9.0"\n[package.metadata.entropy]\npublic-version = "0.8"\n')
        result = self.run_cli(script)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "")
        self.assertFalse(self.destination.exists())
        (self.root / "scripts/public_version.sh").unlink()
        result = self.run_cli(script)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "")

    def test_cargo_target_dir_environment_is_the_only_destination_base_override(self):
        self.fixture()
        for base in (str(self.root / "absolute-cache"), "relative-cache"):
            with self.subTest(base=base), mock.patch.dict(os.environ, CARGO_TARGET_DIR=base):
                receipt = self.stage()
                target_base = Path(base) if Path(base).is_absolute() else self.root / base
                destination = target_base / ARM / "release/entropy"
                self.assertEqual(destination.read_bytes(), self.binary)
                self.assertEqual(receipt["destination"], str(destination))
        self.assertFalse(self.destination.exists())

    def test_cli_rejects_source_version_different_from_manifest(self):
        script = self.cli_fixture()
        self.archive.rename(self.source / "entropy-v0.7-macos-arm64.app.zip")
        result = self.run_cli(script)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "")
        self.assertFalse(self.destination.exists())

    def test_invalid_xml_plist_fails_closed_without_destination(self):
        for payload in (b'<?xml version="1.0"?><plist><dict>',
                        b'<?xml version="1.0"?><plist><dict><key>x</key><integer>bad</integer></dict></plist>'):
            self.fixture(plist=payload)
            with self.subTest(payload=payload), self.assertRaisesRegex(ValueError, "plist"):
                self.stage()
            self.assertFalse(self.destination.exists())

    def test_import_is_safe_with_unrelated_arguments_and_no_processes(self):
        spec = importlib.util.spec_from_file_location("staging_import_probe", SCRIPT)
        assert spec is not None and spec.loader is not None
        module = importlib.util.module_from_spec(spec)
        with mock.patch.object(sys, "argv", [str(SCRIPT), "--unrelated"]), \
             mock.patch.object(subprocess, "run", side_effect=AssertionError("import spawned a process")), \
             mock.patch.object(os, "open", side_effect=AssertionError("import opened a destination")):
            spec.loader.exec_module(module)
        self.assertTrue(callable(module.stage))

    def test_deflated_bundle_with_binary_plist_preserves_bytes(self):
        self.fixture(plist=plistlib.dumps(self.info, fmt=plistlib.FMT_BINARY),
                     compression=zipfile.ZIP_DEFLATED)
        receipt = self.stage()
        self.assertEqual(self.destination.read_bytes(), self.binary)
        self.assertEqual(receipt["staged_sha256"], hashlib.sha256(self.binary).hexdigest())


if __name__ == "__main__":
    unittest.main()
