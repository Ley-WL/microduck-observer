"""Offline integration: pack real signed bundles, verify with the native Rust helper.

Run on Linux/WSL after cargo build --bin microduck-observer-ota-helper.
No systemd, network, root, or robot operations.
"""
import base64
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import padding, rsa

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('package_update', ROOT / 'tools/package-web-update.py')
packer = importlib.util.module_from_spec(spec); spec.loader.exec_module(packer)
HELPER = ROOT / 'backend-rust/target/debug/microduck-observer-ota-helper'


class BundleIntegration(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(); self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.key = rsa.generate_private_key(public_exponent=65537, key_size=2048)
        self.private = self.root / 'key.pem'; self.public = self.root / 'public.pem'
        self.private.write_bytes(self.key.private_bytes(serialization.Encoding.PEM, serialization.PrivateFormat.PKCS8, serialization.NoEncryption()))
        self.public.write_bytes(self.key.public_key().public_bytes(serialization.Encoding.PEM, serialization.PublicFormat.SubjectPublicKeyInfo))
        self.frontend = self.root / 'web'; self.frontend.mkdir(); (self.frontend / 'index.html').write_text('<html>test</html>')
        self.models = self.root / 'models'; self.models.mkdir()
        (self.models / 'walk.onnx').write_bytes(b'fixture-model')
        (self.models / 'walk.metadata.json').write_text('{}')
        self.binary = self.root / 'microduck-observer'
        elf = bytearray(128); elf[:6] = b'\x7fELF\x02\x01'; elf[18:20] = (183).to_bytes(2, 'little')
        self.binary.write_bytes(elf + b'/api/v1/update')

    def bundle(self, components):
        output = self.root / 'update.tar.gz'
        packer.package(self.binary, self.frontend, components, 'test-1', 1, self.private, output, self.models)
        return output

    def verify(self, output, success):
        result = subprocess.run([str(HELPER), 'bundle-verify', str(output), str(self.public)], capture_output=True, text=True, timeout=15)
        self.assertEqual(result.returncode == 0, success, result.stdout + result.stderr)
        return json.loads(result.stdout)

    def rewrite(self, output, change_manifest=None, mutation=None):
        with tarfile.open(output) as archive:
            entries = [(m, archive.extractfile(m).read()) for m in archive.getmembers()]
        if change_manifest:
            envelope = json.loads(entries[0][1]); manifest = json.loads(base64.b64decode(envelope['payload']))
            change_manifest(manifest)
            raw = json.dumps(manifest, separators=(',', ':')).encode()
            entries[0] = (entries[0][0], json.dumps({'payload': base64.b64encode(raw).decode(), 'signature': base64.b64encode(self.key.sign(raw, padding.PKCS1v15(), hashes.SHA256())).decode()}).encode())
        if mutation:
            mutation(entries)
        with tarfile.open(output, 'w:gz', format=tarfile.USTAR_FORMAT) as archive:
            for info, data in entries:
                info.size = len(data); archive.addfile(info, io.BytesIO(data))

    def test_all_component_modes(self):
        for components in [['frontend'], ['backend'], ['models'], ['frontend', 'backend'], ['frontend', 'backend', 'models']]:
            with self.subTest(components=components):
                self.assertEqual(self.verify(self.bundle(components), True)['components'], components)

    def test_models_require_pairs_and_explicit_component(self):
        output = self.bundle(['models'])
        def remove_pair(m):
            del m['files']['models/walk.metadata.json']
        self.rewrite(output, change_manifest=remove_pair); self.verify(output, False)
        output = self.bundle(['models'])
        self.rewrite(output, change_manifest=lambda m: m.update(components=['frontend']))
        self.verify(output, False)
        (self.models / 'walk.metadata.json').unlink()
        with self.assertRaises(ValueError):
            self.bundle(['models'])

    def test_model_registry_and_settings_paths_rejected(self):
        for path in ['models/registry.json', 'models/../calibration.json', 'state/model-registry.json']:
            output = self.bundle(['models'])
            self.rewrite(output, change_manifest=lambda m: m['files'].__setitem__(path, {'size': 1, 'sha256': 'a' * 64}))
            self.verify(output, False)

    def test_tampered_payload_rejected(self):
        output = self.bundle(['frontend'])
        self.rewrite(output, mutation=lambda entries: entries.__setitem__(1, (entries[1][0], b'x' * len(entries[1][1]))))
        self.verify(output, False)

    def test_wrong_signature_rejected(self):
        output = self.bundle(['frontend'])
        def mutate(entries):
            e = json.loads(entries[0][1]); e['signature'] = base64.b64encode(b'fake').decode()
            entries[0] = (entries[0][0], json.dumps(e).encode())
        self.rewrite(output, mutation=mutate); self.verify(output, False)

    def test_duplicate_and_missing_files_rejected(self):
        for mutation in [lambda e: e.append(e[-1]), lambda e: e.pop()]:
            output = self.bundle(['frontend']); self.rewrite(output, mutation=mutation); self.verify(output, False)

    def test_traversal_links_and_wrong_arch_rejected(self):
        output = self.bundle(['frontend'])
        self.rewrite(output, mutation=lambda e: setattr(e[1][0], 'name', 'payload/../../escape')); self.verify(output, False)
        output = self.bundle(['frontend'])
        self.rewrite(output, mutation=lambda e: setattr(e[1][0], 'type', tarfile.SYMTYPE)); self.verify(output, False)
        output = self.bundle(['frontend'])
        self.rewrite(output, change_manifest=lambda m: m.update(architecture='x86_64')); self.verify(output, False)
        self.assertFalse((self.root / 'escape').exists())

    def test_truncation_and_appended_archive_rejected(self):
        output = self.bundle(['frontend']); output.write_bytes(output.read_bytes()[:-12]); self.verify(output, False)
        output = self.bundle(['frontend']); output.write_bytes(output.read_bytes() * 2); self.verify(output, False)

    def test_oversize_signed_manifest_rejected(self):
        output = self.bundle(['frontend'])
        self.rewrite(output, change_manifest=lambda m: m['files']['frontend/dist/index.html'].update(size=129 * 1024 * 1024))
        self.verify(output, False)


if __name__ == '__main__':
    unittest.main()
