"""One-shot, supported pose trial. Never modifies EEPROM or starts a policy.

Temporarily stops the existing servo collector (same Unix user), deliberately
borrows its UART while it is stopped, then resumes it. The read-only default
performs no motor writes. --move is an explicit, bounded motion operation.
"""
import argparse
import json
import math
import os
from pathlib import Path
import signal
import sys
import time

p = argparse.ArgumentParser()
p.add_argument('--move', action='store_true')
p.add_argument('--output', required=True)
p.add_argument('--duration', type=float, default=6)
p.add_argument('--fraction', type=float, default=.15,
               help='Fraction of current-to-STAND travel; full supported pose trial = 1')
p.add_argument('--policy', help='After supported pose, run 3s bounded leg-only ONNX closed loop')
p.add_argument('--keep-enabled',action='store_true',help='Keep verified static leg hold enabled after successful full pose')
p.add_argument('--full-body',action='store_true',help='Include four head/neck joints in the static pose')
p.add_argument('--rated-output',action='store_true',help='Use read-back EEPROM maximum output, never exceed it')
p.add_argument('--battery-floor-voltage', type=float, default=6.0,
               help='Conservative trial guard for the 2S supply, not the servo minimum voltage')
args = p.parse_args()
if not 3 <= args.duration <= 30: p.error('duration must be 3..30 seconds')
if not .01 <= args.fraction <= 1: p.error('fraction must be .01..1')
if args.fraction > .15 and args.duration < 20:
    p.error('Larger supported trials require at least 20 seconds')
if args.policy and (not args.move or args.fraction!=1):
    p.error('Policy phase requires a full supported pose transition')
if args.keep_enabled and (not args.move or args.fraction!=1 or args.policy):
    p.error('Static enabled hold requires full move and no policy phase')
if not 6.0 <= args.battery_floor_voltage <= 8.4:
    p.error('2S trial battery floor must be 6.0..8.4V')
SERVO_VOLTAGE_RANGE = (4.0, 8.4)
sys.path.insert(0, '/home/radxa/microduck-observer/current/debug-server')
from servos import ReadOnlyBus
from pose_calibration import read_register, write_register
import serial

PORT = '/dev/serial/by-id/usb-1a86_USB_Single_Serial_5B79076110-if00'
IDS = [10, 11, 12, 13, 14, 20, 21, 22, 23, 24]
ANGLES = dict(zip(IDS, [0, .0872664626, .457924, .004940, -.452984,
                        0, -.0872664626, -.457924, -.004940, .452984]))
if args.policy or args.full_body:
    IDS += [30,31,32,33]
    ANGLES.update({30:0.,31:0.,32:0.,33:0.})
result = {'mode': 'move' if args.move else 'inspect', 'status': 'preflight',
          'fraction': args.fraction, 'duration':args.duration, 'torqueLimitRaw': 'EEPROM maximum per joint' if args.rated_output else 200,
          'servoVoltageRange':SERVO_VOLTAGE_RANGE,
          'batteryTrialFloorVoltage':args.battery_floor_voltage, 'samples': [], 'cleanup': []}
out = Path(args.output)
out.parent.mkdir(parents=True, exist_ok=True)
def save():
    tmp = out.with_suffix('.tmp')
    with tmp.open('w') as f:
        json.dump(result, f); f.flush(); os.fsync(f.fileno())
    tmp.replace(out)

def goal_bytes(value):
    if not isinstance(value,int) or abs(value)>32767:
        raise ValueError('Goal exceeds verified signed-magnitude encoding')
    return (abs(value)|(0x8000 if value<0 else 0)).to_bytes(2,'little')
def interrupted(signum, frame):
    raise RuntimeError('Trial interrupted: ' + str(signum))
for sig in (signal.SIGTERM, signal.SIGINT, signal.SIGALRM):
    signal.signal(sig, interrupted)
bus = None; paused = None; attempted = False; before = {}; hold_verified=False
try:
    # Fail closed unless the only visible UART owner is the expected collector.
    actual = str(Path(PORT).resolve(strict=True)); owners = []
    for proc in Path('/proc').iterdir():
        if not proc.name.isdigit(): continue
        try:
            if any(os.readlink(fd) == actual for fd in (proc/'fd').iterdir()):
                owners.append(int(proc.name))
        except (PermissionError, FileNotFoundError, ProcessLookupError): pass
    if len(owners) != 1: raise ValueError('Expected exactly one serial owner: ' + str(owners))
    pid = owners[0]
    command = (Path('/proc')/str(pid)/'cmdline').read_bytes()
    if b'multiprocessing.spawn' not in command: raise ValueError('Unexpected serial owner')
    os.kill(pid, signal.SIGSTOP); paused = pid
    for _ in range(30):
        if '\nState:\tT' in (Path('/proc')/str(pid)/'status').read_text(): break
        time.sleep(.01)
    else: raise ValueError('Collector did not stop')
    signal.alarm(math.ceil(args.duration)+20)
    bus = ReadOnlyBus.__new__(ReadOnlyBus)
    # Existing collector retains an advisory lock but cannot execute bus IO.
    bus.serial = serial.Serial(PORT, 1000000, timeout=.025, write_timeout=.1, exclusive=False)
    cal = json.loads(Path('/var/lib/microduck-observer/calibration.json').read_text())
    result['calibrationRevision'] = cal['revision']; result['collectorPid'] = pid
    rows = []
    for sid in IDS:
        config = read_register(bus, sid, 0, 40)
        ram = read_register(bus, sid, 40, 16)
        feedback = bus.read_feedback(sid)
        before[sid] = {'config': list(config), 'ram': list(ram), 'feedback': feedback}
        maximum_output=int.from_bytes(config[16:18],'little')
        if not 0<maximum_output<=1000: raise ValueError(f'#{sid}: invalid EEPROM maximum output')
        ref = cal['joints']['references'][str(sid)]
        direction = cal['joints']['directions'].get(str(sid), -1)
        if direction not in (-1, 1): raise ValueError('Bad direction')
        target = round(ref + ANGLES[sid] * 4096 / (2*math.pi) * direction)
        start = feedback['position']
        # Keep the feedback's multi-turn branch. Do not drive nearly a full
        # revolution when a single-turn reference straddles encoder zero.
        branch=round((start-target)/4096)
        target += branch*4096
        partial = round(start + args.fraction*(target-start))
        low = int.from_bytes(config[9:11], 'little'); high = int.from_bytes(config[11:13], 'little')
        row = dict(id=sid, position=start, standing=target, partial=partial,
                   delta=partial-start, mode=config[33], limits=[low,high],
                   voltage=feedback['voltage'], firmware=list(config[:2]),encoderBranch=branch,
                   outputCap=maximum_output if args.rated_output else (100 if sid>=30 else 200))
        rows.append(row)
        result['before']=before; result['rows']=rows
        if list(config[:2]) != [3,46] or config[33] != 4: raise ValueError(f'#{sid}: firmware/mode not verified')
        enabled_ok=args.keep_enabled and feedback['torque']==1 and abs(feedback['target']-start)<=55
        if (feedback['torque'] and not enabled_ok) or feedback['fault'] or not args.battery_floor_voltage <= feedback['voltage'] <= SERVO_VOLTAGE_RANGE[1] or feedback['temperature'] > 50:
            raise ValueError(f'#{sid}: unsafe feedback')
        travel_limit = 160 if args.fraction <= .15 else 1024
        if abs(start)>32767 or abs(partial)>32767 or abs(partial-start)>travel_limit:
            raise ValueError(f'#{sid}: partial target out of envelope')
        if high>low and not low<=partial<=high: raise ValueError(f'#{sid}: EEPROM limits reject target')
    result['before'] = before; result['rows'] = rows; save()
    for sid in IDS:
        f = bus.read_feedback(sid)
        if abs(f['position'] - before[sid]['feedback']['position'])>8:
            raise ValueError(f'#{sid}: robot moved during preflight')
    if args.move:
        attempted = True
        # Match every stored target to the measured position BEFORE enabling torque.
        for row in rows:
            sid=row['id']
            cap=row['outputCap']
            write_register(bus,sid,48,cap.to_bytes(2,'little'))
            if read_register(bus,sid,48,2)!=cap.to_bytes(2,'little'):
                raise ValueError(f'#{sid}: torque limit write not confirmed')
            write_register(bus,sid,41,bytes([5])+goal_bytes(row['position'])+b'\x00\x00'+(100).to_bytes(2,'little'))
        for sid in IDS:
            write_register(bus,sid,40,b'\x01')
            if read_register(bus,sid,40,1)!=b'\x01': raise ValueError(f'#{sid}: torque enable not confirmed')
        result['status']='moving'; save()
        steps = math.ceil(args.duration/.05)
        for step in range(steps+1):
            tick=time.monotonic()
            goals = {}
            for row in rows:
                goal=round(row['position']+(row['partial']-row['position'])*step/steps)
                goals[row['id']] = goal
            # Official Feetech protocol-v1 SYNC_WRITE: broadcast, no ACK.
            # Feedback below remains mandatory; never retry uncertain motion writes.
            params = bytes([42, 2]) + b''.join(bytes([sid])+goal_bytes(goals[sid]) for sid in IDS)
            body = bytes([254, len(params)+2, 0x83]) + params
            bus.serial.write(b'\xff\xff'+body+bytes([(~sum(body))&255]))
            bus.serial.flush()
            feedback=bus.read_feedback_many(IDS)
            sample={'step':step,'servos':{}}
            for row in rows:
                sid=row['id']; f=feedback.get(sid)
                if f is None: raise ValueError(f'#{sid}: feedback lost')
                goal=round(row['position']+(row['partial']-row['position'])*step/steps)
                sample['servos'][sid]=f
                reasons=[]
                if not SERVO_VOLTAGE_RANGE[0] <= f['voltage'] <= SERVO_VOLTAGE_RANGE[1]: reasons.append('servo_voltage_out_of_spec')
                if f['voltage'] < args.battery_floor_voltage: reasons.append('2s_battery_trial_guard')
                if f['fault']: reasons.append('servo_fault')
                if f['temperature']>50: reasons.append('trial_temperature_guard')
                if abs(f['load'])>300: reasons.append('trial_load_guard')
                if abs(goal-f['position'])>55: reasons.append('tracking_error')
                if reasons:
                    result['trigger']={'step':step,'id':sid,'goal':goal,'feedback':f,'reasons':reasons}
                    raise ValueError(f'#{sid}: feedback protection, target={goal}, actual={f["position"]}, load={f["load"]}, fault={f["fault"]}, voltage={f["voltage"]}, temperature={f["temperature"]}')
            result['samples'].append(sample)
            time.sleep(max(0,args.duration/steps-(time.monotonic()-tick)))
        result['status']='stand-pose-commands-complete' if args.fraction==1 else 'partial-complete'
        if args.policy:
            import urllib.request
            import numpy as np
            sys.path.insert(0,'/home/radxa/microduck-shadow')
            import shadow_policy_20260930 as policy
            session=policy.session(Path(args.policy))
            meta=session.get_modelmeta().custom_metadata_map
            builder=policy.ObservationBuilder(meta,json.loads(Path('/home/radxa/microduck-shadow/model_20260930.json').read_text()),True)
            scale=float(meta['action_scale'])
            if not math.isfinite(scale) or scale<=0: raise ValueError('Invalid action scale')
            joint_ids=[j['id'] for j in builder.joints]
            context=(cal['revision'],cal['imu']['bootId'])
            anchors=None; last=np.zeros(14,dtype=np.float32)
            held={r['id']:r['standing'] for r in rows}
            sent=held.copy(); result['policySamples']=[]
            result['policyMode']='3s bounded 14-joint closed loop; head cap100, leg cap200; not autonomous balance'
            result['policyModel']=args.policy
            began=time.monotonic(); previous=began; deadline=began+3
            while time.monotonic()<deadline:
                tick=time.monotonic()
                with urllib.request.urlopen('http://127.0.0.1:8877/api/v1/snapshot',timeout=.1) as response:
                    snapshot=json.load(response)
                if (snapshot['calibration']['revision'],snapshot['imu.orientation']['bootId'])!=context:
                    raise ValueError('Policy calibration/session changed')
                feedback=bus.read_feedback_many(joint_ids); now=time.monotonic()
                if set(feedback)!=set(joint_ids): raise ValueError('Policy feedback missing')
                for sid,f in feedback.items():
                    if f['fault'] or not args.battery_floor_voltage<=f['voltage']<=8.4 or f['temperature']>50 or abs(f['load'])>300:
                        raise ValueError(f'Policy #{sid}: feedback protection')
                    if sid in IDS and (f['torque']!=1 or abs(sent[sid]-f['position'])>55):
                        raise ValueError(f'Policy #{sid}: torque/tracking protection')
                    if sid not in IDS and f['torque']!=0: raise ValueError('Head unexpectedly enabled')
                snapshot['joints']=dict(valid=True,source='hardware',bootId=context[1],ageMs=0,
                    sampleMonoMs=now*1000,data=dict(servos=[dict(id=sid,online=True,ageMs=(now-f['received'])*1000,**f) for sid,f in feedback.items()]))
                try: obs,details=builder.build(snapshot,last)
                except policy.InvalidObservation as exc:
                    if str(exc)!='warming velocity history': raise
                    time.sleep(.02); continue
                if anchors is None:
                    anchors=np.array([cal['joints']['references'][str(sid)]-cal['joints']['directions'].get(str(sid),-1)*branch*4096 for sid,branch in zip(joint_ids,details['periodic_branches'])])
                    branches=list(details['periodic_branches'])
                    directions=np.array([cal['joints']['directions'].get(str(sid),-1) for sid in joint_ids])
                    actual=np.array(details['joint_rad'])
                    for k,sid in enumerate(joint_ids):
                        if sid in IDS: actual[k]=(sent[sid]-anchors[k])/directions[k]*2*math.pi/4096
                    last=((actual-builder.defaults)/scale).astype(np.float32)
                    obs[34:48]=last
                if list(details['periodic_branches'])!=branches: raise ValueError('Policy encoder branch changed')
                predicted,ms=policy.infer(session,obs)
                targets=builder.defaults+scale*predicted
                actual=np.array(details['joint_rad']); goals={}
                dt=tick-previous; previous=tick
                if not 0<dt<=.15 or time.monotonic()-tick>.15: raise ValueError('Policy deadline exceeded')
                maximum=max(1,min(4,math.floor(80*dt)))
                for k,j in enumerate(builder.joints):
                    sid=j['id']
                    if sid not in IDS: continue
                    lo,hi=j['range']
                    if not lo<=targets[k]<=hi: raise ValueError(f'Policy #{sid}: model target outside joint limits')
                    requested=round(anchors[k]+directions[k]*targets[k]*4096/(2*math.pi))
                    desired=max(held[sid]-46,min(held[sid]+46,requested))
                    goal=sent[sid]+max(-maximum,min(maximum,desired-sent[sid]))
                    limits=next(r['limits'] for r in rows if r['id']==sid)
                    if limits[1]>limits[0] and not limits[0]<=goal<=limits[1]: raise ValueError('Policy EEPROM range violation')
                    q=(goal-anchors[k])/directions[k]*2*math.pi/4096
                    if not lo<=q<=hi: raise ValueError('Policy bounded target outside joint range')
                    goals[sid]=goal; actual[k]=q
                params=bytes([42,2])+b''.join(bytes([sid])+goal_bytes(goals[sid]) for sid in IDS)
                body=bytes([254,len(params)+2,0x83])+params
                bus.serial.write(b'\xff\xff'+body+bytes([(~sum(body))&255]));bus.serial.flush()
                sent=goals; last=((actual-builder.defaults)/scale).astype(np.float32)
                result['policySamples'].append(dict(elapsed=tick-began,gravity=details['gravity'],goals=goals,modelActions=predicted.tolist(),appliedActions=last.tolist(),inferenceMs=ms))
                time.sleep(max(0,.02-(time.monotonic()-tick)))
            if not result['policySamples']: raise ValueError('No closed-loop frames executed')
            result['status']='supported-pose-and-bounded-policy-complete'
        if args.keep_enabled:
            final=bus.read_feedback_many(IDS)
            if set(final)!=set(IDS): raise ValueError('Final hold feedback missing')
            for sid,f in final.items():
                goal=next(r['standing'] for r in rows if r['id']==sid)
                if f['torque']!=1 or f['fault'] or abs(goal-f['position'])>55 or f['temperature']>50 or abs(f['load'])>300 or not args.battery_floor_voltage<=f['voltage']<=8.4:
                    raise ValueError(f'#{sid}: final hold verification failed')
            result['holdFeedback']=final; result['status']='standing-static-hold-enabled'
            hold_verified=True
    else: result['status']='inspect-complete-no-writes'
except Exception as exc:
    result['status']='stopped'; result['error']=str(exc)
finally:
    signal.alarm(0)
    if attempted and bus and not hold_verified:
        # Broadcast torque-off first; no ACK is expected for broadcast packets.
        try:
            body=bytes([254,4,3,40,0]); bus.serial.write(b'\xff\xff'+body+bytes([(~sum(body))&255])); time.sleep(.05)
        except Exception as exc: result['cleanup'].append({'broadcastOffError':str(exc)})
        for sid in IDS:
            try:
                write_register(bus,sid,40,b'\x00')
                if read_register(bus,sid,40,1)!=b'\x00': raise ValueError('Torque off unconfirmed')
                f=bus.read_feedback(sid)
                encoded=abs(f['position'])|(0x8000 if f['position']<0 else 0)
                write_register(bus,sid,42,encoded.to_bytes(2,'little'))
                # Restore acceleration/time/speed and torque limit, never old goals.
                write_register(bus,sid,41,bytes([before[sid]['ram'][1]]))
                write_register(bus,sid,44,bytes(before[sid]['ram'][4:10]))
                # On this hardware, later SRAM writes were observed to leave
                # torque enabled again. Torque-off MUST be the final write.
                write_register(bus,sid,40,b'\x00')
                if read_register(bus,sid,40,1)!=b'\x00': raise ValueError('Final torque off unconfirmed')
                result['cleanup'].append({'id':sid,'torque':read_register(bus,sid,40,1)[0],'position':f['position']})
            except Exception as exc: result['cleanup'].append({'id':sid,'error':str(exc)})
    if bus:
        try: bus.close()
        except Exception: pass
    if paused:
        try: os.kill(paused,signal.SIGCONT);result['collectorResumed']=True
        except Exception as exc:result['resumeError']=str(exc)
    save()
print(json.dumps({k:v for k,v in result.items() if k not in ('before','samples','policySamples')}))
