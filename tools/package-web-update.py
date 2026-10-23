"""Build a signed, portable Observer .tar.gz for the System Update page.

Only the signing machine needs the private key. Never put it inside a bundle.
"""
import argparse
import base64
import hashlib
import io
import json
import re
import tarfile
from datetime import datetime, timezone
from pathlib import Path
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import padding

ROOT = Path(__file__).resolve().parents[1]


def package(binary, frontend, components, version, code, key_path, output, models=None):
    if not re.fullmatch(r'[a-zA-Z0-9._-]{1,64}', version) or code <= 0:
        raise ValueError('Invalid release name or version code')
    if not components or len(set(components)) != len(components) or set(components) - {'backend', 'frontend', 'models'}:
        raise ValueError('Choose backend, frontend and/or models')
    files = {}
    if 'backend' in components:
        for name in ['microduck-observer', 'microduck-tof']:
            source = binary if name == 'microduck-observer' else binary.with_name(name)
            if not source.exists() and name != 'microduck-observer':
                continue
            data = source.read_bytes()
            if len(data) < 20 or data[:6] != b'\x7fELF\x02\x01' or int.from_bytes(data[18:20], 'little') != 183:
                raise ValueError(f'{name}: build ARM64 first')
            if name == 'microduck-observer' and b'/api/v1/update' not in data:
                raise ValueError('Backend does not contain the web-update API; rebuild current source')
            files['bin/' + name] = source
        for folder, names in [('tof', ['LICENSE.txt', 'APACHE-2.0.txt', 'UPSTREAM.md']), ('microduck-sounds', ['LICENSE', 'UPSTREAM.md'])]:
            for name in names:
                files[f'licenses/{folder}/{name}'] = ROOT / 'backend-rust/vendor' / folder / name
    if 'frontend' in components:
        if not (frontend / 'index.html').is_file():
            raise ValueError('Build frontend/dist first')
        for source in sorted(frontend.rglob('*')):
            if source.is_symlink():
                raise ValueError('Symlinks are forbidden')
            if source.is_file():
                files['frontend/dist/' + source.relative_to(frontend).as_posix()] = source
    if 'models' in components:
        if models is None or not models.is_dir() or models.is_symlink():
            raise ValueError('Provide --models with ONNX/metadata pairs')
        selected = sorted(models.glob('*.onnx'))
        if not selected:
            raise ValueError('No ONNX models found')
        for source in selected:
            metadata = source.with_suffix('.metadata.json')
            if not metadata.is_file() or not isinstance(json.loads(metadata.read_text(encoding='utf-8')), dict):
                raise ValueError(f'Missing or invalid metadata: {source.name}')
            files['models/' + source.name] = source
            files['models/' + metadata.name] = metadata
    if len(files) > 2048:
        raise ValueError('Too many files')
    metadata = {}
    for name, source in files.items():
        if source.is_symlink() or len('payload/' + name) > 100 or not re.fullmatch(r'[a-zA-Z0-9/._-]+', name):
            raise ValueError(f'Unsupported archive path: {name}')
        size = source.stat().st_size
        if not 0 < size <= 100 * 1024 * 1024:
            raise ValueError(f'Invalid file size: {name}')
        with source.open('rb') as stream:
            digest = hashlib.file_digest(stream, 'sha256').hexdigest()
        metadata[name] = {'size': size, 'sha256': digest}
    if sum(v['size'] for v in metadata.values()) > 128 * 1024 * 1024:
        raise ValueError('Uncompressed bundle exceeds 128 MiB')
    manifest = dict(schema='microduck-observer-bundle/v1', architecture='aarch64', updaterProtocol=1,
                    version=version, versionCode=code, components=components, files=metadata)
    raw = json.dumps(manifest, ensure_ascii=False, separators=(',', ':')).encode()
    key = serialization.load_pem_private_key(key_path.read_bytes(), password=None)
    signature = key.sign(raw, padding.PKCS1v15(), hashes.SHA256())
    key.public_key().verify(signature, raw, padding.PKCS1v15(), hashes.SHA256())
    envelope = json.dumps({'payload': base64.b64encode(raw).decode(), 'signature': base64.b64encode(signature).decode()}).encode()
    if len(envelope) > 512 * 1024:
        raise ValueError('Manifest exceeds size limit')
    output.parent.mkdir(parents=True, exist_ok=True)
    temp = output.with_name(output.name + '.tmp')
    try:
        with tarfile.open(temp, 'w:gz', format=tarfile.USTAR_FORMAT) as archive:
            info = tarfile.TarInfo('manifest.json'); info.size = len(envelope)
            archive.addfile(info, io.BytesIO(envelope))
            for name, source in files.items():
                info = tarfile.TarInfo('payload/' + name); info.size = metadata[name]['size']; info.mode = 0o644
                with source.open('rb') as stream:
                    archive.addfile(info, stream)
        if temp.stat().st_size > 128 * 1024 * 1024:
            raise ValueError('Compressed bundle exceeds size limit')
        temp.replace(output)
    finally:
        temp.unlink(missing_ok=True)
    return {'output': str(output), 'version': version, 'versionCode': code, 'components': components, 'files': len(files)}


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--components', nargs='+', choices=['backend', 'frontend', 'models'], default=['backend', 'frontend'])
    p.add_argument('--binary', type=Path, default=ROOT / 'backend-rust/target/aarch64-unknown-linux-gnu/release/microduck-observer')
    p.add_argument('--frontend', type=Path, default=ROOT / 'frontend/dist')
    p.add_argument('--models', type=Path, help='Directory of ONNX/metadata pairs; only used with --components models')
    p.add_argument('--version', required=True)
    p.add_argument('--code', type=int, default=int(datetime.now(timezone.utc).timestamp()))
    p.add_argument('--key', type=Path, default=Path.home() / '.microduck/release-manifest-key.pem')
    p.add_argument('--output', required=True, type=Path)
    a = p.parse_args()
    print(json.dumps(package(a.binary, a.frontend, a.components, a.version, a.code, a.key, a.output, a.models), ensure_ascii=False))
