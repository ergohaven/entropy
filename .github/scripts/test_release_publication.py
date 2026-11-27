"""Release publication contracts; all binary and account data are fixtures."""
import hashlib
import importlib.util
import os
from pathlib import Path
import subprocess
import tempfile
import textwrap
import unittest

ROOT = Path(__file__).resolve().parents[2]
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
        self.assertEqual(release.count(secret), 2)
        self.assertIn("needs: [build, validate, nix]", release)
        self.assertIn("draft: true", release)
        self.assertIn("make_latest: 'false'", release)
        self.assertIn("fail_on_unmatched_files: true", release)
        self.assertIn("files: dist/release/*", release)
        self.assertIn('prepare_release_assets.py --source artifacts --destination dist/release --tag "$GITHUB_REF_NAME"', release)
        self.assertLess(release.index("Verify release publisher"), release.index("Create GitHub Release"))
        guard = release.split("      - name: Verify release publisher\n", 1)[1].split("      - name:", 1)[0]
        shell = textwrap.dedent(guard.split("        run: |\n", 1)[1])
        with tempfile.TemporaryDirectory() as directory:
            gh = Path(directory) / "gh"
            gh.write_text('#!/bin/sh\nprintf "%s\\n" "$FIXTURE_ACCOUNT"\n')
            gh.chmod(0o755)
            for account, token, expected in (("kissetfall", "fixture-only", 0), ("other", "fixture-only", 1), ("kissetfall", "", 1)):
                env = {**os.environ, "PATH": directory + os.pathsep + os.defpath,
                       "GH_TOKEN": token, "FIXTURE_ACCOUNT": account}
                result = subprocess.run(["bash", "-e", "-c", shell], env=env, capture_output=True)
                self.assertEqual(result.returncode, expected, (account, bool(token)))


if __name__ == "__main__":
    unittest.main()
