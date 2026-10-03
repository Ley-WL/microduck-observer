"""Extract immutable raw replies from real failures for parser regression tests."""
import json
from pathlib import Path

root=Path(__file__).resolve().parents[1]
base=root/'docs/实测记录/附件/HAT/2026-10-03'
cases=[('group-probe-one-head-1.json',[12,13]),
       ('group-probe-one-leg-2.json',[30,31]),
       ('group-probe-readonly14-no32-3.json',[22,23]),
       ('group-probe-all14-3.json',[24])]
fixtures=[]
for name, missing in cases:
    data=json.loads((base/name).read_text())
    d=data['samples'][-1]['diagnostics']
    fixtures.append({'source':name,'rxHex':d['rxHex'],
                     'expectedIds':[sid for sid in d['requestedIds'] if sid not in missing]})
dest=root/'backend-rust/tests/fixtures/servo-resync.json'
dest.parent.mkdir(parents=True,exist_ok=True)
dest.write_text(json.dumps(fixtures,indent=2)+'\n',encoding='utf-8')
