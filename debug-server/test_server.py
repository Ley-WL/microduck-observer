import math
import unittest
from fastapi.testclient import TestClient
import server


class SimulatorTests(unittest.TestCase):
    def test_pause_does_not_refresh_sample(self):
        sim = server.Simulator()
        sim.tick()
        before = sim.latest['pose'].copy()
        sim.mode = 'imu_pause'
        sim.tick()
        self.assertEqual(before, sim.latest['pose'])

    def test_unit_quaternion_and_invalid_scenario(self):
        sim = server.Simulator()
        sim.tick()
        self.assertAlmostEqual(math.hypot(*sim.latest['pose']['data']['quaternion']), 1)
        sim.mode = 'invalid'
        sim.tick()
        self.assertFalse(sim.latest['pose']['valid'])

    def test_bounded_log_history_and_gap(self):
        original = server.sim
        try:
            server.sim = server.Simulator()
            for i in range(2100):
                server.sim.log('INFO', 'test', str(i))
            self.assertEqual(len(server.sim.logs), 2000)
            result = server.logs(cursor=1, limit=20)
            self.assertTrue(result['gap'])
            self.assertEqual(len(result['items']), 20)
        finally:
            server.sim = original

    def test_http_and_websocket_contract(self):
        with TestClient(server.app) as client:
            self.assertEqual(client.get('/api/v1/info').json()['source'], 'simulation')
            self.assertEqual(client.post('/api/v1/debug/scenario', json={'name':'unknown'}).status_code, 422)
            self.assertEqual(client.get('/api/v1/logs?limit=9999').status_code, 422)
            with client.websocket_connect('/api/v1/stream') as ws:
                ws.send_json({'type':'subscribe','topics':{'pose':1000,'logs':None,'nonexistent':10}})
                ack = ws.receive_json()
                self.assertEqual(ack['topics'], {'pose':50, 'logs':None})
                messages = [ws.receive_json() for _ in range(3)]
                sample = next(m for m in messages if m['type']=='sample')
                self.assertEqual(sample['protocolVersion'], 1)
                self.assertEqual(sample['source'], 'simulation')
                self.assertGreaterEqual(sample['ageMs'], 0)


if __name__ == '__main__':
    unittest.main()
