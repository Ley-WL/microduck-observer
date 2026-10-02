"""HD1910 v5 stand policy. Runs only after an explicit platform start request."""
import hashlib
import json
import math
import queue
import os
import time
from pathlib import Path

import numpy as np
from servo_control import (OFFICIAL_LIMITS_DEG, CONTROL_PERIOD, execute_control,
                           sync_write, goal_bytes, target_for, validate_feedback, torque_off)
from pose_calibration import read_register
from servos import ALL_IDS

POLICY_PATH = Path(__file__).parent / 'models/hd1910-head-v5.onnx'
JOINT_IDS = [20,21,22,23,24,30,31,32,33,10,11,12,13,14]
JOINT_NAMES = ['left_hip_yaw','left_hip_roll','left_hip_pitch','left_knee','left_ankle',
               'neck_pitch','head_pitch','head_yaw','head_roll',
               'right_hip_yaw','right_hip_roll','right_hip_pitch','right_knee','right_ankle']


def save_policy_failure(evidence):
    calibration_path=Path(os.environ.get('MICRODUCK_CALIBRATION_FILE',
        str(Path.home()/'.microduck-observer/calibration.json')))
    destination=calibration_path.parent/'policy-failure-latest.json'
    temporary=destination.with_suffix('.tmp')
    destination.parent.mkdir(parents=True,exist_ok=True)
    temporary.write_text(json.dumps(evidence,ensure_ascii=False,indent=2),encoding='utf-8')
    temporary.replace(destination)


def rotation(q):
    q=np.asarray(q,dtype=float)
    if q.shape!=(4,) or not np.isfinite(q).all() or not .95<np.linalg.norm(q)<1.05:
        raise ValueError('IMU 四元数无效')
    x,y,z,w=q/np.linalg.norm(q)
    return np.array([[1-2*(y*y+z*z),2*(x*y-z*w),2*(x*z+y*w)],
                     [2*(x*y+z*w),1-2*(x*x+z*z),2*(y*z-x*w)],
                     [2*(x*z-y*w),2*(y*z+x*w),1-2*(x*x+y*y)]])


class StandPolicy:
    def __init__(self, path=POLICY_PATH):
        import onnxruntime as ort
        self.path=Path(path)
        self.meta=json.loads(self.path.with_suffix('.metadata.json').read_text(encoding='utf-8'))
        if self.meta['jointOrder']!=JOINT_NAMES or self.meta['controlHz']!=50:
            raise ValueError('模型关节顺序或频率不匹配')
        self.sha=hashlib.sha256(self.path.read_bytes()).hexdigest()
        if self.sha!=self.meta['policySha256']:
            raise ValueError('模型 SHA256 不匹配')
        self.home=np.asarray(self.meta['homeRadians'],dtype=np.float32)
        opts=ort.SessionOptions();opts.intra_op_num_threads=1;opts.inter_op_num_threads=1
        self.session=ort.InferenceSession(str(self.path),sess_options=opts,providers=['CPUExecutionProvider'])
        if self.session.get_inputs()[0].shape!=[1,61] or self.session.get_outputs()[0].shape!=[1,14]:
            raise ValueError('模型输入输出维度不匹配')
        self.previous=None
        self.last_action=np.zeros(14,dtype=np.float32)
        self.previous_velocity=np.zeros(14,dtype=np.float32)

    def observe(self, sensors, feedback, calibration, now=None):
        now=time.monotonic() if now is None else now
        for key in ('imu.orientation','imu.raw'):
            sample=sensors.get(key)
            if not sample or not sample['valid'] or sample['source']!='hardware' or not 0<=now-sample['received']<.15:
                age = round((now-sample['received'])*1000, 2) if sample and 'received' in sample else None
                raise ValueError(f'IMU 数据缺失或过期（{key}，age={age}ms，valid={sample.get("valid") if sample else None}）')
        if sensors['imu.orientation']['bootId']!=sensors['imu.raw']['bootId']:
            raise ValueError('IMU 会话不一致')
        imu=calibration['imu']
        if not imu.get('initialized') or not imu.get('mountingQuaternion'):
            raise ValueError('请先完成 IMU 位置和安装方向标定')
        # Saved mounting M is body->sensor. The platform's relative body pose is
        # target * M.T * reference.T * current * M. Gyro uses M.T directly.
        M=rotation(imu['mountingQuaternion'])
        target=rotation(imu.get('targetQuaternion',[0,0,0,1]))
        relative=rotation(imu['quaternion']).T@rotation(sensors['imu.orientation']['data']['quaternion'])
        body=target@M.T@relative@M
        gravity=body.T@np.array([0.,0.,-1.])
        gyro=np.asarray(sensors['imu.raw']['data']['gyro'],dtype=float)
        if gyro.shape!=(3,) or not np.isfinite(gyro).all(): raise ValueError('IMU 角速度无效')
        angles=[];stamps=[]
        for sid in JOINT_IDS:
            f=feedback.get(sid);validate_feedback(sid,f)
            if not 0<=now-f.get('received',now)<.15: raise ValueError(f'#{sid} 反馈过期')
            ref=calibration['joints']['references'].get(str(sid))
            direction=calibration['joints']['directions'].get(str(sid),-1)
            if ref is None or direction not in (-1,1): raise ValueError(f'#{sid} 缺少位置标定')
            raw=(f['position']-ref)*2*math.pi/4096*direction
            lo,hi=map(math.radians,OFFICIAL_LIMITS_DEG[sid])
            k0=math.ceil((lo-raw)/(2*math.pi));k1=math.floor((hi-raw)/(2*math.pi))
            if k0!=k1: raise ValueError(f'#{sid} 编码位置不在关节角度范围内')
            angles.append(raw+k0*2*math.pi);stamps.append(f.get('received',now))
        angles=np.asarray(angles);stamps=np.asarray(stamps)
        velocity=np.zeros(14)
        if self.previous is not None:
            old,old_t=self.previous;dt=stamps-old_t
            if np.any(dt<0) or np.any(dt>.25): raise ValueError('关节反馈时间不连续')
            changed=dt>1e-6
            velocity=self.previous_velocity.copy()
            velocity[changed]=(angles[changed]-old[changed])/dt[changed]
        # Match the CPU preview/training's one-step delayed velocity observation.
        obs=np.concatenate((M.T@gyro,gravity,angles-self.home,
                            self.previous_velocity,self.last_action,np.zeros(13))).astype(np.float32)
        self.previous=(angles,stamps);self.previous_velocity=velocity
        if not np.isfinite(obs).all(): raise ValueError('模型观测无效')
        return obs

    def infer(self, obs):
        start=time.perf_counter()
        action=self.session.run(None,{self.session.get_inputs()[0].name:obs[None]})[0][0]
        if action.shape!=(14,) or not np.isfinite(action).all(): raise ValueError('模型输出无效')
        return action,(time.perf_counter()-start)*1000

    def targets(self, action, feedback, calibration, limits):
        raw=self.home+action
        low=np.radians([OFFICIAL_LIMITS_DEG[sid][0] for sid in JOINT_IDS])
        high=np.radians([OFFICIAL_LIMITS_DEG[sid][1] for sid in JOINT_IDS])
        # Policy position setpoints can overshoot mechanical stops in training.
        # Constrain only the official joint range; overshoot is not a servo fault.
        angles=np.clip(raw,low,high)
        self.raw_target_degrees=np.degrees(raw).tolist()
        self.sent_target_degrees=np.degrees(angles).tolist()
        self.saturated_ids=[sid for sid,r,v in zip(JOINT_IDS,raw,angles) if abs(float(r-v))>1e-6]
        return {sid:target_for(sid,feedback[sid]['position'],calibration,*limits[sid],
                    float(angle),nearest=True) for sid,angle in zip(JOINT_IDS,angles)}


def execute_policy(bus, command, cancelled, publish, status, sensors_queue, halt):
    policy=StandPolicy();cal=command['calibration'];sensors={};attempted=False
    count=0;first=None;last=None;max_gap=0.;infer_ms=0.;stage='preflight'
    previous_feedback={};feedback={};goals={}
    inference_observation=None;inference_sensors={}
    def latest_sensors():
        nonlocal sensors
        if hasattr(sensors_queue, "snapshot"):
            sensors = sensors_queue.snapshot()
            return sensors
        while True:
            try: sensors=sensors_queue.get_nowait()
            except queue.Empty: break
        return sensors
    def stats():
        return dict(policy='hd1910-head-v5',policySha256=policy.sha,commandTargetHz=50,
            commandCount=count,commandHz=round((count-1)/(last-first),2) if count>1 else None,
            commandMaxGapMs=round(max_gap*1000,2),inferenceMs=round(infer_ms,3),
            saturatedIds=getattr(policy,'saturated_ids',[]),
            rawTargetDegrees=getattr(policy,'raw_target_degrees',[]),
            sentTargetDegrees=getattr(policy,'sent_target_degrees',[]),
            imuAgeMs={key:round((time.monotonic()-row['received'])*1000,2) for key,row in sensors.items()})
    try:
        if cancelled() or halt.is_set(): return dict(state='holding',action='policy',message='模型启动已取消')
        status(dict(state='preflight',action='policy',message='v5 模型与实时传感器预检查'))
        feedback=bus.read_feedback_many(ALL_IDS)
        policy.observe(latest_sensors(),feedback,cal)
        limits={}
        for sid in JOINT_IDS:
            cfg=read_register(bus,sid,0,40)
            if list(cfg[:2])!=[3,46] or cfg[33]!=4: raise ValueError(f'#{sid} 固件或模式不匹配')
            limits[sid]=(int.from_bytes(cfg[9:11],'little'),int.from_bytes(cfg[11:13],'little'))
        policy.targets(np.zeros(14),feedback,cal,limits)
        if command.get('shadow'):
            feedback=bus.read_feedback_many(ALL_IDS)
            action,infer_ms=policy.infer(policy.observe(latest_sensors(),feedback,cal))
            try:
                goals=policy.targets(action,feedback,cal,limits)
                reason='';compatible=True
            except ValueError as exc:
                goals={};reason=str(exc);compatible=False
            return dict(state='shadow',action='policy',
                message='v5 只读推理完成，未发运动指令'+('；当前姿态目标不兼容：'+reason if reason else ''),
                **stats(),targets=goals,targetDegrees=np.degrees(policy.home+action).tolist(),
                currentPoseTargetsCompatible=compatible,reason=reason)
        home_targets=dict(zip(JOINT_IDS,map(float,policy.home)))
        stage='home-transition'
        attempted=True
        execute_control(bus,dict(action='stand',calibration=cal,standTargets=home_targets),
            cancelled,publish,lambda value:status({**value,'action':'policy'}))
        # Reset history after the HOME transition; previous action starts at zero.
        policy.previous=None;policy.previous_velocity[:]=0;policy.last_action[:]=0
        stage='policy-loop'
        deadline=time.monotonic();last_status=0.
        while not cancelled() and not halt.is_set():
            feedback=bus.read_feedback_many(ALL_IDS)
            obs=policy.observe(latest_sensors(),feedback,cal)
            for sid in JOINT_IDS:
                if feedback[sid]['torque']!=1: raise ValueError(f'#{sid} 实时使能状态丢失')
            action,infer_ms=policy.infer(obs)
            inference_observation=obs.tolist();inference_sensors=sensors
            goals=policy.targets(action,feedback,cal,limits)
            if cancelled() or halt.is_set(): break
            sync_write(bus,42,{sid:goal_bytes(goal) for sid,goal in goals.items()})
            # Training's actions observation contains the raw policy output,
            # even when mechanical stops restrict the resulting joint motion.
            policy.last_action=action.copy()
            previous_feedback=feedback
            tick=time.monotonic();count+=1
            if first is None: first=tick
            if last is not None: max_gap=max(max_gap,tick-last)
            last=tick
            publish([dict(id=sid,online=sid in feedback,**feedback.get(sid,{})) for sid in ALL_IDS])
            if tick-last_status>=.2:
                bounded=('；按官方范围限幅 '+','.join('#'+str(s) for s in policy.saturated_ids)) if policy.saturated_ids else ''
                status(dict(state='policy',action='policy',message='v5 模型站立维持中'+bounded,**stats()))
                last_status=tick
            deadline=max(deadline+CONTROL_PERIOD,tick)
            halt.wait(max(0,deadline-time.monotonic()))
        if cancelled():
            off=torque_off(bus)
            return dict(**off,action='policy',**stats())
        # Explicit stop ends inference and leaves the last target holding torque.
        return dict(state='holding',action='policy',message='模型已停止，保持最后目标；可全部失能卸力',**stats())
    except Exception as exc:
        if attempted:
            diagnostic=getattr(bus,'last_read',None)
            evidence=dict(error=str(exc),stage=stage,eventTime=time.time(),
                calibrationRevision=cal.get('revision'),stats=stats(),
                lastInferenceObservation=inference_observation,lastInferenceSensors=inference_sensors,
                imuCalibration=cal.get('imu'),jointCalibration=cal.get('joints'),
                feedback=feedback,previousFeedback=previous_feedback,goals=goals,
                failedRead=diagnostic,serialTrace=list(getattr(bus,'trace',[])))
            detail=''
            if diagnostic and diagnostic.get('missingIds'):
                detail=(f'；本帧缺失{diagnostic["missingIds"]}，读取{diagnostic["elapsedMs"]}ms'
                        f'，校验错误{diagnostic["checksumErrors"]}，收到{diagnostic["rxBytes"]}字节')
            try:
                off=torque_off(bus)
            except Exception as off_error:
                off=dict(state='failed',message=f'全部失能未确认：{off_error}')
            evidence['unload']=off
            try:
                save_policy_failure(evidence)
                saved='；诊断已保存'
            except OSError as save_error:
                saved=f'；诊断保存失败：{type(save_error).__name__}'
            raise ValueError(f'{exc}；stage={stage}，已发令{count}次{detail}{saved}；{off["message"]}') from exc
        raise ValueError(f'{exc}；只读预检查未通过，未发运动指令') from exc
