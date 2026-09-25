import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch
from calibrations import CalibrationStore, Conflict

class CalibrationTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup)
        self.path=Path(self.temp.name)/'calibration.json';self.store=CalibrationStore(self.path)
    def joints(self, position=4971):return {'joints':{'references':{'12':position},'directions':{'12':-1}}}
    def test_restart_and_other_client(self):
        initial=self.store.read('a');self.assertEqual(CalibrationStore(self.path).read('b')['deviceId'],initial['deviceId'])
        saved=self.store.update(0,self.joints(),'a')
        other=CalibrationStore(self.path).read('b');self.assertEqual(other['joints'],saved['joints']);self.assertEqual(other['revision'],1)
    def test_concurrent_revision_and_migration(self):
        self.store.update(0,self.joints(),'a',True)
        with self.assertRaises(Conflict):self.store.update(0,self.joints(6000),'a')
        with self.assertRaises(Conflict):self.store.update(1,self.joints(6000),'a',True)
        self.assertEqual(self.store.read('a')['joints']['references']['12'],4971)
    def test_clear_tombstone_and_imu_session(self):
        self.store.update(0,{'joints':{'references':{},'directions':{}},'imu':{'quaternion':[0,0,0,1],'bootId':'a','time':'12:00'}},'a')
        self.assertTrue(self.store.read('a')['imu']['validForBoot']);self.assertFalse(self.store.read('b')['imu']['validForBoot'])
        with self.assertRaises(Conflict):self.store.update(1,self.joints(),'a',True)
        with self.assertRaises(Conflict):self.store.update(1,{'imu':{'quaternion':[0,0,0,1],'bootId':'a'}},'b')
    def test_invalid_and_failed_write_keep_confirmed_state(self):
        for value in [float('nan'),float('inf'),True,2147483648]:
            with self.assertRaises(ValueError):self.store.update(0,self.joints(value),'a')
        with patch('calibrations.os.replace',side_effect=OSError('disk full')):
            with self.assertRaises(OSError):self.store.update(0,self.joints(),'a')
        self.assertEqual(self.store.read('a')['revision'],0);self.assertEqual(CalibrationStore(self.path).read('a')['revision'],0)
    def test_api_origin_and_conflict(self):
        import server
        from fastapi.testclient import TestClient
        with patch.object(server,'calibrations',self.store),TestClient(server.app) as client:
            body={'revision':0,'patch':self.joints()}
            self.assertEqual(client.post('/api/v1/calibration',json=body,headers={'Origin':'http://evil.example'}).status_code,403)
            self.assertEqual(client.post('/api/v1/calibration',json=body,headers={'Origin':'http://testserver'}).status_code,200)
            self.assertEqual(client.post('/api/v1/calibration',json=body).status_code,409)
            self.assertEqual(client.get('/api/v1/calibration').json()['joints']['references']['12'],4971)
            self.assertEqual(client.get('/api/v1/snapshot').json()['calibration']['revision'],1)

    def test_mounting_and_reference_persist_without_changing_joints(self):
        self.store.update(0, {**self.joints(), 'imu': {'quaternion':[0,0,0,1], 'bootId':'a','time':'12:00'}}, 'a')
        self.store.update(1, {'mounting':{'yaw':90}}, 'a')
        restarted=CalibrationStore(self.path)
        saved=restarted.read('b')
        self.assertEqual(saved['mounting']['yaw'],90)
        self.assertEqual(saved['imu']['quaternion'],[0,0,0,1])
        self.assertFalse(saved['imu']['validForBoot'])
        confirmed=restarted.update(2, {'imu':{'quaternion':saved['imu']['quaternion'],'bootId':'b','time':'12:00'}}, 'b')
        self.assertTrue(confirmed['imu']['validForBoot'])
        self.assertEqual(confirmed['joints'],saved['joints'])
        self.assertEqual(confirmed['mounting'],saved['mounting'])
        for value in (True,91,float('nan')):
            with self.assertRaises(ValueError):restarted.update(3,{'mounting':{'yaw':value}},'b')
