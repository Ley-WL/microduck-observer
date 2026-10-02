import copy
import math
import unittest
from unittest.mock import patch
import numpy as np
import policy_control as p
from servo_control import target_for


class PolicyTests(unittest.TestCase):
    def setUp(self):
        self.policy=p.StandPolicy()
        self.cal=dict(imu=dict(initialized=True,quaternion=[0,0,0,1],mountingQuaternion=[0,0,0,1]),
            joints=dict(references={str(s):2048 for s in p.JOINT_IDS},directions={}))
        self.sensors={k:dict(valid=True,source='hardware',received=10.,bootId='boot',data={})
                      for k in ('imu.orientation','imu.raw')}
        self.sensors['imu.orientation']['data']['quaternion']=[0,0,0,1]
        self.sensors['imu.raw']['data']['gyro']=[1.,2.,3.]
        self.feedback={s:dict(position=2048,received=10.,torque=1,fault=0,voltage=5.9)
                       for s in p.JOINT_IDS}

    def test_sensor_axes_and_executed_previous_action(self):
        self.cal['imu']['mountingQuaternion']=[0,0,math.sqrt(.5),math.sqrt(.5)]
        self.policy.last_action[:]=.1
        obs=self.policy.observe(self.sensors,self.feedback,self.cal,now=10.)
        np.testing.assert_allclose(obs[:3],[2,-1,3],atol=1e-6)
        np.testing.assert_allclose(obs[3:6],[0,0,-1],atol=1e-6)
        np.testing.assert_allclose(obs[34:48],.1)
        self.assertEqual(obs.shape,(61,))
        action,ms=self.policy.infer(obs)
        self.assertEqual(action.shape,(14,));self.assertTrue(np.isfinite(action).all())

    def test_stale_imu_and_real_fault_are_rejected(self):
        with self.assertRaisesRegex(ValueError,'过期'):
            self.policy.observe(self.sensors,self.feedback,self.cal,now=11.)
        self.feedback[20]['fault']=1
        with self.assertRaisesRegex(ValueError,'故障'):
            self.policy.observe(self.sensors,self.feedback,self.cal,now=10.)

    def test_transition_uses_fresh_acquisition_without_web_producer(self):
        import multiprocessing, threading, time
        from imu_mailbox import ImuMailbox
        mailbox=ImuMailbox(multiprocessing.get_context('spawn'))
        done=threading.Event();halt=threading.Event()
        def acquisition():
            while not done.is_set():
                rows=copy.deepcopy(self.sensors)
                for row in rows.values(): row['received']=time.monotonic()
                mailbox.update(rows)
                done.wait(.005)
        thread=threading.Thread(target=acquisition);thread.start()
        rows=copy.deepcopy(self.feedback)
        class Bus:
            def read_feedback_many(self,_):
                for row in rows.values(): row['received']=time.monotonic()
                return rows
        cfg=bytearray(40);cfg[:2]=bytes([3,46]);cfg[33]=4;cfg[11:13]=(4095).to_bytes(2,'little')
        try:
            with patch.object(p,'read_register',return_value=cfg),patch.object(p,'sync_write') as write,\
                 patch.object(p,'execute_control',side_effect=lambda *a:time.sleep(2.2)),\
                 patch.object(p,'torque_off') as off,\
                 patch.object(p.StandPolicy,'infer',return_value=(np.zeros(14),.1)):
                result=p.execute_policy(Bus(),dict(calibration=self.cal),lambda:False,lambda *a:halt.set(),lambda *a:None,mailbox,halt)
                self.assertEqual(result['commandCount'],1)
                self.assertLess(max(result['imuAgeMs'].values()),150)
                write.assert_called_once();off.assert_not_called()
        finally:
            done.set();thread.join(2)

    def test_failure_saves_actual_missing_ids_before_unload_overwrites_trace(self):
        import threading,time,multiprocessing
        from imu_mailbox import ImuMailbox
        mailbox=ImuMailbox(multiprocessing.get_context('spawn'))
        rows=copy.deepcopy(self.feedback);samples=copy.deepcopy(self.sensors)
        for row in samples.values(): row['received']=time.monotonic()
        mailbox.update(samples)
        class Bus:
            calls=0
            trace=[]
            def read_feedback_many(self,_):
                self.calls+=1
                for row in rows.values(): row['received']=time.monotonic()
                if self.calls<3: return rows
                self.last_read=dict(missingIds=[10,11,12,13,14,21],elapsedMs=20.1,checksumErrors=2,rxBytes=350)
                return {sid:row for sid,row in rows.items() if sid not in self.last_read['missingIds']}
        bus=Bus()
        cfg=bytearray(40);cfg[:2]=bytes([3,46]);cfg[33]=4;cfg[11:13]=(4095).to_bytes(2,'little')
        def unload(_):
            bus.last_read={'missingIds':[]}
            return dict(message='已失能',state='disabled')
        with patch.object(p,'read_register',return_value=cfg),patch.object(p,'sync_write') as write,\
             patch.object(p,'execute_control'),patch.object(p,'torque_off',side_effect=unload),\
             patch.object(p,'save_policy_failure') as save,\
             patch.object(p.StandPolicy,'infer',return_value=(np.zeros(14),.1)):
            with self.assertRaisesRegex(ValueError,'本帧缺失.*10.*21'):
                p.execute_policy(bus,dict(calibration=self.cal),lambda:False,lambda *a:None,lambda *a:None,mailbox,threading.Event())
            write.assert_called_once()
            evidence=save.call_args.args[0]
            self.assertEqual(evidence['failedRead']['missingIds'],[10,11,12,13,14,21])
            self.assertEqual(evidence['stats']['commandCount'],1)
            self.assertIn(21,evidence['previousFeedback'])
            self.assertNotIn(21,evidence['feedback'])

    def test_hardware_branch_and_official_range_are_retained(self):
        cal=copy.deepcopy(self.cal);cal['joints']['references']['20']=6144
        self.assertEqual(target_for(20,2048,cal,0,4095,0,nearest=True),2048)
        with self.assertRaisesRegex(ValueError,'角度范围'):
            target_for(33,2048,cal,0,4095,math.radians(229),nearest=True)

    def test_shadow_does_not_write_or_enable(self):
        import queue,threading,time
        rows=copy.deepcopy(self.feedback)
        for r in rows.values(): r['received']=time.monotonic();r['torque']=0
        sensors=copy.deepcopy(self.sensors)
        for r in sensors.values(): r['received']=time.monotonic()
        q=queue.Queue();q.put(sensors)
        class Bus:
            def read_feedback_many(self,_): return rows
        cfg=bytearray(40);cfg[:2]=bytes([3,46]);cfg[33]=4;cfg[11:13]=(4095).to_bytes(2,'little')
        with patch.object(p,'read_register',return_value=cfg),patch.object(p,'sync_write') as write,\
             patch.object(p,'execute_control') as transition,patch.object(p.StandPolicy,'infer',return_value=(np.zeros(14),.1)):
            result=p.execute_policy(Bus(),dict(calibration=self.cal,shadow=True),lambda:False,lambda *a:None,lambda *a:None,q,threading.Event())
            self.assertEqual(result['state'],'shadow');write.assert_not_called();transition.assert_not_called()

    def test_running_policy_stops_without_an_extra_goal_and_keeps_executed_action(self):
        import queue,threading,time
        rows=copy.deepcopy(self.feedback)
        sensors=copy.deepcopy(self.sensors)
        for r in sensors.values(): r['received']=time.monotonic()
        q=queue.Queue();q.put(sensors);halt=threading.Event()
        class Bus:
            def read_feedback_many(self,_):
                for r in rows.values(): r['received']=time.monotonic()
                return rows
        cfg=bytearray(40);cfg[:2]=bytes([3,46]);cfg[33]=4;cfg[11:13]=(4095).to_bytes(2,'little')
        with patch.object(p,'read_register',return_value=cfg),patch.object(p,'sync_write') as write,\
             patch.object(p,'execute_control'),patch.object(p,'torque_off') as off,\
             patch.object(p.StandPolicy,'infer',return_value=(np.zeros(14),.1)):
            result=p.execute_policy(Bus(),dict(calibration=self.cal),lambda:False,lambda *a:halt.set(),lambda *a:None,q,halt)
            self.assertEqual(result['state'],'holding');self.assertEqual(result['commandCount'],1)
            write.assert_called_once();off.assert_not_called()

    def test_policy_overshoot_is_sent_at_official_limit_without_unloading(self):
        import queue,threading,time
        rows=copy.deepcopy(self.feedback)
        sensors=copy.deepcopy(self.sensors)
        for r in sensors.values(): r['received']=time.monotonic()
        q=queue.Queue();q.put(sensors);halt=threading.Event()
        class Bus:
            def read_feedback_many(self,_):
                for r in rows.values(): r['received']=time.monotonic()
                return rows
        cfg=bytearray(40);cfg[:2]=bytes([3,46]);cfg[33]=4;cfg[11:13]=(4095).to_bytes(2,'little')
        action=np.zeros(14);action[8]=4.
        with patch.object(p,'read_register',return_value=cfg),patch.object(p,'sync_write') as write,\
             patch.object(p,'execute_control'),patch.object(p,'torque_off',return_value={'message':'卸力'}) as off,\
             patch.object(p.StandPolicy,'infer',return_value=(action,.1)):
            result=p.execute_policy(Bus(),dict(calibration=self.cal),lambda:False,lambda *a:halt.set(),lambda *a:None,q,halt)
            self.assertEqual(result['saturatedIds'],[33]);self.assertAlmostEqual(result['sentTargetDegrees'][8],25.)
            write.assert_called_once();off.assert_not_called()


if __name__=='__main__': unittest.main()
