"""Assertion-level metadata rejection before any GUI or hardware interaction."""
from pathlib import Path
import importlib.util
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('appimage_verifier', ROOT / 'scripts/verify_linux_appimage.py')
assert spec and spec.loader
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class AppImageMetadataRejection(unittest.TestCase):
    def extractor(self, root, machine=62):
        image = root / 'metadata-fixture.AppImage'
        image.write_text('''#!/usr/bin/env python3
from pathlib import Path
import struct
p=Path('squashfs-root');(p/'usr/bin').mkdir(parents=True)
header=bytearray(64);header[:6]=b'\\x7fELF\\x02\\x01';struct.pack_into('<H',header,18,%d)
(p/'usr/bin/entropy').write_bytes(header);(p/'usr/bin/entropy').chmod(0o755)
(p/'AppRun').write_text('#!/bin/sh\\nexit 1\\n');(p/'AppRun').chmod(0o755)
(p/'entropy.desktop').write_text('X-AppImage-Version=0.7\\n')
''' % machine)
        image.chmod(0o755)
        return image

    def test_wrong_embedded_version_is_rejected_before_gui_launch(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with self.assertRaisesRegex(ValueError, 'AppImage version mismatch'):
                module.verify(self.extractor(root), '0.8', root / 'output')
            self.assertFalse((root / 'output/launch.log').exists())

    def test_wrong_elf_architecture_is_rejected_before_gui_launch(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with self.assertRaisesRegex(ValueError, 'expected ELF64 x86_64'):
                module.verify(self.extractor(root, machine=183), '0.7', root / 'output')
            self.assertFalse((root / 'output/launch.log').exists())

    def test_existing_evidence_directory_is_never_overwritten(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            output = root / 'output'
            output.mkdir()
            sentinel = output / 'original'
            sentinel.write_bytes(b'keep')
            with self.assertRaises(FileExistsError):
                module.verify(self.extractor(root), '0.7', output)
            self.assertEqual(sentinel.read_bytes(), b'keep')


if __name__ == '__main__':
    unittest.main()
