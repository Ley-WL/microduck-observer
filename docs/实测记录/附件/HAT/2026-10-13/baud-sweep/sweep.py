"""Temporary locked baud experiment; only READ and baud WRITE, never motion.

All 15 online in latest snapshot. Service is stopped for exclusive UART.
Writes register 6 only while register 55 is confirmed 1: no EEPROM persistence.
All touched IDs are restored to original 1Mbps in finally; failed restoration
leaves the service stopped and requires a power cycle of the servos.
"""
import collections, fcntl, json, os, select, struct, subprocess, termios, time
import urllib.request
from pathlib import Path

IDS = list(range(10,15))+list(range(20,25))+list(range(30,35))
RATES = [1000000, 500000, 250000, 128000, 115200, 76800, 57600, 38400]
OUT = Path('/home/radxa/servo-baud-sweep-20261013')

def packet(sid, instruction, params):
    assert instruction == 2 or (instruction == 3 and len(params) == 2 and params[0] == 6)
    data = [sid, len(params)+2, instruction, *params]
    return bytes([255, 255, *data, (~sum(data)) & 255])

def parse(raw):
    frames, bad = [], 0
    pos = 0
    while pos + 4 <= len(raw):
        if raw[pos:pos+2] != b'\xff\xff':
            pos += 1
            continue
        length = raw[pos+3]
        if length < 2 or length > 80:
            pos += 1
            continue
        end = pos + length + 4
        if end > len(raw):
            pos += 1
            continue
        frame = raw[pos:end]
        if sum(frame[2:]) & 255 != 255:
            bad += 1
            pos += 1
            continue
        frames.append({'id':frame[2], 'error':frame[4], 'data':list(frame[5:-1]), 'hex':frame.hex()})
        pos = end
    return frames, bad

def ratesetting(fd, rate):
    # Linux asm-generic termios2: 4 u32, line u8, cc[19], ispeed/ospeed u32.
    buf = bytearray(44)
    fcntl.ioctl(fd, 0x802c542a, buf, True)
    vals = list(struct.unpack('=IIIIB19BII', buf))
    vals[:4] = [0, 0, termios.CS8 | termios.CLOCAL | termios.CREAD | 0x1000, 0]
    vals[5+termios.VMIN] = 0
    vals[5+termios.VTIME] = 0
    vals[-2:] = [rate, rate]
    fcntl.ioctl(fd, 0x402c542b, struct.pack('=IIIIB19BII', *vals))
    check = bytearray(44)
    fcntl.ioctl(fd, 0x802c542a, check, True)
    assert struct.unpack('=IIIIB19BII', check)[-2:] == (rate, rate)
    time.sleep(.012)
    termios.tcflush(fd, termios.TCIFLUSH)

def exchange(fd, tx, budget=.08):
    termios.tcflush(fd, termios.TCIFLUSH)
    started = time.monotonic()
    assert os.write(fd, tx) == len(tx)
    raw = bytearray()
    while time.monotonic() - started < budget:
        ready, _, _ = select.select([fd], [], [], .002)
        if ready:
            raw.extend(os.read(fd, 4096))
    frames, bad = parse(bytes(raw))
    return {'txHex':tx.hex(), 'rxHex':raw.hex(), 'frames':frames, 'badChecksum':bad,
            'elapsedMs':(time.monotonic()-started)*1000}

def readreg(fd, sid, address, length=1, attempts=12):
    for _ in range(attempts):
        result = exchange(fd, packet(sid, 2, [address, length]))
        for f in result['frames']:
            if f['id'] == sid and f['error'] == 0 and len(f['data']) == length:
                return f['data']
    raise RuntimeError(f'Cannot read id={sid} address={address}')

def serialcount():
    return Path('/proc/tty/driver/serial').read_text()

def save(report):
    (OUT/'progress.json').write_text(json.dumps(report, indent=2))

def switch(fd, sid, oldindex, newindex, report):
    ratesetting(fd, RATES[oldindex])
    assert readreg(fd, sid, 55) == [1], 'Persistent writes forbidden'
    assert readreg(fd, sid, 6) == [oldindex]
    report['touchedIds'] = sorted(set(report['touchedIds']+[sid]))
    save(report)
    tx = packet(sid, 3, [6, newindex])
    termios.tcflush(fd, termios.TCIFLUSH)
    assert os.write(fd, tx) == len(tx)
    termios.tcdrain(fd)
    time.sleep(.05)
    ratesetting(fd, RATES[newindex])
    assert readreg(fd, sid, 6) == [newindex], 'Baud did not switch or no valid reply'
    assert readreg(fd, sid, 55) == [1]
    report['switches'].append({'id':sid, 'from':RATES[oldindex], 'to':RATES[newindex], 'verified':True})
    save(report)

def restore_id(fd, sid, report):
    # Locate actual active baud before writing, no blind writes across rates.
    for index, rate in enumerate(RATES):
        ratesetting(fd, rate)
        try:
            current = readreg(fd, sid, 6, attempts=8)[0]
            assert readreg(fd, sid, 55, attempts=8) == [1]
            if current != 0:
                exchange(fd, packet(sid, 3, [6, 0]))
            ratesetting(fd, 1000000)
            assert readreg(fd, sid, 6, attempts=20) == [0]
            report['restoration'][str(sid)] = {'ok':True, 'foundActiveBaud':rate, 'foundRegister':current}
            return
        except Exception:
            continue
    report['restoration'][str(sid)] = {'ok':False}

def main():
    OUT.mkdir(exist_ok=True)
    assert not (OUT/'progress.json').exists(), 'Do not duplicate this experiment'
    snapshot = json.load(urllib.request.urlopen('http://127.0.0.1:8877/api/v1/snapshot',timeout=5))
    assert snapshot['system']['data']['servoControl']['state'] in ('idle','failed','disabled')
    (OUT/'before-snapshot.json').write_text(json.dumps(snapshot))
    report = {'hardwareMotion':False, 'persistentWrites':False, 'connectedIds':IDS,
              'rates':RATES, 'sampleHz':5, 'samplesPerRate':150, 'replyBudgetMs':150,
              'touchedIds':[], 'switches':[], 'stages':[], 'restoration':{}, 'registersBefore':{},
              'serialBefore':serialcount()}
    fd = None; original = None; stopped = False
    try:
        subprocess.run(['systemctl','stop','microduck-observer'],check=True); stopped = True
        owners = subprocess.run(['fuser','/dev/ttyS2'],capture_output=True,text=True)
        assert not owners.stdout.strip(), 'Other UART owner present'
        fd = os.open('/dev/ttyS2',os.O_RDWR|os.O_NOCTTY|os.O_NONBLOCK)
        original = bytearray(44);fcntl.ioctl(fd,0x802c542a,original,True)
        ratesetting(fd,1000000)
        for sid in IDS:
            config = readreg(fd,sid,5,35,attempts=20)
            ram = readreg(fd,sid,40,16,attempts=20)
            assert config[1] == 0 and ram[0] == 0 and ram[15] == 1, 'Require original baud, disabled torque, and lock=1'
            report['registersBefore'][str(sid)] = {'config5to39':config,'ram40to55':ram}
        save(report)
        # Pilot confirms immediate volatile change and verified recovery.
        switch(fd,30,0,1,report);switch(fd,30,1,0,report)
        current = 0
        for index, rate in enumerate(RATES):
            if index != current:
                for sid in IDS:switch(fd,sid,current,index,report)
            current = index; ratesetting(fd,rate)
            stage = {'baud':rate,'index':index,'serialBefore':serialcount(),'complete':0,'zeroRx':0,
                     'validNone':0,'badChecksum':0,'missingById':dict.fromkeys(map(str,IDS),0),'samples':0}
            started = time.monotonic()
            with (OUT/f'frames-{rate}.jsonl').open('w') as log:
                for sample in range(150):
                    time.sleep(max(0,started+sample*.2-time.monotonic()))
                    row=exchange(fd,sync_packet(),budget=.15)
                    valid = {f['id'] for f in row['frames'] if f['id'] in IDS and f['error']==0 and len(f['data'])==15}
                    stage['complete'] += valid == set(IDS)
                    stage['zeroRx'] += not row['rxHex']
                    stage['validNone'] += not valid
                    stage['badChecksum'] += row['badChecksum']
                    stage['samples'] += 1
                    for sid in set(IDS)-valid:stage['missingById'][str(sid)]+=1
                    row.update(sample=sample,mono=time.monotonic(),validIds=sorted(valid))
                    log.write(json.dumps(row)+'\n');log.flush()
            stage['seconds']=time.monotonic()-started;stage['serialAfter']=serialcount()
            stage['completePercent']=stage['complete']/stage['samples']*100
            report['stages'].append(stage);save(report)
            print(json.dumps({'completedBaud':rate,**stage}),flush=True)
    except Exception as exc:
        report['error']=repr(exc);print('EXPERIMENT STOPPED '+repr(exc),flush=True)
    finally:
        if fd is not None:
            for sid in report['touchedIds']:restore_id(fd,sid,report)
            if all(v['ok'] for v in report['restoration'].values()):
                ratesetting(fd,1000000)
                for sid in IDS:
                    try:
                        config=readreg(fd,sid,5,35,attempts=20)
                        ram=readreg(fd,sid,40,16,attempts=20)
                        report.setdefault('registersAfter',{})[str(sid)]={'config5to39':config,'ram40to55':ram}
                        assert config==report['registersBefore'][str(sid)]['config5to39']
                        assert ram==report['registersBefore'][str(sid)]['ram40to55']
                    except Exception as exc:report.setdefault('afterAuditErrors',[]).append({'id':sid,'error':repr(exc)})
            if original is not None:fcntl.ioctl(fd,0x402c542b,original)
            os.close(fd)
        report['serialAfter']=serialcount()
        safe=all(v['ok'] for v in report['restoration'].values())
        if stopped and safe:
            subprocess.run(['systemctl','start','microduck-observer'],check=True)
            report['serviceRestored']=True
        else:report['serviceRestored']=not stopped
        report['finished']=True;save(report)
        (OUT/'result.json').write_text(json.dumps(report,indent=2))
        print(json.dumps({'finished':True,'error':report.get('error'),'restoration':report['restoration'],'serviceRestored':report['serviceRestored']}),flush=True)

def sync_packet():
    data=[254,len(IDS)+4,130,56,15,*IDS]
    return bytes([255,255,*data,(~sum(data))&255])

if __name__=='__main__':main()
