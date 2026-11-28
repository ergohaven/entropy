"""Release publication contracts; all binary and account data are fixtures."""
import hashlib
import importlib.util

import os
from pathlib import Path

import sys
import tempfile

import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
candidate_spec = importlib.util.spec_from_file_location('release_candidate', ROOT / 'scripts/release_candidate.py')
assert candidate_spec is not None and candidate_spec.loader is not None
release_candidate = importlib.util.module_from_spec(candidate_spec)
candidate_spec.loader.exec_module(release_candidate)
spec = importlib.util.spec_from_file_location("prepare_release_assets", ROOT / "scripts/prepare_release_assets.py")
assert spec is not None and spec.loader is not None
assets = importlib.util.module_from_spec(spec)
spec.loader.exec_module(assets)


class ReleasePublication(unittest.TestCase):
    def fixture(self, root, tag="v0.7"):
        source = root / "inputs"
        names = [f"entropy-{tag}-{suffix}" for suffix in (
            "x86_64.AppImage", "amd64.deb", "x86_64.rpm", "x86_64.pkg.tar.zst",
            "windows-x86_64.exe", "macos-arm64.dmg", "macos-x86_64.dmg",
        )]
        for index, name in enumerate(names):
            path = source / str(index) / name
            path.parent.mkdir(parents=True)
            path.write_bytes(f"release inventory fixture {index}".encode())
        return source, names

    def test_exact_inventory_and_actual_checksum_bytes(self):
        for tag in ("v0.7", "v0.7.1", "v0.8-rc.1"):
            with self.subTest(tag=tag), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                source, names = self.fixture(root, tag)
                destination = root / "output"
                self.assertEqual(assets.collect(source, destination, tag), sorted(names + ["SHA256SUMS.txt"]))
                checksums = (destination / "SHA256SUMS.txt").read_text().splitlines()
                self.assertEqual(len(checksums), len(names))
                for line in checksums:
                    digest, name = line.split("  ", 1)
                    original = next(source.rglob(name)).read_bytes()
                    self.assertEqual((destination / name).read_bytes(), original)
                    self.assertEqual(digest, hashlib.sha256(original).hexdigest())

    def test_missing_duplicate_extra_symlink_and_empty_inputs_reject(self):
        for fault in ("missing", "duplicate", "extra", "symlink", "empty"):
            with self.subTest(fault=fault), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                source, names = self.fixture(root)
                first = next(source.rglob(names[0]))
                if fault == "missing":
                    first.unlink()
                elif fault == "duplicate":
                    (source / names[0]).write_bytes(first.read_bytes())
                elif fault == "extra":
                    (source / "unexpected.txt").write_text("fixture")
                elif fault == "symlink":
                    (source / "link").symlink_to(first)
                else:
                    first.write_bytes(b"")
                with self.assertRaises(ValueError):
                    assets.collect(source, root / "output", "v0.7")
                self.assertFalse((root / "output").exists())

    def test_existing_output_and_noncanonical_tags_reject(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source, _ = self.fixture(root)
            destination = root / "output"
            destination.mkdir()
            for tag in ("v0.7", "v0.07", "v0.7/other", "0.7", "v0"):
                with self.subTest(tag=tag), self.assertRaises(ValueError):
                    assets.collect(source, destination, tag)

    def test_publisher_secret_is_release_only_and_draft_is_not_latest(self):
        workflow = (ROOT / ".github/workflows/release.yml").read_text()
        before, release = workflow.split("\n  release:\n", 1)
        secret = "${{ secrets.ERGOHAVEN_RELEASE_TOKEN }}"
        self.assertNotIn(secret, before)
        self.assertEqual(release.count(secret), 1)
        self.assertIn('needs: plan', release)
        self.assertIn('needs: [plan, build, validate, nix]', before)
        self.assertIn('release_candidate.py fetch', release)
        self.assertIn('release_candidate.py stage', release)
        self.assertIn('release_candidate.py publish', release)
        self.assertLess(release.index('release_candidate.py stage'), release.index(secret))
        self.assertNotIn('action-gh-release', release)
        # The owning Python publisher now constructs the typed draft and checks
        # the actual authenticated actor at EVERY write, not just a YAML guard.
        owner = (ROOT / 'scripts/release_candidate.py').read_text()
        self.assertIn("draft=True, prerelease=False, make_latest='false', generate_release_notes=False", owner)
        with tempfile.TemporaryDirectory() as directory:
            gh = Path(directory) / "gh"
            gh.write_text('#!/usr/bin/env python3\nimport os,sys,json\n'
                          'if sys.argv[2]=="user": print(json.dumps({"login":os.environ["FIXTURE_ACCOUNT"]}))\n'
                          'else:\n data=json.load(sys.stdin)\n assert data["draft"] is True and data["make_latest"]=="false"\n print(json.dumps(dict(data,id=71)))\n')
            gh.chmod(0o755)
            for account, token, expected in (("kissetfall", "fixture-only", 0), ("other", "fixture-only", 1), ("kissetfall", "", 1)):
                env = {**os.environ, "PATH": directory + os.pathsep + os.defpath,
                       "GH_TOKEN": token, "FIXTURE_ACCOUNT": account}
                from unittest.mock import patch
                with patch.dict(os.environ, env, clear=True):
                    if expected:
                        with self.assertRaises(ValueError):
                            release_candidate.GitHub(env).mutate('POST', 'repos/ergohaven/entropy/releases', payload={'draft':True, 'make_latest':'false'})
                    else:
                        result = release_candidate.GitHub(env).mutate('POST', 'repos/ergohaven/entropy/releases', payload={'draft':True, 'make_latest':'false'})
                        self.assertEqual(result['id'], 71)


if __name__ == "__main__":
    unittest.main()
