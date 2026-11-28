#!/usr/bin/env python3
"""Stage unchanged bytes from a qualified macOS app ZIP; never run its code.

The caller owns same-run artifact provenance/qualification. --source must be a
directory containing only entropy-vPUBLIC-macos-ARCH.app.zip (not the outer
GitHub artifact ZIP). No archive paths are extracted. Only the checked executable
is copied, at CARGO_TARGET_DIR/TARGET/release/entropy (default: ROOT/target).
Relative CARGO_TARGET_DIR values are relative to ROOT; symlink paths are refused.
The stdout JSON receipt binds source ZIP, member bytes, target and staged bytes.
This performs no build, signing, native signature verification or execution.
Requires macOS/POSIX Python 3.9+ and the standard library only.
"""
import argparse
from contextlib import contextmanager
import hashlib
import io
import json
import os
from pathlib import Path
import plistlib
import re
import stat
import struct
import subprocess
import sys
import unicodedata
from xml.parsers.expat import ExpatError
import zipfile
import zlib

BINARY = "Entropy.app/Contents/MacOS/entropy"
PLIST = "Entropy.app/Contents/Info.plist"
TARGETS = {
    "aarch64-apple-darwin": ("arm64", 0x0100000C),
    "x86_64-apple-darwin": ("x86_64", 0x01000007),
}
MAX_ARCHIVE_BYTES = 512 * 1024 * 1024
MAX_MEMBER_BYTES = 512 * 1024 * 1024
MAX_TOTAL_BYTES = 1024 * 1024 * 1024
MAX_PLIST_BYTES = 1024 * 1024
MAX_ENTRIES = 4096
CHUNK_BYTES = 64 * 1024


def absolute_path(path):
    path = Path(path)
    if ".." in path.parts:
        raise ValueError("parent traversal in staging path")
    return Path(os.path.abspath(str(path)))


@contextmanager
def directory_fd(path, create=False):
    """Walk with openat/O_NOFOLLOW: no symlink parents or check/open race."""
    path = absolute_path(path)
    flags = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW
    fd = os.open(path.anchor, flags)
    try:
        for component in path.parts[1:]:
            if create:
                try:
                    os.mkdir(component, mode=0o755, dir_fd=fd)
                except FileExistsError:
                    pass
            child = os.open(component, flags, dir_fd=fd)
            os.close(fd)
            fd = child
        yield fd
    finally:
        os.close(fd)


def read_source(source, name):
    try:
        with directory_fd(source) as parent:
            if set(os.listdir(parent)) != {name}:
                raise ValueError("source must contain exactly the expected app archive")
            fd = os.open(name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=parent)
            with os.fdopen(fd, "rb") as stream:
                info = os.fstat(stream.fileno())
                if not stat.S_ISREG(info.st_mode):
                    raise ValueError("source archive must be a regular file")
                if not 0 < info.st_size <= MAX_ARCHIVE_BYTES:
                    raise ValueError("source archive size limit exceeded")
                # One immutable bounded snapshot binds the hash to validated bytes.
                data = stream.read(MAX_ARCHIVE_BYTES + 1)
                if len(data) > MAX_ARCHIVE_BYTES or len(data) != info.st_size:
                    raise ValueError("source archive size limit or concurrent change")
        return data
    except OSError as error:
        raise ValueError("unsafe source directory or archive (symlinks forbidden)") from error


def write_destination(destination, binary):
    """Exclusively create the fixed filename and verify its bytes on readback."""
    expected = hashlib.sha256(binary).hexdigest()
    try:
        with directory_fd(destination.parent, create=True) as parent:
            fd = os.open(destination.name, os.O_RDWR | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW,
                         0o600, dir_fd=parent)
            created = os.fstat(fd)
            try:
                with os.fdopen(fd, "w+b") as stream:
                    stream.write(binary)
                    stream.flush()
                    stream.seek(0)
                    digest = hashlib.sha256()
                    for chunk in iter(lambda: stream.read(CHUNK_BYTES), b""):
                        digest.update(chunk)
                    actual = digest.hexdigest()
                    if actual != expected:
                        raise ValueError("staged destination checksum mismatch")
                    os.fchmod(stream.fileno(), 0o755)
                    os.fsync(stream.fileno())
                current = os.stat(destination.name, dir_fd=parent, follow_symlinks=False)
                if (current.st_dev, current.st_ino) != (created.st_dev, created.st_ino):
                    raise ValueError("staged destination was replaced")
            except BaseException:
                # Only remove the file this call created, never a pre-existing path.
                current = os.stat(destination.name, dir_fd=parent, follow_symlinks=False)
                if (current.st_dev, current.st_ino) == (created.st_dev, created.st_ino):
                    os.unlink(destination.name, dir_fd=parent)
                raise
        return actual
    except OSError as error:
        raise ValueError("unsafe or existing destination (symlinks forbidden)") from error


def validate_archive(bundle):
    """Validate every member, even discarded resources; never extract paths."""
    infos = bundle.infolist()
    if not 0 < len(infos) <= MAX_ENTRIES:
        raise ValueError("ZIP entry count limit exceeded")
    paths = {}
    total = 0
    for item in infos:
        name = item.orig_filename
        path = name[:-1] if name.endswith("/") else name
        if (not path or len(name) > 1024 or "\\" in name or ":" in name
                or any(ord(c) < 32 or ord(c) == 127 for c in name)
                or any(part in ("", ".", "..") for part in path.split("/"))):
            raise ValueError("unsafe ZIP member path")
        canonical = unicodedata.normalize("NFD", path).casefold()
        if canonical in paths:
            raise ValueError("duplicate or aliased ZIP member")
        mode = item.external_attr >> 16
        if item.create_system != 3 or (not stat.S_ISREG(mode) and not stat.S_ISDIR(mode)):
            raise ValueError("ZIP members must be regular files or directories")
        if item.is_dir() != stat.S_ISDIR(mode) or (item.is_dir() and item.file_size):
            raise ValueError("inconsistent ZIP directory entry")
        if item.flag_bits & 1 or item.compress_type not in (zipfile.ZIP_STORED, zipfile.ZIP_DEFLATED):
            raise ValueError("encrypted or unsupported ZIP member")
        if not 0 <= item.file_size <= MAX_MEMBER_BYTES:
            raise ValueError("ZIP member size limit exceeded")
        if item.filename == PLIST and item.file_size > MAX_PLIST_BYTES:
            raise ValueError("bundle plist size limit exceeded")
        total += item.file_size
        if total > MAX_TOTAL_BYTES:
            raise ValueError("ZIP total size limit exceeded")
        paths[canonical] = item
    for path in paths:
        components = path.split("/")
        for end in range(1, len(components)):
            ancestor = paths.get("/".join(components[:end]))
            if ancestor is not None and not ancestor.is_dir():
                raise ValueError("ZIP file blocks a directory path")
    names = {item.filename for item in infos}
    if not {PLIST, BINARY} <= names:
        raise ValueError("bundle plist or executable is missing")
    contents = {}
    for item in infos:
        chunks = [] if item.filename in (PLIST, BINARY) else None
        size = 0
        with bundle.open(item) as stream:
            while True:
                chunk = stream.read(CHUNK_BYTES)
                if not chunk:
                    break
                size += len(chunk)
                if size > item.file_size or size > MAX_MEMBER_BYTES:
                    raise ValueError("ZIP actual member size limit exceeded")
                if chunks is not None:
                    chunks.append(chunk)
        if size != item.file_size:
            raise ValueError("ZIP member size mismatch")
        if chunks is not None:
            contents[item.filename] = b"".join(chunks)
    return contents


def validate_bundle(bundle, contents, target, version):
    try:
        info = plistlib.loads(contents[PLIST])
    except (ValueError, TypeError, OverflowError, ExpatError, plistlib.InvalidFileException) as error:
        raise ValueError("invalid bundle plist") from error
    expected = {"CFBundleIdentifier": "com.ergohaven.entropy",
                "CFBundleExecutable": "entropy",
                "CFBundleShortVersionString": version, "CFBundleVersion": version}
    if not isinstance(info, dict) or any(info.get(k) != v for k, v in expected.items()):
        raise ValueError("bundle plist identity or public version mismatch")
    if not (bundle.getinfo(BINARY).external_attr >> 16) & 0o111:
        raise ValueError("bundle binary lacks executable mode")
    binary = contents[BINARY]
    if len(binary) < 32:
        raise ValueError("truncated Mach-O executable")
    magic, cpu, _, filetype, ncmds, sizeofcmds, _, _ = struct.unpack("<8I", binary[:32])
    if (magic != 0xFEEDFACF or cpu != TARGETS[target][1] or filetype != 2
            or 32 + sizeofcmds > len(binary) or ncmds > sizeofcmds // 8):
        raise ValueError("expected thin little-endian Mach-O executable for target")
    return binary


def public_version(root):
    """Use the repository's version owner, never VERSION or an archive label."""
    try:
        result = subprocess.run(["/bin/sh", str(root / "scripts/public_version.sh")],
                                cwd=str(root), check=True, capture_output=True, text=True,
                                env={"PATH": "/usr/bin:/bin", "LC_ALL": "C"}, timeout=30)
    except (OSError, subprocess.SubprocessError) as error:
        raise ValueError("public version helper failed") from error
    version = result.stdout.strip()
    if not re.fullmatch(r"[0-9]+\.[0-9]+(?:\.[0-9]+)?", version):
        raise ValueError("invalid public version from version helper")
    return version


def stage(source, target, *, root=None, version=None, target_dir=None):
    """Stage one target. Keyword overrides exist only for isolated unit tests.

    The CLI never accepts or derives root/version overrides from environment.
    """
    if target not in TARGETS:
        raise ValueError("unsupported macOS target")
    root = absolute_path(root) if root is not None else Path(__file__).resolve().parents[1]
    source = absolute_path(source)
    version = public_version(root) if version is None else version
    if not isinstance(version, str) or not re.fullmatch(r"[0-9]+\.[0-9]+(?:\.[0-9]+)?", version):
        raise ValueError("invalid public version")
    archive = source / ("entropy-v%s-macos-%s.app.zip" % (version, TARGETS[target][0]))
    source_bytes = read_source(source, archive.name)
    try:
        with zipfile.ZipFile(io.BytesIO(source_bytes)) as bundle:
            contents = validate_archive(bundle)
            binary = validate_bundle(bundle, contents, target, version)
    except (zipfile.BadZipFile, RuntimeError, NotImplementedError, zlib.error) as error:
        raise ValueError("invalid or corrupt bundle ZIP") from error
    target_base = Path(target_dir if target_dir is not None else os.environ.get("CARGO_TARGET_DIR") or "target")
    if not target_base.is_absolute():
        target_base = root / target_base
    destination = absolute_path(target_base) / target / "release/entropy"
    staged_sha256 = write_destination(destination, binary)
    return {
        "source": str(archive), "source_sha256": hashlib.sha256(source_bytes).hexdigest(),
        "member": BINARY, "target": target, "version": version,
        "destination": str(destination), "size": len(binary),
        "binary_sha256": hashlib.sha256(binary).hexdigest(),
        "staged_sha256": staged_sha256,
    }


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, allow_abbrev=False)
    parser.add_argument("--source", required=True, type=Path)
    parser.add_argument("--target", required=True, choices=sorted(TARGETS))
    args = parser.parse_args(argv)
    try:
        receipt = stage(args.source, args.target)
    except (ValueError, OSError) as error:
        print("macOS staging failed: %s" % error, file=sys.stderr)
        return 1
    print(json.dumps(receipt, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())
