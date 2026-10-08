from pathlib import Path
import subprocess
root=Path(__file__).resolve().parents[1]
opts=['-o','BatchMode=yes','-o','HostKeyAlias=192.168.31.193'];host='radxa@192.168.31.186'
for source,target in [(root/'backend-rust/target/aarch64-unknown-linux-gnu/release/examples/policy_reply_budget_check','policy-reply-budget-check'),(root/'tools/policy-reply-budget-board.py','policy-reply-budget-board.py')]:
 subprocess.run(['scp',*opts,str(source),f'{host}:/home/radxa/{target}'],check=True)
pw=next(x.removeprefix('Password: ') for x in Path('D:/Users/wl/Downloads/MicroDuck-Flash/board-login-20260928.txt').read_text().splitlines() if x.startswith('Password: '))
r=subprocess.run(['ssh',*opts,host,"chmod +x /home/radxa/policy-reply-budget-check && sudo -S -p '' /home/radxa/microduck-observer/venv/bin/python /home/radxa/policy-reply-budget-board.py"],input=pw+'\n',capture_output=True,text=True,encoding='utf-8')
print(r.stdout,r.stderr)
p=root/'docs/实测记录/附件/HAT/2026-10-05/xgoduck-schedule';p.mkdir(parents=True,exist_ok=True)
for n in ['policy-reply-budget.json','policy-reply-budget-manifest.json']:
 subprocess.run(['scp',*opts,f'{host}:/home/radxa/{n}',str(p/n.replace('.json','-no-drain.json'))],check=True)
r.check_returncode()
