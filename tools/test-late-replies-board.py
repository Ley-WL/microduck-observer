"""Read-only UART timing probe. No register writes, enable, or position targets."""
import json,sys,time,subprocess,urllib.request
from pathlib import Path
sys.path.insert(0,'/home/radxa/microduck-observer/releases/20261002-servo-scan-recovery/debug-server')
from servos import ReadOnlyBus,ALL_IDS,decode

def kernel(): return Path('/proc/tty/driver/serial').read_text()

def feed(pending,data):
    pending.extend(data); frames=[];bad=0;discarded=0
    while len(pending)>=4:
        if pending[:2]!=b'\xff\xff' or not 2<=pending[3]<=64:
            del pending[0];discarded+=1;continue
        size=pending[3]+4
        if len(pending)<size:break
        frame=bytes(pending[:size])
        if sum(frame[2:])%256!=255:
            bad+=1;del pending[0];discarded+=1;continue
        del pending[:size];frames.append(frame)
    return frames,bad,discarded

out={'readOnly':True,'positionCommands':0,'registerWrites':0,'rounds':[]}
bus=None;stopped=False
try:
    snapshot=json.load(urllib.request.urlopen('http://127.0.0.1:8877/api/v1/snapshot',timeout=5))
    out['before']=snapshot
    assert snapshot['system']['data']['servoControl']['state'] not in ('policy','moving','preflight','disabling')
    assert all(row['online'] and row['torque']==0 for row in snapshot['joints']['data']['servos']), '需要全在线全卸力'
    subprocess.run(['systemctl','stop','microduck-observer'],check=True);stopped=True
    bus=ReadOnlyBus('/dev/ttyS2');bus.serial.timeout=.001
    rows=bus.read_feedback_many(ALL_IDS)
    assert len(rows)==15 and all(r['torque']==0 for r in rows.values()),'基线未确认全卸力'
    pending=bytearray()
    for repetition in range(1,4):
        for name,ids in [('all15',list(ALL_IDS)),('single32',[32]),('no32',[x for x in ALL_IDS if x!=32])]:
            run={'group':name,'round':repetition,'kernelBefore':kernel(),'scans':[]}
            out['rounds'].append(run)
            began=time.monotonic();due=began
            for n in range(500):
                if time.monotonic()-began>=10:break
                delay=due-time.monotonic()
                if delay>0:time.sleep(delay)
                body=bytes([254,len(ids)+4,0x82,40,31,*ids])
                tx=b'\xff\xff'+body+bytes([(~sum(body))&255])
                # Do not clear input between requests. Missing replies extend this
                # transaction, with no new request, so late bytes stay identifiable.
                bus.serial.write(tx);start=time.monotonic()
                scan={'sentMono':start,'txHex':tx.hex(),'chunks':[],'replies':{},'checksumErrors':0,'discardedBytes':0,'unexpectedFrames':[]}
                run['scans'].append(scan);raw=bytearray();at20=None
                while True:
                    elapsed=time.monotonic()-start
                    if at20 is None and elapsed>=.02:
                        at20=sorted(set(ids)-{int(x) for x,r in scan['replies'].items() if r['observedMs']<=20})
                        scan['missingAt20Ms']=at20;scan['boundaryObservedMs']=elapsed*1000
                    if elapsed>=.1 or (elapsed>=.02 and not at20):break
                    call=time.monotonic();chunk=bus.serial.read(max(1,min(bus.serial.in_waiting,1024)));now=time.monotonic()
                    if chunk:
                        raw.extend(chunk);scan['chunks'].append({'atMs':(now-start)*1000,'callMs':(now-call)*1000,'hex':chunk.hex()})
                        frames,bad,discarded=feed(pending,chunk)
                        scan['checksumErrors']+=bad;scan['discardedBytes']+=discarded
                        for frame in frames:
                            sid=frame[2]
                            if sid in ids and len(frame)==37 and str(sid) not in scan['replies']:
                                row=decode(frame[5:-1],frame[4]);row['observedMs']=(now-start)*1000
                                assert row['torque']==0 and row['fault']==0 and 4<=row['voltage']<=8.4,'有效反馈状态异常'
                                scan['replies'][str(sid)]=row
                            else:scan['unexpectedFrames'].append(frame.hex())
                    if at20 and len(scan['replies'])==len(ids):break
                scan['rxHex']=raw.hex();scan['rxBytes']=len(raw);scan['pendingHex']=pending.hex()
                scan['elapsedMs']=(time.monotonic()-start)*1000
                scan['missingFinal']=sorted(set(ids)-{int(x) for x in scan['replies']})
                if scan['missingFinal'] or scan['checksumErrors']:
                    run['stopReason']='incomplete-or-corrupt';break
                due=max(due+.02,time.monotonic())
            run['kernelAfter']=kernel()
            print(json.dumps({'group':name,'round':repetition,'scans':len(run['scans']),'last':{k:v for k,v in run['scans'][-1].items() if k in ('missingAt20Ms','missingFinal','elapsedMs','rxBytes','checksumErrors')}}),flush=True)
            # Release pending fragments only between separate trials, after waiting
            # out the previous transaction. Store discarded bytes, never use as data.
            time.sleep(.1)
            run['betweenTrialBytes']=bus.serial.read(bus.serial.in_waiting).hex()
            pending.clear()
    out['result']='completed'
except Exception as e:out['error']=str(e);out['result']='stopped'
finally:
    if bus:bus.close()
    Path('/home/radxa/late-replies-20261003.json').write_text(json.dumps(out))
    if stopped:subprocess.run(['systemctl','start','microduck-observer'],check=True)
print(json.dumps({'result':out['result'],'error':out.get('error')}),flush=True)
