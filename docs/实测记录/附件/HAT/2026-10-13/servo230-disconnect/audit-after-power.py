import json,hashlib,re
from pathlib import Path

root=Path(__file__).resolve().parent
summary=json.loads((root/'after-power-summary.json').read_text(encoding='utf-8'))
def serial2(text):
    line=next(line for line in text.splitlines() if line.startswith('2:'))
    return {field:int(re.search(r'\b'+field+r':(\d+)',line).group(1)) if re.search(r'\b'+field+r':(\d+)',line) else 0
            for field in ('tx','rx','fe','oe','brk','pe')}
before=serial2(summary['serialBefore']);after=serial2(summary['serialAfter'])
result=dict(**summary,uart2CounterDelta={field:after[field]-before[field] for field in before},
    evidenceSha256={name:hashlib.sha256((root/name).read_bytes()).hexdigest() for name in
        ('after-power-summary.json','after-power-start-v3.json','after-power-frames-v3.jsonl')},
    scope='30 seconds idle production telemetry subscription; no new UART opens or motor actions; not loaded-motion validation',
    previousIdleCompleteSamples=286,previousIdleSamples=903,rootCauseIsolated=False)
assert summary['completeSamples']==summary['uniqueJointSamples']==1500
assert summary['zeroRxSamples']==summary['checksumErrorsInSampledReads']==0
assert all(result['uart2CounterDelta'][key]==0 for key in ('fe','oe','brk','pe'))
(root/'after-power-final-audit.json').write_text(json.dumps(result,indent=2),encoding='utf-8')
print(json.dumps(dict(samples=1500,complete=1500,counterDelta=result['uart2CounterDelta'],hardwareMotion=False)))
