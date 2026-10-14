from pathlib import Path
import json,base64,hashlib,urllib.request
from cryptography.hazmat.primitives import serialization,hashes
from cryptography.hazmat.primitives.asymmetric import padding
root=Path(__file__).resolve().parents[6]
e=json.load(urllib.request.urlopen('http://8.133.210.70:6661/microduck/observer/latest.json',timeout=20));raw=base64.b64decode(e['payload']);key=serialization.load_pem_public_key((root/'deploy/ota-public.pem').read_bytes());key.verify(base64.b64decode(e['signature']),raw,padding.PKCS1v15(),hashes.SHA256());m=json.loads(raw);b=urllib.request.urlopen(m['url'],timeout=30).read();assert m['versionCode']==2026101303 and len(b)==m['size'] and hashlib.sha256(b).hexdigest()==m['sha256'];assert b==(root/'backend-rust/target/aarch64-unknown-linux-gnu/release/microduck-observer').read_bytes();m.update(publicSignatureVerified=True,fullBinaryBytesVerified=True);Path(__file__).with_name('feed-verification.json').write_text(json.dumps(m,indent=2));print('Signed Observer feed full bytes verified')
