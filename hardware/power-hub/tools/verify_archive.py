"""Read-only archive integrity check; --git-index also checks staged Git blob bytes."""
from pathlib import Path
import argparse,csv,hashlib,json,subprocess,zipfile
args=argparse.ArgumentParser();args.add_argument('--git-index',action='store_true');opts=args.parse_args()
module=Path(__file__).resolve().parents[1];repo=module.parents[1];version=module/'revN-eco1'
manifest=json.loads((version/'manifest.json').read_text(encoding='utf-8'))
for name,digest in manifest['files'].items():
 f=version/name;assert hashlib.sha256(f.read_bytes()).hexdigest()==digest,('working file changed',name)
 if opts.git_index:
  data=subprocess.check_output(['git','show',':'+f.relative_to(repo).as_posix()],cwd=repo)
  assert hashlib.sha256(data).hexdigest()==digest,('Git changed artifact bytes',name)
for f in [version/'eda/MicroDuck-Hub-RevN-Cap-ECO1.epro',version/'manufacturing/MicroDuck-Hub-RevN-Gerber.zip',module/'packages/MicroDuck-Hub-RevN-ECO1-Complete.zip']:
 with zipfile.ZipFile(f) as z:assert z.testzip() is None
with zipfile.ZipFile(version/'manufacturing/MicroDuck-Hub-RevN-Gerber.zip') as z:assert len(z.namelist())==9
with (version/'bom/BOM.csv').open(encoding='utf-8-sig') as f:
 rows=list(csv.DictReader(f));assert len(rows)==2 and sum(int(r['每板数量']) for r in rows)==4
print('PASS:',len(manifest['files']),'artifact hashes,3 ZIPs,9 fabrication files,2 BOM rows/4 parts'+(', staged Git blobs preserve original bytes' if opts.git_index else ''))
