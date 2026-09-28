#!/usr/bin/env python3
"""Release publication regressions: real signatures/parser/rename, fake builds.

Run with python3 -B packaging/test_update_release.py. Test keys and artifacts
exist only in a TemporaryDirectory, outside the repository. Cargo stays offline.
Release history comes from a local server answering as GitHub would, through
the updater's own test-build endpoint; nothing reaches the network.
"""
import copy
import http.server
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import threading
import unittest

REPO = Path(__file__).resolve().parent.parent


def run(*args, **kwargs):
    return subprocess.run(args, check=True, stdout=subprocess.PIPE,
                          stderr=subprocess.STDOUT, **kwargs).stdout


def script(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text('#!/bin/sh\nset -eu\n' + text)
    path.chmod(0o755)


class Releases(http.server.BaseHTTPRequestHandler):
    """The official repository's releases, as the test sets them."""
    routes = {}

    def do_GET(self):
        body = self.routes.get(self.path)
        if body is None:
            self.send_response(404)
            body = b''
        else:
            self.send_response(200)
        self.send_header('Content-Length', str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *args):
        pass


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
        run('cargo', 'build', '--offline', '--manifest-path', str(p), '--features', 'test-endpoint',
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
        # The builder clears LCL_UPDATE_TEST_ENDPOINT; the fake build's
        # verifier names the local release server itself.
        script(cls.bin / 'cargo', '''
while [ "$1" != --target-dir ]; do shift; done
mkdir -p "$2/debug"
printf '#!/bin/sh\\nLCL_UPDATE_TEST_ENDPOINT=%s exec %s "$@"\\n' "$TEST_ENDPOINT" "$TEST_VERIFIER" > "$2/debug/lcl-update"
chmod +x "$2/debug/lcl-update"
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
        cls.template['source_commit'] = run('git', '-C', str(cls.root), 'rev-parse', 'HEAD').decode().strip()
        cls.server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Releases)
        threading.Thread(target=cls.server.serve_forever, daemon=True).start()
        cls.addClassCleanup(cls.server.server_close)
        cls.addClassCleanup(cls.server.shutdown)
        cls.endpoint = 'http://127.0.0.1:%d' % cls.server.server_address[1]

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
                        LCL_RELEASE_NOTES_FILE=str(self.notes), TEST_ENDPOINT=self.endpoint)
        self.env.pop('LCL_UPDATE_DRY_RUN', None)
        self.env.pop('LCL_ANDROID_SIGNER_SHA256', None)
        self.sign(self.template)
        self.publish()

    def sign(self, manifest):
        self.manifest.write_text(json.dumps(manifest) if isinstance(manifest, dict) else manifest)
        run('openssl', 'dgst', '-sha256', '-sign', str(self.key), '-out',
            str(self.signature), str(self.manifest))

    def publish(self, tag=None, manifest=None, signature=None, draft=False, prerelease=False):
        """Make the official repository's latest release the one supplied (by default)."""
        manifest = self.manifest.read_bytes() if manifest is None else manifest
        signature = self.signature.read_bytes() if signature is None else signature
        tag = tag or json.loads(manifest)['release_tag']
        pc = json.loads(manifest)['pc']
        assets = [{'name': 'update-manifest.json', 'size': len(manifest)},
                  {'name': 'update-manifest.sig', 'size': len(signature)},
                  {'name': pc['artifact_name'], 'size': pc['size']}]
        Releases.routes = {
            '/api/releases/latest': json.dumps({'tag_name': tag, 'draft': draft,
                                                'prerelease': prerelease, 'assets': assets}).encode(),
            f'/download/{tag}/update-manifest.json': manifest,
            f'/download/{tag}/update-manifest.sig': signature,
        }

    def bootstrap(self):
        self.env['LCL_UPDATE_DRY_RUN'] = '1'
        self.env['LCL_PREVIOUS_MANIFEST'] = 'none'
        self.env.pop('LCL_PREVIOUS_MANIFEST_SIGNATURE')
        self.env['LCL_ANDROID_SIGNER_SHA256'] = self.env['TEST_SIGNER']

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
        self.assertIn('previous release:  v0.2.0 (manifest sha256',
                      (self.out / 'lcl-0.3.0-UPDATE-PROVENANCE.txt').read_text())

    def test_bootstrap_only_while_no_stable_release_exists(self):
        # No release, or only a draft or a pre-release: the first release.
        for state in ('none', 'draft', 'prerelease'):
            with self.subTest(state=state):
                self.setUp()
                self.bootstrap()
                if state == 'none': Releases.routes = {}
                else: self.publish(draft=state == 'draft', prerelease=state == 'prerelease')
                self.build(success=True)
                self.assertIn('previous release:  none',
                              (self.out / 'lcl-0.3.0-UPDATE-PROVENANCE.txt').read_text())
        # A stable release exists, or its state cannot be read: refused.
        unreadable = 'could not be read and verified'
        for state, why in (('published', 'only for the first update release'),
                           ('unavailable', unreadable), ('unverifiable', unreadable)):
            with self.subTest(state=state):
                self.setUp()
                self.bootstrap()
                if state == 'unavailable': self.env['TEST_ENDPOINT'] = 'http://127.0.0.1:1'
                if state == 'unverifiable': self.publish(signature=b'wrong')
                self.assertIn(why, self.build())

    def test_history_must_be_the_latest_stable_release(self):
        older = copy.deepcopy(self.template)
        older.update(product_version='0.1.5', release_tag='v0.1.5')
        older['pc']['artifact_name'] = 'lcl-0.1.5-linux-x86_64.tar.gz'
        older['android'].update(version_name='0.1.5', version_code=3,
                                artifact_name='lcl-android-0.1.5-3.apk')
        not_latest = 'not the official repository\'s latest stable update release'
        unreadable = 'could not be read and verified'
        for defect, why in (('older', not_latest), ('nothing-published', not_latest),
                            ('draft', not_latest), ('prerelease', not_latest),
                            ('wrong-tag', 'carries the manifest of'),
                            ('published-signature', unreadable),
                            ('other-repository', "not in this checkout's history"),
                            ('unavailable', unreadable)):
            with self.subTest(defect=defect):
                self.setUp()
                if defect == 'older':
                    # The newer v0.2.0 is published; an older signed manifest is supplied.
                    self.sign(older)
                if defect == 'nothing-published': Releases.routes = {}
                if defect in ('draft', 'prerelease'):
                    self.publish(draft=defect == 'draft', prerelease=defect == 'prerelease')
                if defect == 'wrong-tag': self.publish(tag='v0.2.1')
                if defect == 'published-signature': self.publish(signature=b'wrong')
                if defect == 'other-repository':
                    foreign = copy.deepcopy(self.template)
                    foreign['source_commit'] = 'f' * 40
                    self.sign(foreign)
                    self.publish()
                if defect == 'unavailable': self.env['TEST_ENDPOINT'] = 'http://127.0.0.1:1'
                self.assertIn(why, self.build())

    def test_more_than_one_production_key_is_refused(self):
        keys = self.root / 'update/trusted_keys.txt'
        listed = keys.read_text()
        self.addCleanup(keys.write_text, listed)
        other = run('openssl', 'ecparam', '-name', 'prime256v1', '-genkey', '-noout')
        public = run('openssl', 'pkey', '-pubout', '-outform', 'DER', input=other)
        keys.write_text(listed + 'lcl-update-2 ' + public.hex() + '\n')
        self.env['LCL_UPDATE_DRY_RUN'] = '1'
        self.assertIn('2 production update keys', self.build())

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
