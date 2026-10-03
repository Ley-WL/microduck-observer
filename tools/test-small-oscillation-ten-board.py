"""Explicitly authorized ten trials; each trial has its own unload and service recovery."""
import json
from pathlib import Path
import shutil
import subprocess
import time

results=[]
for n in range(1,11):
    subprocess.run(['/home/radxa/microduck-observer/venv/bin/python','/home/radxa/test-small-oscillation-board.py'],check=True)
    source=Path('/home/radxa/small-oscillation-20261003.json')
    target=Path(f'/home/radxa/small-oscillation-ten-{n}-20261003.json')
    shutil.copy2(source,target)
    data=json.loads(source.read_text())
    results.append({'round':n,'result':data['result'],'error':data.get('error'),'timing':data['timing'],'unload':data.get('unload')})
    print(json.dumps(results[-1]),flush=True)
    if data.get('unload',{}).get('unconfirmedIds') or data.get('unload',{}).get('state')!='disabled':
        print('Unload not confirmed; no next trial',flush=True)
        break
    time.sleep(1)
Path('/home/radxa/small-oscillation-ten-summary.json').write_text(json.dumps(results))
