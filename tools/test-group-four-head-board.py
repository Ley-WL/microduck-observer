"""User-authorized 10 second, 50Hz, +/-1 degree all-14 motion probe. Run as root."""
import json, math, subprocess, sys, time, urllib.request
from pathlib import Path
sys.path.insert(0,'/home/radxa/microduck-observer/releases/20261002-servo-scan-recovery/debug-server')
from servos import ReadOnlyBus, ALL_IDS
from servo_control import sync_write, goal_bytes, torque_off, clear_legacy_profile, OFFICIAL_LIMITS_DEG
from pose_calibration import read_register

ids=[30, 31, 32, 33]; bus=None; stopped=False
out={'mode':'four-head-sine-1Hz-amplitude1degree-50Hz-10seconds','samples':[],'commands':[]}
def kernel(): return Path('/proc/tty/driver/serial').read_text()
def checked(rows):
    assert set(rows)==set(ALL_IDS), '反馈缺失'+str(sorted(set(ALL_IDS)-set(rows)))
    assert bus.last_read['checksumErrors']==0, '回包校验错误'
    for sid,row in rows.items():
        assert row['fault']==0 and 4<=row['voltage']<=8.4, '真实故障或工作电压越界'+str(sid)
try:
    before=json.load(urllib.request.urlopen('http://127.0.0.1:8877/api/v1/snapshot',timeout=5))
    assert before['system']['data']['servoControl']['state'] not in ('policy','moving','preflight','disabling')
    out['before']=before; out['kernelBefore']=kernel()
    cal=json.loads(Path('/var/lib/microduck-observer/calibration.json').read_text());out['calibration']=cal
    subprocess.run(['systemctl','stop','microduck-observer'],check=True);stopped=True
    bus=ReadOnlyBus('/dev/ttyS2')
    rows=bus.read_feedback_many(ALL_IDS);checked(rows);out['baseline']=rows
    starts={sid:rows[sid]['position'] for sid in ids}
    directions={sid:cal['joints']['directions'].get(str(sid),-1) for sid in ids}
    for sid in ids:
        cfg=read_register(bus,sid,0,40)
        assert list(cfg[:2])==[3,46] and cfg[33]==4,'固件/模式异常'
        low,high=int.from_bytes(cfg[9:11],'little'),int.from_bytes(cfg[11:13],'little')
        reference=cal['joints']['references'][str(sid)]
        angle=(starts[sid]-reference)*360/4096/directions[sid]
        angle=(angle+180)%360-180
        amin,amax=OFFICIAL_LIMITS_DEG[sid]
        assert amin<=angle-1 and angle+1<=amax,'往返目标超关节角度范围'+str(sid)
        assert high<=low or low<=starts[sid]-11<=starts[sid]+11<=high,'往返超硬件限位'
        clear_legacy_profile(bus,sid,rows[sid])
    off=[sid for sid in ids if rows[sid]['torque']!=1]
    if off:
        sync_write(bus,42,{sid:goal_bytes(starts[sid]) for sid in off})
        sync_write(bus,40,{sid:b'\x01' for sid in off})
    rows=bus.read_feedback_many(ALL_IDS);checked(rows)
    assert all(rows[sid]['torque']==1 for sid in ids),'使能未确认'
    out['enabled']=rows
    start=time.monotonic()
    for n in range(500):
        due=start+n*.02
        delay=due-time.monotonic()
        if delay>0: time.sleep(delay)
        offset=round(math.sin(2*math.pi*n/50)*4096/360)
        targets={sid:starts[sid]+offset*directions[sid] for sid in ids}
        command_at=time.monotonic()
        if targets: sync_write(bus,42,{sid:goal_bytes(value) for sid,value in targets.items()})
        out['commands'].append({'mono':command_at,'targets':targets})
        rows=bus.read_feedback_many(ALL_IDS)
        out['samples'].append({'feedback':rows,'diagnostics':dict(bus.last_read),'trace':list(bus.trace)})
        checked(rows)
        assert all(rows[sid]['torque']==1 for sid in ids),'运动中使能丢失'
    if ids: sync_write(bus,42,{sid:goal_bytes(starts[sid]) for sid in ids})
    time.sleep(.15)
    rows=bus.read_feedback_many(ALL_IDS);checked(rows);out['return']=rows
    out['result']='completed'
except Exception as e:
    out['error']=str(e);out['result']='stopped'
finally:
    if bus:
        out['lastRead']=bus.last_read;out['lastTrace']=list(bus.trace)
        try: out['unload']=torque_off(bus)
        except Exception as e: out['unloadError']=str(e)
        bus.close()
    out['kernelAfter']=kernel()
    stamps=[r['mono'] for r in out['commands']]
    gaps=[b-a for a,b in zip(stamps,stamps[1:])]
    out['timing']={'commandCount':len(stamps),'observedHz':(len(stamps)-1)/(stamps[-1]-stamps[0]) if len(stamps)>1 else None,'maxGapMs':max(gaps)*1000 if gaps else None}
    Path('/home/radxa/group-probe-four-head-20261003.json').write_text(json.dumps(out))
    if stopped: subprocess.run(['systemctl','start','microduck-observer'],check=True)
print(json.dumps({k:out[k] for k in ('result','error','timing','unload','unloadError') if k in out}))
