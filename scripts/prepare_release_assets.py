#!/usr/bin/env python3
"""Collect the exact public release inventory and its SHA-256 checksums."""
import argparse
import hashlib
from pathlib import Path
import re
import shutil


def collect(source: Path, destination: Path, tag: str) -> list[str]:
    if not re.fullmatch(r"v0\.(?:0|[1-9][0-9]*)(?:\.(?:0|[1-9][0-9]*))?(?:-[0-9A-Za-z.-]+)?", tag):
        raise ValueError("unsupported release tag")
    suffixes = [
        "x86_64.AppImage", "amd64.deb", "x86_64.rpm", "x86_64.pkg.tar.zst",
        "windows-x86_64.exe", "macos-arm64.dmg", "macos-x86_64.dmg",
    ]
    expected = {f"entropy-{tag}-{suffix}" for suffix in suffixes}
    inputs = {}
    for path in source.rglob("*"):
        if path.is_symlink():
            raise ValueError(f"symlink in release inputs: {path}")
        if path.is_dir():
            continue
        if not path.is_file() or path.name in inputs or path.stat().st_size == 0:
            raise ValueError(f"invalid, empty or duplicate release input: {path}")
        inputs[path.name] = path
    if set(inputs) != expected:
        raise ValueError(f"release inventory mismatch: missing={sorted(expected - set(inputs))}, unexpected={sorted(set(inputs) - expected)}")
    if destination.exists():
        raise ValueError("release output directory already exists")
    destination.mkdir(parents=True)
    checksums = []
    for name in sorted(expected):
        with inputs[name].open("rb") as stream:
            digest = hashlib.file_digest(stream, "sha256").hexdigest()
        shutil.copyfile(inputs[name], destination / name)
        with (destination / name).open("rb") as stream:
            if hashlib.file_digest(stream, "sha256").hexdigest() != digest:
                raise ValueError(f"release copy checksum mismatch: {name}")
        checksums.append(f"{digest}  {name}\n")
    (destination / "SHA256SUMS.txt").write_text("".join(checksums), encoding="utf-8")
    return sorted(expected | {"SHA256SUMS.txt"})


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", required=True, type=Path)
    parser.add_argument("--destination", required=True, type=Path)
    parser.add_argument("--tag", required=True)
    arguments = parser.parse_args()
    for name in collect(arguments.source, arguments.destination, arguments.tag):
        print(name)
