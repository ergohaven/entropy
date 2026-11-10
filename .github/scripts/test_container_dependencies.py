"""Exercise the Docker dependency owner with substituted APT/network boundaries."""
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class ContainerDependencies(unittest.TestCase):
    def run_dependency_layer(self, mode='recover'):
        dockerfile = (ROOT / 'Dockerfile').read_text()
        layers = re.findall(r'^RUN (.*?(?=^[A-Z]+ |\Z))', dockerfile, re.M | re.S)
        owner = next(layer for layer in layers if 'libgtk-3-dev' in layer)
        owner = owner.replace('/usr/local/lib/entropy/install_debian_dependencies.sh',
                              str(ROOT / 'scripts/install_debian_dependencies.sh'))
        with tempfile.TemporaryDirectory() as temp:
            temp = Path(temp)
            commands = temp / 'commands'
            commands.mkdir()
            archive = temp / 'archives'
            archive.mkdir()
            log = temp / 'calls.jsonl'
            stub = commands / 'stub'
            stub.write_text('''#!/usr/bin/env python3
import json, os, pathlib, sys
name=pathlib.Path(sys.argv[0]).name
args=sys.argv[1:]
with open(os.environ['CALL_LOG'], 'a') as f: f.write(json.dumps([name,args])+'\\n')
mode=os.environ['CASE']
if name=='apt-config':
 print("ARCHIVES='"+os.environ['ARCHIVES_DIR']+"/'")
elif name=='apt-get':
 if 'update' in args: sys.exit(100 if mode=='index-failure' else 0)
 if '--print-uris' in args:
  if mode=='plan-failure': sys.exit(100)
  uri='https://snapshot.debian.org/archive/debian/20260901T000000Z/pool/main/d/dconf/libdconf1_0.40.0-4_amd64.deb'
  if mode=='foreign-uri': uri='https://example.com/pool/main/evil.deb'
  if mode=='security': uri=uri.replace('/archive/debian/', '/archive/debian-security/')
  filename='../escape.deb' if mode=='unsafe-filename' else 'libdconf1_0.40.0-4_amd64.deb'
  digest='SHA256:'+('a'*64) if mode!='weak-hash' else 'MD5Sum:'+('a'*32)
  print("'%s' %s 40908 %s"%(uri,filename,digest))
 elif '--download-only' in args:
  sys.exit(0 if mode=='healthy' else 100)
 elif '--no-download' in args: sys.exit(0)
 else: sys.exit(100)
elif name=='apt-helper':
 sys.exit(100 if mode=='bad-checksum' else 0)
# rm is intentionally inert: never touch host APT lists in a layer fixture.
''')
            stub.chmod(0o755)
            for name in ('apt-get', 'apt-config', 'apt-helper', 'rm'):
                (commands / name).symlink_to(stub)
            env = {**os.environ, 'PATH': str(commands) + ':' + os.environ['PATH'],
                   'CALL_LOG': str(log), 'ARCHIVES_DIR': str(archive), 'CASE': mode}
            result = subprocess.run(['sh', '-c', owner], env=env, capture_output=True,
                                    text=True, timeout=15)
            calls = [json.loads(line) for line in log.read_text().splitlines()]
            return result, calls

    def test_snapshot_503_uses_identical_hash_checked_debian_pool_object(self):
        result, calls = self.run_dependency_layer()
        self.assertEqual(result.returncode, 0, result.stderr)
        fetches = [args for name, args in calls if name == 'apt-helper']
        self.assertEqual(len(fetches), 1)
        self.assertIn('https://deb.debian.org/debian/pool/main/d/dconf/libdconf1_0.40.0-4_amd64.deb', fetches[0])
        self.assertIn('SHA256:' + 'a' * 64, fetches[0])
        installs = [args for name, args in calls if name == 'apt-get' and '--no-download' in args]
        self.assertEqual(len(installs), 1)
        self.assertIn('libgtk-3-dev', installs[0])


    def test_healthy_snapshot_does_not_contact_the_pool(self):
        result, calls = self.run_dependency_layer('healthy')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(any(name == 'apt-helper' for name, _ in calls))

    def test_failed_metadata_plan_or_hash_never_reaches_install(self):
        for mode in ('index-failure', 'plan-failure', 'bad-checksum', 'weak-hash', 'foreign-uri', 'unsafe-filename'):
            with self.subTest(mode=mode):
                result, calls = self.run_dependency_layer(mode)
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(any(name == 'apt-get' and '--no-download' in args
                                     for name, args in calls))

    def test_security_snapshot_uses_only_the_debian_security_pool(self):
        result, calls = self.run_dependency_layer('security')
        self.assertEqual(result.returncode, 0, result.stderr)
        fetches = [args for name, args in calls if name == 'apt-helper']
        self.assertEqual(len(fetches), 1)
        self.assertIn('https://deb.debian.org/debian-security/pool/main/d/dconf/libdconf1_0.40.0-4_amd64.deb', fetches[0])

    def test_base_snapshot_and_authentication_policy_remain_pinned(self):
        dockerfile = (ROOT / 'Dockerfile').read_text()
        self.assertIn('ARG DEBIAN_SNAPSHOT=20260901T000000Z', dockerfile)
        self.assertIn('rust:1.97-bookworm@sha256:705e294093973d7c10e83400393dce7b3611f8e03e55a80af7fff6d02ae1affb', dockerfile)
        installer = (ROOT / 'scripts/install_debian_dependencies.sh').read_text()
        self.assertNotIn('--allow-unauthenticated', installer)
        self.assertNotIn('trusted=yes', installer)
        self.assertIn('APT::Update::Error-Mode=any', installer)


if __name__ == '__main__':
    unittest.main()
