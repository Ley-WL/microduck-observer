"""Independent static 50Hz sync-read comparison; reuse locked baud recovery."""
import importlib.util, json, os, select, subprocess, termios, time, fcntl, urllib.request
from pathlib import Path
spec=importlib.util.spec_from_file_location('sweep','/home/radxa/servo-baud-sweep-20261013.py')
b=importlib.util.module_from_spec(spec);spec.loader.exec_module(b)
b.OUT=Path('/home/radxa/servo-baud-load50-20261013')

def readcycle(fd):
    termios.tcflush(fd,termios.TCIFLUSH)
    started=time.monotonic();tx=b.sync_packet();assert os.write(fd,tx)==len(tx)
    raw=bytearray();frames=[];bad=0;valid=set()
    while time.monotonic()-started < .0185:
        ready,_,_=select.select([fd],[],[],.0005)
        if ready:
            raw.extend(os.read(fd,4096));frames,bad=b.parse(bytes(raw))
            valid={f['id'] for f in frames if f['id'] in b.IDS and f['error']==0 and len(f['data'])==15}
            if valid==set(b.IDS):break
    return {'txHex':tx.hex(),'rxHex':raw.hex(),'frames':frames,'badChecksum':bad,
            'validIds':sorted(valid),'readMs':(time.monotonic()-started)*1000,'mono':started}

def main():
    b.OUT.mkdir(exist_ok=True);assert not (b.OUT/'progress.json').exists()
    snapshot=json.load(urllib.request.urlopen('http://127.0.0.1:8877/api/v1/snapshot',timeout=5))
    assert snapshot['system']['data']['servoControl']['state'] in ('idle','failed','disabled')
    (b.OUT/'before-snapshot.json').write_text(json.dumps(snapshot))
    r={'hardwareMotion':False,'persistentWrites':False,'connectedIds':b.IDS,'sampleHz':50,
       'samplesPerStage':1500,'replyBudgetMs':18.5,'touchedIds':[],'switches':[],
       'stages':[],'restoration':{},'registersBefore':{}}
    fd=None;old=None;stopped=False
    try:
        subprocess.run(['systemctl','stop','microduck-observer'],check=True);stopped=True
        assert not subprocess.run(['fuser','/dev/ttyS2'],capture_output=True,text=True).stdout.strip()
        fd=os.open('/dev/ttyS2',os.O_RDWR|os.O_NOCTTY|os.O_NONBLOCK)
        old=bytearray(44);fcntl.ioctl(fd,0x802c542a,old,True);b.ratesetting(fd,1000000)
        for sid in b.IDS:
            cfg=b.readreg(fd,sid,5,35,20);ram=b.readreg(fd,sid,40,16,20)
            assert cfg[1]==0 and ram[0]==0 and ram[15]==1
            r['registersBefore'][str(sid)]={'config5to39':cfg,'ram40to55':ram}
        b.save(r);current=0
        for stage_index,index in enumerate([0,1,2,0]):
            if index!=current:
                for sid in b.IDS:b.switch(fd,sid,current,index,r)
            current=index;b.ratesetting(fd,b.RATES[index])
            stage={'baud':b.RATES[index],'serialBefore':b.serialcount(),'complete':0,'badChecksum':0,
                   'zeroRx':0,'validNone':0,'samples':0,'missingById':dict.fromkeys(map(str,b.IDS),0)}
            started=time.monotonic();rows=[]
            with (b.OUT/f'frames-{stage_index}-{stage["baud"]}.jsonl').open('w') as log:
                for sample in range(1500):
                    time.sleep(max(0,started+sample*.02-time.monotonic()))
                    row=readcycle(fd);valid=set(row['validIds']);row['sample']=sample
                    stage['complete']+=valid==set(b.IDS);stage['badChecksum']+=row['badChecksum']
                    stage['zeroRx']+=not row['rxHex'];stage['validNone']+=not valid;stage['samples']+=1
                    for sid in set(b.IDS)-valid:stage['missingById'][str(sid)]+=1
                    rows.append(row);log.write(json.dumps(row)+'\n');log.flush()
            stage['seconds']=time.monotonic()-started;stage['observedHz']=len(rows)/stage['seconds']
            durations=sorted(v['readMs'] for v in rows)
            stage['readMsMean']=sum(durations)/len(durations);stage['readMsP95']=durations[int(.95*(len(durations)-1))]
            stage['maxIntervalMs']=max((rows[i]['mono']-rows[i-1]['mono'])*1000 for i in range(1,len(rows)))
            stage['serialAfter']=b.serialcount();r['stages'].append(stage);b.save(r)
            print(json.dumps(stage),flush=True)
    except Exception as e:r['error']=repr(e);print('STOPPED '+repr(e),flush=True)
    finally:
        if fd is not None:
            for sid in r['touchedIds']:b.restore_id(fd,sid,r)
            if all(v['ok'] for v in r['restoration'].values()):
                b.ratesetting(fd,1000000)
                for sid in b.IDS:
                    try:
                        cfg=b.readreg(fd,sid,5,35,20);ram=b.readreg(fd,sid,40,16,20)
                        r.setdefault('registersAfter',{})[str(sid)]={'config5to39':cfg,'ram40to55':ram}
                        assert cfg==r['registersBefore'][str(sid)]['config5to39'] and ram==r['registersBefore'][str(sid)]['ram40to55']
                    except Exception as e:r.setdefault('afterAuditErrors',[]).append({'id':sid,'error':repr(e)})
            if old is not None:fcntl.ioctl(fd,0x402c542b,old)
            os.close(fd)
        safe=all(v['ok'] for v in r['restoration'].values())
        if stopped and safe:subprocess.run(['systemctl','start','microduck-observer'],check=True);r['serviceRestored']=True
        else:r['serviceRestored']=not stopped
        r['finished']=True;b.save(r);(b.OUT/'result.json').write_text(json.dumps(r,indent=2))
        print(json.dumps({'finished':True,'error':r.get('error'),'serviceRestored':r['serviceRestored'],'restoration':r['restoration']}),flush=True)

if __name__=='__main__':main()
