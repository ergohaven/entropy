#!/usr/bin/env python3
"""Launch the exact release AppImage under an isolated Xvfb/dbus session."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import struct
import subprocess
import tempfile
import time


def verify(artifact, version, output, *, timeout=45):
    image = Path(artifact).resolve(strict=True)
    output = Path(output)
    if not re.fullmatch(r'[0-9]+\.[0-9]+(?:\.[0-9]+)?', version):
        raise ValueError('invalid public version')
    output.mkdir(parents=True, exist_ok=False)
    extraction = output / 'extracted'
    extraction.mkdir()
    with (output / 'extraction.log').open('w') as log:
        subprocess.run([str(image), '--appimage-extract'], cwd=extraction,
                       stdout=log, stderr=subprocess.STDOUT, check=True, timeout=90)
    appdir = extraction / 'squashfs-root'
    app, binary = appdir / 'AppRun', appdir / 'usr/bin/entropy'
    desktop = appdir / 'entropy.desktop'
    if not os.access(app, os.X_OK) or not os.access(binary, os.X_OK):
        raise ValueError('AppRun or executable is not executable')
    header = binary.read_bytes()[:64]
    if len(header) < 64 or header[:6] != b'\x7fELF\x02\x01' or struct.unpack_from('<H', header, 18)[0] != 62:
        raise ValueError('expected ELF64 x86_64')
    if re.findall(r'^X-AppImage-Version=(.+)$', desktop.read_text(), re.M) != [version]:
        raise ValueError('AppImage version mismatch')
    subprocess.run(['desktop-file-validate', str(desktop)], check=True, capture_output=True)
    receipt = dict(appimage_sha256=hashlib.sha256(image.read_bytes()).hexdigest(),
                   executable_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
                   version=version, architecture='x86_64', hardware_test=False)
    with tempfile.TemporaryDirectory(prefix='entropy-release-smoke-', dir=os.environ.get('TMPDIR')) as private:
        env = {key: value for key, value in os.environ.items()
               if not any(word in key.upper() for word in ('TOKEN', 'PASSWORD', 'SECRET'))}
        env['LIBGL_ALWAYS_SOFTWARE'] = '1'
        for key, suffix in [('HOME', 'home'), ('XDG_CONFIG_HOME', 'config'), ('XDG_DATA_HOME', 'data'),
                            ('XDG_CACHE_HOME', 'cache'), ('XDG_STATE_HOME', 'state'),
                            ('XDG_RUNTIME_DIR', 'runtime')]:
            path = Path(private) / suffix
            path.mkdir(mode=0o700)
            env[key] = str(path)
        with (output / 'launch.log').open('w') as log:
            process = subprocess.Popen([str(app)], cwd=appdir, env=env, stdout=log,
                                       stderr=subprocess.STDOUT, start_new_session=True)
            try:
                deadline = time.monotonic() + timeout
                while time.monotonic() < deadline:
                    if process.poll() is not None:
                        raise RuntimeError('AppRun exited before showing its window')
                    tree = subprocess.check_output(['xwininfo', '-root', '-tree'], env=env, text=True, timeout=10)
                    if re.search(r'"Entropy(?: \(v[^\"]+\))?"', tree):
                        break
                    time.sleep(0.5)
                else:
                    raise TimeoutError('Entropy window did not appear')
                (output / 'window-tree.txt').write_text(tree)
                time.sleep(2)
                if process.poll() is not None:
                    raise RuntimeError('Entropy exited immediately after startup')
                from PIL import ImageGrab
                ImageGrab.grab(xdisplay=env['DISPLAY']).save(output / 'startup.png')
                receipt.update(window_alive=True, screenshot_captured=True)
            finally:
                if process.poll() is None:
                    os.killpg(process.pid, signal.SIGTERM)
                    try:
                        process.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        os.killpg(process.pid, signal.SIGKILL)
                        process.wait(timeout=5)
    (output / 'smoke.json').write_text(json.dumps(receipt, indent=2) + '\n')
    return receipt


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--artifact', required=True, type=Path)
    parser.add_argument('--version', required=True)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    print(json.dumps(verify(args.artifact, args.version, args.output), indent=2))
