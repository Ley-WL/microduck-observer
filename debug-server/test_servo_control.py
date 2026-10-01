import copy
import math
import threading
import queue
import unittest
from types import SimpleNamespace
from unittest.mock import patch
from concurrent.futures import Future
from fastapi.testclient import TestClient
import servo_control as c
from servos import ALL_IDS, signed, ServoPoller


class Clock:
    def __init__(self): self.now = 0.
    def __call__(self): return self.now
    def sleep(self, n): self.now += max(n, .001)


class Bus:
    def __init__(self):
        self.rows = {sid: dict(position=2048, target=2048, torque=0, fault=0, load=0, voltage=7.4, temperature=30, accelerationRaw=0, speedLimitRaw=500) for sid in ALL_IDS}
        self.writes = []; self.caps = {}; self.drop = None; self.stuck = False
    def read_feedback_many(self, ids):
        return {sid: copy.deepcopy(self.rows[sid]) for sid in ids if sid != self.drop}
    def read(self, bus, sid, address, size):
        if address == 48: return self.caps[sid]
        data = bytearray(40); data[:2] = bytes([3,46]); data[33] = 4
        data[11:13] = (4095).to_bytes(2,'little'); data[16:18] = (1000).to_bytes(2,'little')
        return data
    def write(self, bus, sid, address, data):
        self.writes.append((address, {sid:data})); self.caps[sid] = data
    def sync(self, bus, address, values):
        self.writes.append((address, values))
        for sid, data in values.items():
            if address == 41: self.rows[sid]['accelerationRaw'] = data[0]
            if address == 46: self.rows[sid]['speedLimitRaw'] = int.from_bytes(data, 'little')
            if address == 40: self.rows[sid]['torque'] = data[0]
            if address == 42:
                goal = signed(int.from_bytes(data, 'little'), 15)
                self.rows[sid]['target'] = goal
                if not self.stuck: self.rows[sid]['position'] = goal


class ControlTests(unittest.TestCase):
    def setUp(self):
        self.bus = Bus(); self.clock = Clock(); self.statuses = []; self.frames = []
        self.cal = dict(joints=dict(references={str(sid):2048 for sid in ALL_IDS}, directions={}))
        for name, method in [('read_register',self.bus.read),('write_register',self.bus.write),('sync_write',self.bus.sync)]:
            p = patch.object(c, name, method); p.start(); self.addCleanup(p.stop)
    def run_control(self, action='stand', cancelled=lambda:False):
        return c.execute_control(self.bus,dict(action=action, calibration=self.cal),cancelled,self.frames.append,self.statuses.append,self.clock,self.clock.sleep)
    def test_stand_reaches_official_targets_within_three_seconds_and_holds(self):
        self.bus.rows[34]['position'] = 2400
        result = self.run_control()
        self.assertEqual(result['state'],'holding'); self.assertLessEqual(result['elapsedSeconds'],3)
        self.assertGreaterEqual(result['elapsedSeconds'],2.2)
        for sid in c.STAND:
            self.assertEqual(self.bus.rows[sid]['position'],c.target_for(sid,2048,self.cal,0,4095))
            self.assertEqual(self.bus.rows[sid]['torque'],1)
        self.assertEqual(self.bus.rows[34]['position'],2400); self.assertEqual(self.bus.rows[34]['torque'],0)
        self.assertGreater(len(self.frames),110)
        self.assertAlmostEqual(result['commandHz'],50,places=1)
        self.assertAlmostEqual(result['commandMaxGapMs'],20,places=1)
    def test_slow_serial_reports_actual_frequency_not_target(self):
        original = self.bus.sync
        def slow(bus, address, values):
            if address == 42: self.clock.sleep(.03)
            original(bus, address, values)
        with patch.object(c, 'sync_write', slow):
            result = self.run_control()
        self.assertEqual(result['commandTargetHz'],50)
        self.assertLess(result['commandHz'],35)
        self.assertGreaterEqual(result['commandMaxGapMs'],30)
    def test_enable_aligns_goal_as_first_write_and_holds_all_fifteen(self):
        for f in self.bus.rows.values(): f['target'] = 0
        self.assertEqual(self.run_control('enable')['state'],'enabled')
        self.assertEqual(self.bus.writes[0][0],42)
        self.assertTrue(all(not address <= 44 < address+len(data) for address,values in self.bus.writes for data in values.values()))
        self.assertTrue(all(f['torque']==1 and f['position']==2048 for f in self.bus.rows.values()))
        self.assertTrue(all(address >= 40 for address,_ in self.bus.writes))
    def test_missing_reference_or_missing_feedback_causes_no_writes(self):
        del self.cal['joints']['references']['12']
        with self.assertRaises(ValueError): self.run_control()
        self.assertEqual(self.bus.writes,[])
        self.bus.drop = 10
        with self.assertRaises(ValueError): self.run_control('enable')
        self.assertEqual(self.bus.writes,[])
    def test_cancel_and_final_timeout_end_with_torque_off(self):
        for stuck in (False,True):
            self.bus.stuck = stuck
            with self.assertRaises(ValueError): self.run_control(cancelled=lambda:self.clock.now > 1 if not stuck else False)
            self.assertEqual(self.bus.writes[-1][0],40)
            self.assertTrue(all(f['torque']==0 for f in self.bus.rows.values()))
    def test_slow_preflight_does_not_write_or_claim_three_second_success(self):
        old = self.bus.read
        def read(*args): self.clock.sleep(.1); return old(*args)
        with patch.object(c,'read_register',read), self.assertRaisesRegex(ValueError,'过慢'):
            self.run_control()
        self.assertEqual(self.bus.writes,[])
    def test_offline_off_is_unconfirmed_not_success(self):
        self.bus.drop = 24
        result = c.torque_off(self.bus)
        self.assertEqual(result['state'],'failed'); self.assertEqual(result['unconfirmedIds'],[24])
    def test_large_legal_travel_is_not_rejected_by_speed_or_distance(self):
        # Head yaw starts at -131.8 degrees: within the official +/-170 range.
        self.cal['joints']['references']['32'] = 548
        self.assertEqual(self.run_control()['state'], 'holding')
        self.assertEqual(self.bus.rows[32]['position'], 548)
        self.assertGreater(2048-548, 1024)
    def test_official_angle_bounds_reject_invalid_target_before_any_write(self):
        with patch.dict(c.STAND, {32: math.radians(171)}):
            with self.assertRaisesRegex(ValueError, '关节角度范围'):
                self.run_control()
        self.assertEqual(self.bus.writes, [])
    def test_official_angle_boundaries_are_inclusive(self):
        for sid, bounds in c.OFFICIAL_LIMITS_DEG.items():
            for angle in bounds:
                with self.subTest(sid=sid, angle=angle), patch.dict(c.STAND, {sid: math.radians(angle)}):
                    c.target_for(sid, 2048, self.cal, 0, 4095)
    def test_temperature_alone_has_no_arbitrary_cutoff_but_fault_still_rejects(self):
        f = dict(self.bus.rows[30], temperature=53)
        c.validate_feedback(30, f)
        self.assertEqual(f['temperature'], 53)
        f['fault'] = 4
        with self.assertRaisesRegex(ValueError, '故障码 4'):
            c.validate_feedback(30, f)
    def test_feedback_protection_reports_each_actual_trigger(self):
        f = dict(self.bus.rows[30], fault=4, voltage=3.9, temperature=49, load=-301)
        with self.assertRaises(ValueError) as caught: c.validate_feedback(30, f)
        self.assertIn('故障码 4', str(caught.exception))
        self.assertIn('3.9V', str(caught.exception))
        self.assertNotIn('负载', str(caught.exception))
        self.assertNotIn('温度', str(caught.exception))
    def test_voltage_uses_full_hd1910_working_range(self):
        for voltage in (4.0, 5.9, 6.0, 8.4):
            with self.subTest(voltage=voltage):
                c.validate_feedback(10, dict(self.bus.rows[10], voltage=voltage))
        for voltage in (3.9, 8.5):
            with self.subTest(voltage=voltage), self.assertRaisesRegex(ValueError, '4.0–8.4V'):
                c.validate_feedback(10, dict(self.bus.rows[10], voltage=voltage))
    def test_final_residual_error_times_out_instead_of_claiming_hold(self):
        original = self.bus.sync
        def lagging(bus, address, values):
            original(bus, address, values)
            if address == 42:
                if self.clock.now >= 2.2:
                    for sid in values: self.bus.rows[sid]['position'] -= 57
        with patch.object(c, 'sync_write', lagging), self.assertRaisesRegex(ValueError,'3 秒') as caught:
            self.run_control()
        self.assertIn('差57步', str(caught.exception))
        self.assertIn('输出上限1000', str(caught.exception))
        self.assertIn('最后反馈', str(caught.exception))
        self.assertLess(self.clock.now,3.1)
        self.assertTrue(all(f['torque']==0 for f in self.bus.rows.values()))
    def test_arrival_accepts_56_ticks_without_mid_motion_tracking_trip(self):
        original = self.bus.sync
        def lagging(bus, address, values):
            original(bus, address, values)
            if address == 42 and self.clock.now >= 2.2:
                for sid in values: self.bus.rows[sid]['position'] -= 56
        with patch.object(c, 'sync_write', lagging):
            self.assertEqual(self.run_control()['state'], 'holding')
        self.assertTrue(all(self.bus.rows[sid]['torque'] == 1 for sid in c.STAND))
        f = dict(self.bus.rows[10], position=2000, torque=1)
        c.validate_feedback(10, f, 2055)
        c.validate_feedback(10, f, 2056)
        c.validate_feedback(10, dict(f, load=900), 2300)
    def test_transient_lag_and_high_load_can_catch_up_and_hold(self):
        original = self.bus.sync
        def lagging(bus, address, values):
            original(bus, address, values)
            if address == 42 and 0 < self.clock.now < 2.2:
                for sid in values:
                    self.bus.rows[sid]['position'] -= 150
                    self.bus.rows[sid]['load'] = 800
        with patch.object(c, 'sync_write', lagging):
            self.assertEqual(self.run_control()['state'], 'holding')
        self.assertTrue(all(data == b'\x00' for address, values in self.bus.writes if address == 41 for data in values.values()))
        self.assertFalse(any(address <= 44 < address+len(data) for address, values in self.bus.writes for data in values.values()))
    def test_preflight_uses_fresh_position_without_drift_or_old_target_gate(self):
        self.bus.rows[10].update(torque=1, target=2300)
        original = self.bus.read_feedback_many
        reads = 0
        def drift(ids):
            nonlocal reads
            reads += 1
            if reads == 2: self.bus.rows[10]['position'] += 20
            return original(ids)
        with patch.object(self.bus, 'read_feedback_many', drift):
            self.assertEqual(self.run_control('enable')['state'], 'enabled')
        self.assertEqual(int.from_bytes(self.bus.writes[0][1][10], 'little'), 2068)
    def test_hardware_fault_during_motion_still_unloads(self):
        original = self.bus.sync
        def fault(bus, address, values):
            original(bus, address, values)
            if address == 42 and self.clock.now > .5: self.bus.rows[13]['fault'] = 8
        with patch.object(c, 'sync_write', fault), self.assertRaisesRegex(ValueError, '故障码 8'):
            self.run_control()
        self.assertTrue(all(f['torque'] == 0 for f in self.bus.rows.values()))
    def test_encoder_nearest_branch_rejects_wrap_across_hardware_limit(self):
        self.cal['joints']['references']['10'] = 79
        with self.assertRaises(ValueError): c.target_for(10,4052,self.cal,0,4095)
        self.assertEqual(c.target_for(10,4052,self.cal,0,0),4175)
        self.assertEqual(c.goal_bytes(-145),bytes([145,128]))
    def test_delayed_queued_move_is_cancelled_even_after_off_flag_cleared(self):
        commands, results = queue.Queue(), queue.Queue()
        commands.put(dict(action='stand',issued=1))
        poller = ServoPoller('fake',ALL_IDS,queue.Queue(),threading.Event(),commands=commands,results=results,
                            off=threading.Event(),cancelled_before=SimpleNamespace(value=2))
        poller.execute_command(self.bus)
        self.assertFalse(results.get()[0]); self.assertEqual(self.bus.writes,[])

    def angle(self, sid=13, angle=30, cancelled=lambda: False):
        return c.execute_control(self.bus, dict(action='angle', id=sid, angleDeg=angle, calibration=self.cal),
                                 cancelled, self.frames.append, self.statuses.append)
    def test_angle_only_writes_selected_target_and_enable(self):
        result = self.angle()
        self.assertEqual(result['state'], 'commanded')
        self.assertEqual(self.bus.rows[13]['position'], round(2048-30*4096/360))
        self.assertEqual([a for a, _ in self.bus.writes], [42,40])
        self.assertTrue(all(set(v) == {13} for _,v in self.bus.writes))
        self.assertTrue(all(self.bus.rows[i]['torque'] == 0 for i in ALL_IDS if i != 13))
    def test_profile_clears_old_mouth_settings_without_goal_or_torque_write(self):
        self.bus.rows[34].update(accelerationRaw=5, speedLimitRaw=300)
        result = c.execute_control(self.bus, dict(action='profile', id=34), lambda:False, self.frames.append, self.statuses.append)
        self.assertEqual(result['state'], 'configured')
        self.assertEqual([a for a,_ in self.bus.writes], [41,46])
        self.assertTrue(all(set(v)=={34} for _,v in self.bus.writes))
        self.assertEqual(self.bus.rows[34]['position'], 2048)
        self.assertEqual(self.bus.rows[34]['torque'], 0)
    def test_angle_removes_legacy_profile_before_enabling(self):
        self.bus.rows[34].update(accelerationRaw=5, speedLimitRaw=300)
        self.angle(34, 10)
        self.assertEqual([a for a,_ in self.bus.writes], [42,41,46,40])
        self.assertEqual(self.bus.rows[34]['accelerationRaw'],0)
        self.assertEqual(self.bus.rows[34]['speedLimitRaw'],500)
    def test_angle_rejects_invalid_or_unverified_ranges_without_write(self):
        for sid, angle in [(13,91),(10,-31),(34,-.1),(34,30.1),(35,0),(True,0),(13,True),(13,float('nan'))]:
            with self.subTest(sid=sid,angle=angle), self.assertRaises(ValueError): self.angle(sid,angle)
        self.assertEqual(self.bus.writes, [])
    def test_mouth_user_range_accepts_both_endpoints(self):
        for angle in (0, 30):
            self.assertEqual(self.angle(34, angle)['state'], 'commanded')
            self.assertEqual(self.bus.rows[34]['position'], round(2048-angle*4096/360))
    def test_angle_uses_calibration_direction_and_never_wraps_requested_angle(self):
        self.cal['joints']['directions']['32'] = 1
        self.bus.rows[32]['position'] = 113
        self.angle(32,170)
        self.assertEqual(self.bus.rows[32]['position'], round(2048+170*4096/360))
    def test_angle_does_not_wait_or_reject_position_lag(self):
        self.bus.stuck = True
        self.assertEqual(self.angle()['state'], 'commanded')
        self.assertEqual(self.bus.rows[13]['torque'], 1)
    def test_angle_cancel_after_target_unloads(self):
        with self.assertRaises(ValueError): self.angle(cancelled=lambda: bool(self.bus.writes))
        self.assertEqual(self.bus.writes[-1][0], 40)
        self.assertTrue(all(f['torque'] == 0 for f in self.bus.rows.values()))


class ApiTests(unittest.TestCase):
    def setUp(self):
        import server
        self.server = server; self.client = TestClient(server.app)
        self.source = SimpleNamespace(submit_control=self.submit, submit_off=self.off)
        self.calls = []
        p = patch.object(server,'servos',self.source); p.start(); self.addCleanup(p.stop)
        p = patch.object(server.calibrations,'read',return_value=dict(revision=9,joints=dict(references={str(sid):2048 for sid in ALL_IDS})))
        p.start(); self.addCleanup(p.stop)
    def submit(self, command):
        self.calls.append(command); f=Future(); f.set_result(dict(state='holding',message='ok')); return f
    def off(self): return self.submit(dict(action='disable'))
    def test_stand_requires_confirmation_and_matching_revision(self):
        for body in ({},{'confirmCalibration':True,'revision':8}):
            self.assertEqual(self.client.post('/api/v1/servos/stand',json=body).status_code,409)
        self.assertEqual(self.calls,[])
        self.assertEqual(self.client.post('/api/v1/servos/stand',json=dict(confirmCalibration=True,revision=9)).status_code,200)
    def test_off_bypasses_calibration_lock_but_enable_does_not(self):
        with patch.object(self.server.pose_calibration.lock,'locked',return_value=True):
            self.assertEqual(self.client.post('/api/v1/servos/enable',json={}).status_code,409)
            self.assertEqual(self.client.post('/api/v1/servos/disable',json={}).status_code,200)
        self.assertEqual([x['action'] for x in self.calls],['disable'])
    def test_cross_origin_is_rejected(self):
        self.assertEqual(self.client.post('/api/v1/servos/enable',json={},headers={'Origin':'http://evil.example'}).status_code,403)
        self.assertEqual(self.calls,[])
    def test_angle_api_validates_bounds_and_revision_before_queue(self):
        for body in [dict(id=13,angleDeg=91,revision=9),dict(id=13,angleDeg=30,revision=8),dict(id=34,angleDeg=31,revision=9)]:
            self.assertEqual(self.client.post('/api/v1/servos/angle',json=body).status_code,409)
        self.assertEqual(self.calls, [])
        self.assertEqual(self.client.post('/api/v1/servos/angle',json=dict(id=13,angleDeg=30,revision=9)).status_code,200)
        self.assertEqual(self.calls[0]['id'],13)
        self.assertEqual(self.calls[0]['angleDeg'],30)
        self.assertEqual(self.client.get('/api/v1/servos/limits').json()['limits']['13'],[-90,90])
