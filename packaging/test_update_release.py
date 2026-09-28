#!/usr/bin/env python3
"""Release publication regressions: real signatures/parser/rename, fake builds.

Run with python3 -B packaging/test_update_release.py. Test keys and artifacts
exist only in a TemporaryDirectory, outside the repository. Cargo stays offline.
"""
import copy
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

REPO = Path(__file__).resolve().parent.parent


def run(*args, **kwargs):
    return subprocess.run(args, check=True, stdout=subprocess.PIPE,
                          stderr=subprocess.STDOUT, **kwargs).stdout


def script(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text('#!/bin/sh\nset -eu\n' + text)
    path.chmod(0o755)


class ReleaseTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp = tempfile.TemporaryDirectory(prefix='lcl-release-test-')
        cls.addClassCleanup(cls.temp.cleanup)
        cls.base = Path(cls.temp.name)
        cls.root = cls.base / 'source'
        cls.root.mkdir()
        # Reuse production verification code, with a throwaway trusted PUBLIC key.
        shutil.copytree(REPO / 'update', cls.root / 'update',
                        ignore=shutil.ignore_patterns('target'))
        shutil.copytree(REPO / 'impl/crates/lcl-spec', cls.root / 'impl/crates/lcl-spec')
        (cls.root / 'impl/Cargo.toml').write_text('''[workspace]
members = ["crates/lcl-spec"]
resolver = "2"
[workspace.package]
version = "0.3.0"
edition = "2021"
rust-version = "1.89"
publish = false
''')
        (cls.root / 'remote').mkdir()
        (cls.root / 'remote/Cargo.toml').write_text('version = "0.3.0"\n')
        (cls.root / '.gitignore').write_text('/android/app/build/\n')
        p = cls.root / 'update/Cargo.toml'
        p.write_text(p.read_text().replace('version = "0.1.0"', 'version = "0.3.0"'))
        cls.key = cls.base / 'test.key'
        run('openssl', 'ecparam', '-name', 'prime256v1', '-genkey', '-noout', '-out', str(cls.key))
        public = run('openssl', 'pkey', '-in', str(cls.key), '-pubout', '-outform', 'DER')
        (cls.root / 'update/trusted_keys.txt').write_text('lcl-update-1 ' + public.hex() + '\n')
        # Updating only the test snapshot's package version updates its lockfile;
        # all external crate versions still come from the copied production lock.
        run('cargo', 'build', '--offline', '--manifest-path', str(p),
            '--target-dir', str(cls.base / 'target'), '--bin', 'lcl-update', '-j', '2')
        cls.verifier = cls.base / 'target/debug/lcl-update'
        (cls.root / 'packaging').mkdir()
        shutil.copy(REPO / 'packaging/build_update_release.sh', cls.root / 'packaging')
        script(cls.root / 'packaging/build_release.sh', '''
mkdir -p "$LCL_RELEASE_OUT" "$TEST_BASE/payload/lcl-0.3.0-linux-x86_64/bin"
cp "$TEST_VERIFIER" "$TEST_BASE/payload/lcl-0.3.0-linux-x86_64/bin/lcl-update"
tar -czf "$LCL_RELEASE_OUT/lcl-0.3.0-linux-x86_64.tar.gz" -C "$TEST_BASE/payload" lcl-0.3.0-linux-x86_64
echo provenance > "$LCL_RELEASE_OUT/lcl-0.3.0-PROVENANCE.txt"
''')
        script(cls.root / 'android/gradlew', '''
[ "${TEST_FAIL:-}" != android ] || exit 31
mkdir -p app/build/outputs/apk/release
echo apk > app/build/outputs/apk/release/app-release.apk
''')
        cls.sdk = cls.base / 'sdk'
        script(cls.sdk / 'build-tools/1/apksigner', '''
[ "${TEST_FAIL:-}" != certificate ] || exit 32
printf 'Signer #1 certificate SHA-256 digest: %s\n' "$TEST_SIGNER"
''')
        script(cls.sdk / 'build-tools/1/aapt2', '''
[ "${TEST_FAIL:-}" != metadata ] || exit 33
echo "package: name='io.lcl.workspace' versionCode='5' versionName='0.3.0'"
echo "minSdkVersion:'29'"
''')
        cls.bin = cls.base / 'bin'
        script(cls.bin / 'cargo', '''
while [ "$1" != --target-dir ]; do shift; done
mkdir -p "$2/debug"
cp "$TEST_VERIFIER" "$2/debug/lcl-update"
''')
        script(cls.bin / 'openssl', '''
if [ "${TEST_FAIL:-}" = signing ] && [ "$1" = dgst ]; then exit 34; fi
exec /usr/bin/openssl "$@"
''')
        script(cls.bin / 'sha256sum', '''
if [ "${TEST_FAIL:-}" = checksum ] && [ "$1" = -- ]; then exit 35; fi
if [ "${TEST_FAIL:-}" = collision ] && [ "$1" = -c ]; then
    mkdir -p "$LCL_UPDATE_OUT"
    echo owner > "$LCL_UPDATE_OUT/owner"
fi
if [ "${TEST_FAIL:-}" = mismatch ] && [ "$1" = -c ]; then exit 36; fi
exec /usr/bin/sha256sum "$@"
''')
        run('git', 'init', '-q', str(cls.root))
        run('git', '-C', str(cls.root), 'add', '.')
        run('git', '-C', str(cls.root), '-c', 'user.name=Test', '-c',
            'user.email=test@example.invalid', 'commit', '-qm', 'fixture')
        cls.template = json.loads((REPO / 'update/manifest_vectors/valid.json').read_text())

    def setUp(self):
        self.case = Path(tempfile.mkdtemp(dir=self.base))
        self.out = self.case / 'release'
        self.manifest = self.case / 'previous.json'
        self.signature = self.case / 'previous.sig'
        self.notes = self.case / 'notes'
        self.notes.write_text('Repair test')
        self.env = dict(os.environ, PATH=str(self.bin) + os.pathsep + os.environ['PATH'],
                        TEST_BASE=str(self.case), TEST_VERIFIER=str(self.verifier),
                        TEST_SIGNER=self.template['android']['signer_sha256'],
                        ANDROID_HOME=str(self.sdk), LCL_UPDATE_OUT=str(self.out),
                        LCL_UPDATE_SIGNING_KEY=str(self.key), LCL_UPDATE_KEY_ID='lcl-update-1',
                        LCL_ANDROID_VERSION_CODE='5', LCL_PREVIOUS_MANIFEST=str(self.manifest),
                        LCL_PREVIOUS_MANIFEST_SIGNATURE=str(self.signature),
                        LCL_RELEASE_NOTES_FILE=str(self.notes))
        self.env.pop('LCL_UPDATE_DRY_RUN', None)
        self.env.pop('LCL_ANDROID_SIGNER_SHA256', None)
        self.sign(self.template)

    def sign(self, manifest):
        self.manifest.write_text(json.dumps(manifest) if isinstance(manifest, dict) else manifest)
        run('openssl', 'dgst', '-sha256', '-sign', str(self.key), '-out',
            str(self.signature), str(self.manifest))

    def build(self, success=False):
        result = subprocess.run([str(self.root / 'packaging/build_update_release.sh')],
                                env=self.env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        if success:
            self.assertEqual(result.returncode, 0, result.stdout.decode())
            run('sha256sum', '-c', 'SHA256SUMS', cwd=self.out)
        else:
            self.assertNotEqual(result.returncode, 0, result.stdout.decode())
            if self.env.get('TEST_FAIL') == 'collision':
                self.assertEqual([p.name for p in self.out.iterdir()], ['owner'])
                self.assertEqual((self.out / 'owner').read_text(), 'owner\n')
            else:
                self.assertFalse(self.out.exists(), result.stdout.decode())
        return result.stdout.decode()

    def test_authenticated_history_and_complete_publication(self):
        self.build(success=True)
        self.assertNotIn('SHA256SUMS', (self.out / 'SHA256SUMS').read_text())

    def test_bootstrap_is_explicit(self):
        self.env['LCL_UPDATE_DRY_RUN'] = '1'
        self.env['LCL_PREVIOUS_MANIFEST'] = 'none'
        self.env.pop('LCL_PREVIOUS_MANIFEST_SIGNATURE')
        self.env['LCL_ANDROID_SIGNER_SHA256'] = self.env['TEST_SIGNER']
        self.build(success=True)

    def test_history_refusals(self):
        for defect in ('modified', 'wrong-signature', 'unknown-key', 'wrong-product',
                       'wrong-signer', 'malformed', 'missing-signature', 'wrong-channel'):
            with self.subTest(defect=defect):
                self.setUp()
                previous = copy.deepcopy(self.template)
                if defect == 'wrong-product': previous['product'] = 'other'
                if defect == 'wrong-channel': previous['channel'] = 'preview'
                if defect == 'wrong-signer': previous['android']['signer_sha256'] = 'a' * 64
                self.sign('{' if defect == 'malformed' else previous)
                if defect == 'modified': self.manifest.write_bytes(self.manifest.read_bytes() + b' ')
                if defect == 'wrong-signature': self.signature.write_bytes(b'wrong')
                if defect == 'missing-signature': self.env.pop('LCL_PREVIOUS_MANIFEST_SIGNATURE')
                if defect == 'unknown-key':
                    other = self.case / 'unknown.key'
                    run('openssl', 'ecparam', '-name', 'prime256v1', '-genkey', '-noout', '-out', str(other))
                    run('openssl', 'dgst', '-sha256', '-sign', str(other), '-out', str(self.signature), str(self.manifest))
                self.build()

    def test_late_failures_publish_nothing(self):
        for failure in ('android', 'certificate', 'metadata', 'signing', 'checksum', 'mismatch', 'collision'):
            with self.subTest(failure=failure):
                self.setUp()
                self.env['TEST_FAIL'] = failure
                self.build()

    def test_invalid_new_manifest_is_refused(self):
        self.notes.write_text('x' * 100_000)
        self.build()


if __name__ == '__main__':
    unittest.main(verbosity=2)
