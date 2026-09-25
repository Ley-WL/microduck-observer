"""Pose-guided calibration. Hardware commands run only on the collector thread."""
import asyncio
import copy
import json
import math
import os
import time
import uuid
from pathlib import Path
from servos import ALL_IDS

FOLD = {sid: 0.0 for sid in ALL_IDS}
FOLD.update({22: 1.57, 23: 1.57, 12: -1.57, 13: -1.57})
POSES = {
    'fold': dict(name='折叠支撑 · 全身', ids=list(ALL_IDS), angles=FOLD),
    'left': dict(name='折叠支撑 · 左腿', ids=list(range(20,25)), angles=FOLD),
    'right': dict(name='折叠支撑 · 右腿', ids=list(range(10,15)), angles=FOLD),
    'head': dict(name='头颈摆正 · 闭嘴', ids=list(range(30,35)), angles=FOLD),
    'imu': dict(name='躯干水平 · IMU', ids=[], angles=FOLD),
    'imu-roll': dict(name='右侧放 · IMU 安装轴 X', ids=[], angles=FOLD),
    'imu-pitch': dict(name='低头支撑 · IMU 安装轴 Y', ids=[], angles=FOLD),
}

def multiply(a,b):
    x,y,z,w=a; u,v,s,t=b
    return [w*u+x*t+y*s-z*v,w*v-x*s+y*t+z*u,w*s+x*v-y*u+z*t,w*t-x*u-y*v-z*s]

def mounting_axis(reference_q, current):
    delta=multiply([-v for v in reference_q[:3]]+[reference_q[3]],current)
    norm=math.sqrt(sum(v*v for v in delta)); delta=[v/norm for v in delta]
    if delta[3]<0: delta=[-v for v in delta]
    angle=2*math.acos(max(-1,min(1,delta[3])))
    if not math.radians(70)<angle<math.radians(110): raise ValueError('请从水平参考转动约 +90°，当前姿势不符合轴向标定')
    n=math.sqrt(sum(v*v for v in delta[:3])); return [v/n for v in delta[:3]]

def mounting_quaternion(x,y):
    dot=sum(a*b for a,b in zip(x,y))
    if abs(dot)>.17: raise ValueError('两个姿势的转轴不垂直，请重新采集侧放和低头姿势')
    y=[b-dot*a for a,b in zip(x,y)]; norm=math.sqrt(sum(v*v for v in y)); y=[v/norm for v in y]
    z=[x[1]*y[2]-x[2]*y[1],x[2]*y[0]-x[0]*y[2],x[0]*y[1]-x[1]*y[0]]
    m=[[x[i],y[i],z[i]] for i in range(3)]; tr=sum(m[i][i] for i in range(3))
    if tr>0:
        s=math.sqrt(tr+1)*2; q=[(m[2][1]-m[1][2])/s,(m[0][2]-m[2][0])/s,(m[1][0]-m[0][1])/s,s/4]
    else:
        i=max(range(3),key=lambda i:m[i][i]); j=(i+1)%3; k=(i+2)%3
        s=math.sqrt(1+m[i][i]-m[j][j]-m[k][k])*2; q=[0.,0.,0.,0.]
        q[i]=s/4; q[j]=(m[j][i]+m[i][j])/s; q[k]=(m[k][i]+m[i][k])/s; q[3]=(m[k][j]-m[j][k])/s
    n=math.sqrt(sum(v*v for v in q));return [v/n for v in q]

def imu_patch(state, pose, q, boot):
    if pose not in ('imu-roll','imu-pitch'):
        return dict(quaternion=q,bootId=boot,time=time.strftime('%Y-%m-%d %H:%M:%S'))
    old=state['imu']
    if old.get('bootId')!=boot or not old.get('quaternion'): raise ValueError('请先采集本次会话的躯干水平参考')
    axis=mounting_axis(old['quaternion'],q)
    samples=copy.deepcopy(old.get('mountingSamples',{}));samples['roll' if pose=='imu-roll' else 'pitch']=axis
    result={**old,'mountingSamples':samples};result.pop('validForBoot',None)
    result.pop('mountingQuaternion',None)
    if 'roll' in samples and 'pitch' in samples: result['mountingQuaternion']=mounting_quaternion(samples['roll'],samples['pitch'])
    return result

def reference(position, angle, direction):
    return position - angle * 4096 / (2 * math.pi) * direction

def packet(bus, sid, instruction, params, expected=None):
    serial = bus.serial
    body = bytes([sid, len(params)+2, instruction]) + params
    serial.reset_input_buffer()
    serial.write(b'\xff\xff' + body + bytes([(~sum(body)) & 255]))
    received = bytearray(); deadline = time.monotonic() + .2
    while time.monotonic() < deadline:
        received.extend(serial.read(1))
        while len(received) >= 4:
            if received[:2] != b'\xff\xff' or not 2 <= received[3] <= 64:
                del received[0]; continue
            size = received[3]+4
            if len(received) < size: break
            frame = bytes(received[:size]); del received[:size]
            if frame[2] != sid or sum(frame[2:]) & 255 != 255: continue
            if expected is not None and len(frame[5:-1]) != expected: continue
            if frame[4]: raise ValueError(f'#{sid} 返回故障 {frame[4]}')
            return frame[5:-1]
    raise TimeoutError(f'#{sid} 指令应答超时')

def read_register(bus, sid, address, size):
    return packet(bus, sid, 2, bytes([address,size]), size)

def write_register(bus, sid, address, data):
    packet(bus, sid, 3, bytes([address])+data, 0)

def calibrate_hardware(bus, plan, backup_path):
    """No motion/torque-enable writes, no retries of uncertain EEPROM operations."""
    before = {}
    for row in plan['rows']:
        sid = row['id']; feedback = bus.read_feedback(sid)
        if feedback['torque'] != 0 or feedback['fault'] or abs(feedback['position']-row['position']) > 8:
            raise ValueError(f'#{sid} 扭矩、位置或故障状态已改变，请重新采样')
        before[str(sid)] = dict(feedback=feedback,
            offset=list(read_register(bus,sid,31,2)), lock=list(read_register(bus,sid,55,1)),
            version=list(read_register(bus,sid,0,6)))
        if before[str(sid)]['version'][:2] != [3,46]:
            raise ValueError(f'#{sid} 固件不是已核对的 3.46，停止硬件校准；可先做软件位置标定')
    backup_path.parent.mkdir(parents=True, exist_ok=True)
    with os.fdopen(os.open(backup_path,os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600),'w',encoding='utf-8') as stream:
        json.dump(dict(plan=plan,before=before),stream,ensure_ascii=False)
        stream.flush();os.fsync(stream.fileno())
    results=[]
    for row in plan['rows']:
        sid=row['id']; result=dict(id=sid,ok=False,attempted=False)
        try:
            feedback=bus.read_feedback(sid)
            if feedback['torque'] != 0 or feedback['fault'] or abs(feedback['position']-row['position'])>8:
                raise ValueError('姿势改变或扭矩未关闭，停止后续写入')
            # Mark attempted before unlocking: a lost ACK is an uncertain write.
            result['attempted']=True
            write_register(bus,sid,55,b'\x00')
            packet(bus,sid,11,int(row['target']).to_bytes(2,'little'),0)
            time.sleep(.08)
            after=bus.read_feedback(sid)
            offset=list(read_register(bus,sid,31,2))
            if abs(after['position']-row['target'])>3 or after['torque'] != 0 or after['fault']:
                raise ValueError('写后位置或状态校验失败；保留备份，不自动重试')
            if abs(feedback['position']-row['target'])>3 and offset==before[str(sid)]['offset']:
                raise ValueError('偏移未改变，固件可能不支持 0x0B 参数校准')
            result.update(ok=True,position=after['position'],offset=offset)
        except Exception as exc:
            result['error']=str(exc)
        finally:
            if result['attempted']:
                try:
                    # Align the stored target with the actual new coordinate before later torque use.
                    current=bus.read_feedback(sid)['position']
                    encoded=abs(current) | (0x8000 if current<0 else 0)
                    write_register(bus,sid,42,encoded.to_bytes(2,'little'))
                    write_register(bus,sid,55,b'\x01')
                    if read_register(bus,sid,55,1)!=b'\x01': raise ValueError('EEPROM 锁定未确认')
                except Exception as exc:
                    result.update(ok=False,error='收尾校验失败：'+str(exc))
        results.append(result)
        if not result['ok']: break
    backup_path.with_suffix('.result.json').write_text(json.dumps(results,ensure_ascii=False),encoding='utf-8')
    return results

class PoseCalibration:
    def __init__(self, telemetry, store, source):
        self.telemetry=telemetry; self.store=store; self.source=source
        self.plans={}; self.lock=asyncio.Lock()

    async def capture(self, ids, imu):
        values={sid:[] for sid in ids}; quats=[]; seen={}; boot=self.telemetry.boot
        until=time.monotonic()+1.3
        while time.monotonic()<until:
            for topic in (['joints'] if ids else [])+(['imu.raw','imu.orientation'] if imu else []):
                s=self.telemetry.latest.get(topic)
                if not s or s.get('source')!='hardware' or not s['valid'] or self.telemetry.stamp(s)['ageMs']>350:
                    raise ValueError(f'{topic} 无新鲜实机数据')
                if seen.get(topic)==s['seq']: continue
                seen[topic]=s['seq']
                if topic=='joints':
                    rows={r['id']:r for r in s['data']['servos']}
                    for sid in ids:
                        r=rows.get(sid)
                        if not r or not r['online'] or r.get('fault') or r.get('ageMs',999)>350:
                            raise ValueError(f'#{sid} 离线、过期或故障；可取消该关节后分批标定')
                        values[sid].append(copy.deepcopy(r))
                elif topic=='imu.raw':
                    a=s['data']['accel']; g=s['data']['gyro']
                    if not all(math.isfinite(x) for x in a+g) or not 8.3<math.sqrt(sum(x*x for x in a))<11.3 or math.sqrt(sum(x*x for x in g))>.08:
                        raise ValueError('IMU 未静止或重力读数异常，不能用标定掩盖传感器异常')
                else:
                    q=s['data']['quaternion']
                    if len(q)!=4 or not all(math.isfinite(x) for x in q) or not .95<math.sqrt(sum(x*x for x in q))<1.05:
                        raise ValueError('IMU 四元数无效')
                    quats.append(q)
            await asyncio.sleep(.025)
        if boot!=self.telemetry.boot: raise ValueError('采样期间后端会话改变')
        result=[]
        for sid, samples in values.items():
            if len(samples)<5: raise ValueError(f'#{sid} 有效样本不足')
            positions=[r['position'] for r in samples]
            if max(positions)-min(positions)>8: raise ValueError(f'#{sid} 未保持静止，请支撑关节后重试')
            result.append(dict(id=sid,position=sum(positions)/len(positions),torque=samples[-1]['torque']))
        q=None
        if imu:
            if len(quats)<15: raise ValueError('IMU 有效样本不足')
            normalized=[[v/math.sqrt(sum(x*x for x in item)) for v in item] for item in quats]
            if any(abs(sum(a*b for a,b in zip(normalized[0],item)))<math.cos(math.radians(2)/2) for item in normalized):
                raise ValueError('IMU 朝向不稳定')
            aligned=[[v*(1 if sum(a*b for a,b in zip(normalized[0],item))>=0 else -1) for v in item] for item in normalized]
            mean=[sum(q[k] for q in aligned)/len(aligned) for k in range(4)]
            q=[v/math.sqrt(sum(x*x for x in mean)) for v in mean]
        return result,q

    async def preview(self, body):
        pose=body.get('pose'); ids=body.get('ids'); modes=body.get('modes')
        if pose not in POSES or not isinstance(ids,list) or len(ids)!=len(set(ids)) or any(type(i)!=int or i not in POSES[pose]['ids'] for i in ids): raise ValueError('无效姿势或关节选择')
        if not isinstance(modes,list) or not modes or set(modes)-{'position','hardware','imu'}: raise ValueError('请选择标定项目')
        if ('position' in modes or 'hardware' in modes) and not ids: raise ValueError('请选择至少一个关节')
        state=self.store.read(self.telemetry.boot)
        rows,q=await self.capture(ids if set(modes)&{'position','hardware'} else [],'imu' in modes)
        if q: imu_patch(state,pose,q,self.telemetry.boot)
        if 'hardware' in modes and any(r['torque']!=0 for r in rows): raise ValueError('硬件中位要求所有所选舵机扭矩关闭；本页面不会自动开关扭矩')
        for r in rows:
            direction=state['joints']['directions'].get(str(r['id']),-1)
            angle=POSES[pose]['angles'][r['id']]
            r.update(direction=direction,angle=angle,reference=reference(r['position'],angle,direction),target=round(2048+angle*4096/(2*math.pi)*direction))
        plan=dict(token=uuid.uuid4().hex,bootId=self.telemetry.boot,revision=state['revision'],pose=pose,modes=modes,rows=rows,quaternion=q,created=time.monotonic(),previousCalibration=state)
        self.plans={plan['token']:plan}
        return plan

    async def execute(self, token):
        plan=self.plans.pop(token,None)
        if not plan or time.monotonic()-plan['created']>60 or plan['bootId']!=self.telemetry.boot: raise ValueError('预览已过期，请重新采样')
        state=self.store.read(self.telemetry.boot)
        if state['revision']!=plan['revision']: raise ValueError('标定已被其他页面修改，请重新采样')
        rows,q=await self.capture([r['id'] for r in plan['rows']],'imu' in plan['modes'])
        if any(abs(a['position']-b['position'])>8 for a,b in zip(rows,plan['rows'])): raise ValueError('姿势已改变，请重新采样')
        if q and abs(sum(a*b for a,b in zip(q,plan['quaternion'])))<math.cos(math.radians(2)/2): raise ValueError('IMU 姿势已改变')
        joints=copy.deepcopy(state['joints']); results=[]
        if 'hardware' in plan['modes']:
            source=self.source()
            if not source: raise ValueError('舵机串口未启用')
            path=self.store.path.parent/'calibration-backups'/(plan['token']+'.json')
            future=source.submit(lambda bus: calibrate_hardware(bus,plan,path))
            try: results=await asyncio.wait_for(asyncio.shield(asyncio.wrap_future(future)),30)
            except asyncio.TimeoutError:
                # Do not release the calibration lock while a serial job can still write.
                if future.cancel(): raise ValueError('串口任务未开始，已取消；检查串口连接')
                results=await asyncio.shield(asyncio.wrap_future(future))
            for result in results:
                sid=str(result['id'])
                if result['ok']: joints['references'][sid]=2048
                elif result['attempted']: joints['references'].pop(sid,None)
        elif 'position' in plan['modes']:
            for row in plan['rows']: joints['references'][str(row['id'])]=row['reference']
        patch={}
        if plan['rows']: patch['joints']=joints
        success=all(r['ok'] for r in results)
        if 'imu' in plan['modes'] and success: patch['imu']=imu_patch(state,plan['pose'],q,self.telemetry.boot)
        saved=self.store.update(state['revision'],patch,self.telemetry.boot) if patch else state
        return dict(ok=success,calibration=saved,results=results,backup=plan['token'] if results else None)
