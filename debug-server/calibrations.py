"""Persistent display calibration, shared by Web and BLE; never hardware writes."""
import copy
import json
import math
import os
from pathlib import Path
import threading
import time
import uuid

IDS = {str(n) for n in (*range(10,15), *range(20,25), *range(30,35))}


class Conflict(ValueError): pass


class CalibrationStore:
    def __init__(self, path):
        self.path = Path(path); self.lock = threading.RLock()
        if self.path.exists():
            self.state = json.loads(self.path.read_text(encoding='utf-8'))
            self.validate({'joints':self.state['joints'], 'imu':self.state['imu']})
        else:
            self.state = dict(schema=1, deviceId=str(uuid.uuid4()), revision=0, updatedAt=0,
                joints=dict(initialized=False,references={},directions={}),
                imu=dict(initialized=False,quaternion=None,bootId='',time=''))
            self.persist(self.state)

    @staticmethod
    def validate(patch):
        if not isinstance(patch,dict) or not patch or set(patch)-{'joints','imu'}: raise ValueError('Invalid calibration section')
        if 'joints' in patch:
            j = patch['joints']
            if not isinstance(j,dict) or set(j)-{'initialized','references','directions'}: raise ValueError('Invalid joint calibration')
            for key in ('references','directions'):
                values=j.get(key)
                if not isinstance(values,dict) or set(values)-IDS: raise ValueError('Unknown servo ID')
                for value in values.values():
                    if type(value) not in (int,float) or not math.isfinite(value): raise ValueError('Calibration must be finite')
                    if key=='directions' and value not in (-1,1): raise ValueError('Direction must be -1 or 1')
                    if key=='references' and abs(value)>2147483647: raise ValueError('Encoder reference out of range')
        if 'imu' in patch:
            i=patch['imu']
            if not isinstance(i,dict) or set(i)-{'initialized','quaternion','bootId','time','mountingQuaternion','mountingSamples'}: raise ValueError('Invalid IMU calibration')
            q=i.get('quaternion')
            if q is not None and (not isinstance(q,list) or len(q)!=4 or not all(type(x) in (int,float) and math.isfinite(x) for x in q) or not .81<=sum(x*x for x in q)<=1.21): raise ValueError('Invalid unit quaternion')
            if not isinstance(i.get('bootId'),str) or len(i['bootId'])>100 or not isinstance(i.get('time',''),str) or len(i.get('time',''))>100: raise ValueError('Invalid IMU context')
            m=i.get('mountingQuaternion')
            if m is not None and (not isinstance(m,list) or len(m)!=4 or not all(type(x) in (int,float) and math.isfinite(x) for x in m) or not .99<sum(x*x for x in m)<1.01): raise ValueError('Invalid mounting quaternion')
            samples=i.get('mountingSamples',{})
            if not isinstance(samples,dict) or set(samples)-{'roll','pitch'}: raise ValueError('Invalid mounting samples')
            for axis in samples.values():
                if not isinstance(axis,list) or len(axis)!=3 or not all(type(x) in (int,float) and math.isfinite(x) for x in axis) or not .99<sum(x*x for x in axis)<1.01: raise ValueError('Invalid mounting axis')

    def read(self, boot):
        with self.lock:
            value=copy.deepcopy(self.state)
        value['imu']['validForBoot']=value['imu'].get('bootId')==boot
        value['bootId']=boot
        return value

    def update(self, revision, patch, boot, migrate=False):
        self.validate(patch)
        with self.lock:
            if type(revision) is not int or revision!=self.state['revision']: raise Conflict('标定已被其他页面更新，请重试')
            if migrate and any(self.state[k].get('initialized') for k in patch): raise Conflict('主板已有标定，未覆盖旧数据')
            if 'imu' in patch and patch['imu'].get('quaternion') is not None and patch['imu'].get('bootId')!=boot: raise Conflict('IMU 会话已改变，请重新归零')
            candidate=copy.deepcopy(self.state)
            for key,value in patch.items(): candidate[key]={**copy.deepcopy(value),'initialized':True}
            candidate.update(revision=candidate['revision']+1,updatedAt=int(time.time()*1000))
            self.persist(candidate)
            self.state=candidate
            return self.read(boot)

    def persist(self, candidate):
        self.path.parent.mkdir(parents=True,exist_ok=True)
        temporary=self.path.with_suffix('.tmp')
        fd=os.open(temporary,os.O_WRONLY|os.O_CREAT|os.O_TRUNC,0o600)
        with os.fdopen(fd,'w',encoding='utf-8') as stream:
            json.dump(candidate,stream,ensure_ascii=False,allow_nan=False);stream.flush();os.fsync(stream.fileno())
        # Keep the last saved revision for administrator recovery.
        if self.path.exists():
            backup=self.path.with_suffix('.previous.json')
            backup.write_bytes(self.path.read_bytes());os.chmod(backup,0o600)
        os.replace(temporary,self.path)
