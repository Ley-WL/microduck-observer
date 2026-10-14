"""Sign binary-only Observer feed; optionally upload with pinned SSH host key.

Private signing key and cloud password are never included in the public feed.
"""
import argparse, base64, hashlib, json, re, sys, time
from pathlib import Path
from cryptography.hazmat.primitives import serialization, hashes
from cryptography.hazmat.primitives.asymmetric import padding
import paramiko

BASE='http://8.133.210.70:6661/microduck/observer/'
def prepare(binary, version, code, key_path, output):
    assert re.fullmatch(r'[a-zA-Z0-9.-]{1,31}', version) and code>0
    data=binary.read_bytes()
    assert data[:6]==b'\x7fELF\x02\x01' and int.from_bytes(data[18:20],'little')==183
    assert 100000<=len(data)<=100000000
    output.mkdir(parents=True,exist_ok=True)
    name=f'observer-{code}.bin'; (output/name).write_bytes(data)
    metadata=dict(schema='microduck-observer-ota/v1',architecture='aarch64',version=version,versionCode=code,url=BASE+name,size=len(data),sha256=hashlib.sha256(data).hexdigest())
    raw=json.dumps(metadata,separators=(',',':')).encode()
    key=serialization.load_pem_private_key(key_path.read_bytes(),password=None)
    signature=key.sign(raw,padding.PKCS1v15(),hashes.SHA256())
    key.public_key().verify(signature,raw,padding.PKCS1v15(),hashes.SHA256())
    (output/'latest.json').write_text(json.dumps(dict(payload=base64.b64encode(raw).decode(),signature=base64.b64encode(signature).decode())),encoding='utf-8')
    return metadata

def deploy(output, code, password):
    client=paramiko.SSHClient();client.load_host_keys(str(Path.home()/'.ssh/microduck_cloud_known_hosts'))
    client.connect('8.133.210.70',username='leiwu30',password=password,timeout=15,allow_agent=False,look_for_keys=False)
    stage=f'/home/leiwu30/.observer-ota-{time.time_ns()}'
    try:
        sftp=client.open_sftp();sftp.mkdir(stage,mode=0o700)
        for name in [f'observer-{code}.bin','latest.json']:sftp.put(str(output/name),stage+'/'+name)
        script='''import pathlib,os,shutil,sys,json,base64,hashlib
stage=pathlib.Path(sys.argv[1]);dest=pathlib.Path('/opt/microduck-downloads/public/observer');dest.mkdir(exist_ok=True)
env=json.loads((stage/'latest.json').read_text());m=json.loads(base64.b64decode(env['payload']));name='observer-'+str(m['versionCode'])+'.bin'
assert hashlib.sha256((stage/name).read_bytes()).hexdigest()==m['sha256']
prior=dest/'latest.json'
if prior.exists():
 old=json.loads(base64.b64decode(json.loads(prior.read_text())['payload']))
 assert m['versionCode']>old['versionCode'] or (m['versionCode']==old['versionCode'] and m['sha256']==old['sha256']), 'Version code must increase'
for name in [name,'latest.json']:
 temp=dest/(name+'.upload');shutil.copyfile(stage/name,temp);os.chmod(temp,0o644);os.replace(temp,dest/name)
print('Signed Observer feed installed')
'''
        with sftp.file(stage+'/install.py','w') as f:f.write(script)
        sftp.close()
        stdin,stdout,stderr=client.exec_command("sudo -S -p '' python3 "+stage+'/install.py '+stage,timeout=30)
        stdin.write(password+'\n');stdin.flush();stdin.channel.shutdown_write()
        print(stdout.read().decode());error=stderr.read().decode()
        if stdout.channel.recv_exit_status()!=0:raise RuntimeError(error)
        sftp=client.open_sftp()
        for name in [f'observer-{code}.bin','latest.json','install.py']:sftp.remove(stage+'/'+name)
        sftp.rmdir(stage);sftp.close()
    finally:client.close()

if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--binary',type=Path,required=True);p.add_argument('--version',required=True);p.add_argument('--code',type=int,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--key',type=Path,default=Path.home()/'.microduck/release-manifest-key.pem');p.add_argument('--deploy',action='store_true');a=p.parse_args()
    m=prepare(a.binary,a.version,a.code,a.key,a.output)
    if a.deploy:deploy(a.output,a.code,sys.stdin.readline().rstrip('\r\n'))
    print(json.dumps(m))
