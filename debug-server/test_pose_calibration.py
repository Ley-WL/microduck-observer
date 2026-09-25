import asyncio
import math
import tempfile
import unittest
from pathlib import Path
from unittest.mock import AsyncMock
from types import SimpleNamespace
from calibrations import CalibrationStore
from pose_calibration import PoseCalibration, calibrate_hardware, reference, mounting_axis, mounting_quaternion, imu_patch

class FakeBus:
    def __init__(self, torque=0, supported=True):
        self.serial=self; self.response=bytearray(); self.sent=[]; self.pos={12:2500,13:2600}
        self.offset={12:0,13:0};self.lock={12:1,13:1};self.torque=torque;self.supported=supported
    def reset_input_buffer(self): self.response.clear()
    def read(self,n):
        data=self.response[:n];del self.response[:n];return bytes(data)
    def write(self,frame):
        self.sent.append(frame);sid=frame[2];ins=frame[4];args=frame[5:-1];payload=b''
        if ins==2:
            reg,n=args
            payload=bytes([3,46,0,0,0,0]) if reg==0 else (self.offset[sid].to_bytes(2,'little') if reg==31 else bytes([self.lock[sid]]))
        elif ins==3:
            if args[0]==55:self.lock[sid]=args[1]
            if args[0]==40:raise AssertionError('No torque writes allowed')
        elif ins==11 and self.supported:
            self.pos[sid]=int.from_bytes(args,'little');self.offset[sid]=100
        body=bytes([sid,len(payload)+2,0])+payload
        self.response.extend(b'\xff\xff'+body+bytes([(~sum(body))&255]))
    def read_feedback(self,sid):return dict(position=self.pos[sid],torque=self.torque,fault=0)

class HardwareTests(unittest.TestCase):
    def plan(self):return dict(rows=[dict(id=12,position=2500,target=3072),dict(id=13,position=2600,target=3072)])
    def test_calibrates_with_backup_readback_and_lock_without_torque_enable(self):
        with tempfile.TemporaryDirectory() as d:
            bus=FakeBus();path=Path(d)/'backup.json';result=calibrate_hardware(bus,self.plan(),path)
            self.assertTrue(all(r['ok'] for r in result));self.assertTrue(path.is_file())
            self.assertEqual(bus.lock,{12:1,13:1})
            self.assertEqual([p[2] for p in bus.sent if p[4]==11],[12,13])
            self.assertFalse(any(p[4]==3 and p[5]==40 for p in bus.sent))
    def test_rejects_enabled_torque_before_any_write(self):
        with tempfile.TemporaryDirectory() as d:
            bus=FakeBus(torque=1)
            with self.assertRaises(ValueError):calibrate_hardware(bus,self.plan(),Path(d)/'b.json')
            self.assertFalse(bus.sent)
    def test_unsupported_calibration_stops_without_retrying_or_next_servo(self):
        with tempfile.TemporaryDirectory() as d:
            bus=FakeBus(supported=False);result=calibrate_hardware(bus,self.plan(),Path(d)/'b.json')
            self.assertFalse(result[0]['ok']);self.assertEqual(len(result),1)
            self.assertEqual(len([p for p in bus.sent if p[4]==11]),1)
            self.assertEqual(bus.lock[12],1)

class PoseTests(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.temp=tempfile.TemporaryDirectory();self.store=CalibrationStore(Path(self.temp.name)/'calibration.json')
        self.telemetry=SimpleNamespace(boot='boot',latest={})
        self.wizard=PoseCalibration(self.telemetry,self.store,lambda:None)
        self.wizard.capture=AsyncMock(return_value=([dict(id=12,position=4000,torque=0)],None))
    async def asyncTearDown(self):self.temp.cleanup()
    async def test_pose_reference_preserves_nonzero_angle_and_other_joints(self):
        self.store.update(0,{'joints':{'references':{'24':3000},'directions':{}}},'boot')
        plan=await self.wizard.preview(dict(pose='right',ids=[12],modes=['position']))
        r=plan['rows'][0];self.assertAlmostEqual((4000-r['reference'])*2*math.pi/4096*-1,-1.57)
        result=await self.wizard.execute(plan['token'])
        self.assertTrue(result['ok']);self.assertEqual(result['calibration']['joints']['references']['24'],3000)
        with self.assertRaises(ValueError):await self.wizard.execute(plan['token'])
    async def test_concurrent_change_and_movement_reject_execution(self):
        p=await self.wizard.preview(dict(pose='right',ids=[12],modes=['position']))
        self.wizard.capture=AsyncMock(return_value=([dict(id=12,position=4020,torque=0)],None))
        with self.assertRaises(ValueError):await self.wizard.execute(p['token'])
        self.assertEqual(self.store.read('boot')['revision'],0)
    async def test_stale_data_rejected(self):
        real=PoseCalibration(self.telemetry,self.store,lambda:None)
        with self.assertRaises(ValueError):await real.capture([12],False)
    async def test_invalid_imu_gravity_rejected(self):
        self.telemetry.latest={'imu.raw':dict(source='hardware',valid=True,seq=1,data=dict(accel=[1,5,1.8],gyro=[0,0,0]))}
        self.telemetry.stamp=lambda s:{**s,'ageMs':0}
        real=PoseCalibration(self.telemetry,self.store,lambda:None)
        with self.assertRaises(ValueError):await real.capture([],True)
    async def test_stable_live_window_accepts_joints_and_imu(self):
        self.telemetry.latest={
            'joints':dict(source='hardware',valid=True,seq=1,data=dict(servos=[dict(id=12,online=True,position=4971,torque=0,fault=0,ageMs=10)])),
            'imu.raw':dict(source='hardware',valid=True,seq=1,data=dict(accel=[0,0,9.81],gyro=[0,0,0])),
            'imu.orientation':dict(source='hardware',valid=True,seq=1,data=dict(quaternion=[0,0,0,1]))}
        def stamp(s):
            s['seq']+=1
            return {**s,'ageMs':10}
        self.telemetry.stamp=stamp
        real=PoseCalibration(self.telemetry,self.store,lambda:None)
        rows,q=await real.capture([12],True)
        self.assertEqual(rows[0]['position'],4971);self.assertEqual(q,[0,0,0,1])

class ImuMountingTests(unittest.TestCase):
    def test_three_pose_mounting_reconstructs_rotated_sensor_axes(self):
        identity=[0,0,0,1];h=math.sqrt(.5)
        # Body +X maps to sensor +Y, body +Y maps to sensor -X.
        x=mounting_axis(identity,[0,h,0,h]);y=mounting_axis(identity,[-h,0,0,h])
        q=mounting_quaternion(x,y)
        self.assertAlmostEqual(abs(q[2]),h);self.assertAlmostEqual(abs(q[3]),h)
        with self.assertRaises(ValueError):mounting_quaternion(x,x)
        with self.assertRaises(ValueError):mounting_axis(identity,identity)
    def test_requires_current_horizontal_reference(self):
        with self.assertRaises(ValueError):imu_patch({'imu':{'bootId':'old','quaternion':[0,0,0,1]}},'imu-roll',[0,0,0,1],'new')
