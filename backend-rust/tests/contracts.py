"""Rust HTTP/WS contract checks against an isolated simulation service.

Run after cargo build: python tests/contracts.py
No hardware, Bluetooth or existing calibration state is accessed.
"""
import argparse
import asyncio
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request

import websockets

# Local contract checks must never be sent through the host's network proxy.
urllib.request.install_opener(urllib.request.build_opener(urllib.request.ProxyHandler({})))


def request(port, path, body=None, headers=None, raw=None):
    data = raw if raw is not None else (json.dumps(body).encode() if body is not None else None)
    h = {'Content-Type': 'application/json', **(headers or {})}
    r = urllib.request.Request(f'http://127.0.0.1:{port}{path}', data=data, headers=h)
    try:
        with urllib.request.urlopen(r, timeout=5) as response:
            return response.status, json.load(response)
    except urllib.error.HTTPError as e:
        payload = e.read()
        try:
            payload = json.loads(payload)
        except json.JSONDecodeError:
            payload = payload.decode()
        return e.code, payload


def wait(port, process):
    for _ in range(100):
        if process.poll() is not None:
            raise RuntimeError(f'server exited: {process.returncode}')
        try:
            if request(port, '/api/v1/health')[0] == 200:
                return
        except OSError:
            pass
        time.sleep(.05)
    raise TimeoutError('server did not start')


async def websocket(port):
    async with websockets.connect(f'ws://127.0.0.1:{port}/api/v1/stream', proxy=None) as ws:
        await ws.send(json.dumps({'type': 'subscribe', 'requestId': 'parity', 'topics': {
            'pose': 999, 'imu.raw': 20, 'system': 1, 'logs': None,
            'unknown': 20, 'imu.orientation': 50}}))
        ack = json.loads(await asyncio.wait_for(ws.recv(), 3))
        assert ack['type'] == 'subscribed' and ack['requestId'] == 'parity'
        assert ack['topics'] == {'pose': 50, 'imu.raw': 20, 'system': 1, 'logs': None}
        received = set()
        deadline = time.monotonic() + 3
        while time.monotonic() < deadline and len(received) < 5:
            value = json.loads(await asyncio.wait_for(ws.recv(), 3))
            if value['type'] == 'sample':
                assert set(value) == {'type', 'protocolVersion', 'bootId', 'topic', 'seq',
                                      'sampleMonoMs', 'source', 'valid', 'data', 'ageMs'}
                assert value['protocolVersion'] == 1 and value['bootId'] == ack['bootId']
                assert value['source'] == 'simulation' and value['ageMs'] >= 0
                received.add(value['topic'])
            elif value['type'] == 'log_batch':
                assert set(value) == {'type', 'items', 'gap'}
                received.add('logs')
            elif value['type'] == 'heartbeat':
                assert value['bootId'] == ack['bootId']
                received.add('heartbeat')
        assert received == {'pose', 'imu.raw', 'system', 'logs', 'heartbeat'}, received
    async with websockets.connect(f'ws://127.0.0.1:{port}/api/v1/stream',proxy=None) as ws:
        await ws.send(json.dumps({'type': 'subscribe', 'topics': []}))
        try:
            await asyncio.wait_for(ws.recv(), 3)
            raise AssertionError('invalid subscription accepted')
        except websockets.exceptions.ConnectionClosed as e:
            assert e.rcvd.code == 1008


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    binary = args.binary.resolve() if args.binary else root / ('target/debug/microduck-observer.exe' if os.name=='nt' else 'target/debug/microduck-observer')
    with tempfile.TemporaryDirectory() as directory:
        processes = []
        logs = []
        try:
            for port, command, cwd in [(18878, [str(binary)], root)]:
                env = dict(os.environ, MICRODUCK_SOURCE='simulation', MICRODUCK_BLE_ENABLED='0', MICRODUCK_SERVO_PORT='',
                           MICRODUCK_TOF_SOCKET='', MICRODUCK_CALIBRATION_FILE=f'{directory}/{port}.json',
                           MICRODUCK_BIND=f'127.0.0.1:{port}', MICRODUCK_STATIC_DIR=f'{directory}/static',
                           MICRODUCK_POLICY_PATH=str(root.parent / 'models/hd1910-head-v5.onnx'))
                log = open(f'{directory}/{port}.log', 'w+')
                logs.append(log)
                process = subprocess.Popen(command, cwd=cwd, env=env, stdout=log, stderr=log)
                processes.append(process)
                wait(port, process)
            ports = (18878,)
            assert request(18878, '/api/v1/info')[0] == 200
            for path in ('/api/v1/servos/limits', '/api/v1/calibration/poses', '/api/v1/policy'):
                assert request(18878,path)[0] == 200, path
            cases = [('/api/v1/logs?cursor=-1', None, {}, None, 422),
                     ('/api/v1/logs?limit=2001', None, {}, None, 422),
                     ('/api/v1/servos/angle', {}, {}, None, 409),
                     ('/api/v1/servos/bogus', {}, {}, None, 404),
                     ('/api/v1/calibration/preview', {'pose': 'bad', 'ids': [], 'modes': []}, {}, None, 409),
                     ('/api/v1/calibration', {'revision': 0, 'patch': {}}, {}, None, 422),
                     ('/api/v1/calibration', {}, {'Origin': 'http://evil.invalid'}, None, 403),
                     ('/api/v1/calibration', {}, {'Content-Type': 'text/plain'}, None, 415),
                     ('/api/v1/calibration', None, {}, b'{', 422)]
            for path, body, headers, raw, expected in cases:
                results = [request(p, path, body, headers, raw) for p in ports]
                assert all(r[0] == expected for r in results), (path, results)
            for p in ports:
                state = request(p, '/api/v1/calibration')[1]
                patch = {'joints': {'references': {'34': 2058.608695652174}, 'directions': {'34': -1}},
                         'mounting': {'yaw': -90, 'positionMm': [-22.82198102170202, -.8570884683296031, -14.48367988571167]}}
                code, saved = request(p, '/api/v1/calibration', {'revision': 0, 'patch': patch})
                assert code == 200 and saved['revision'] == 1
                assert saved['joints']['references'] == patch['joints']['references']
                assert saved['mounting'] == patch['mounting']
                assert request(p, '/api/v1/calibration', {'revision': 0, 'patch': patch})[0] == 409
                for _ in range(21):
                    assert request(p, '/api/v1/debug/scenario', {'name': 'log_burst'})[0] == 200
                assert request(p, '/api/v1/logs?cursor=1&limit=2')[1]['gap'] is True
                assert request(p, '/api/v1/debug/scenario', {'name': 'steady'})[0] == 200
                asyncio.run(websocket(p))
                disk = json.loads(Path(f'{directory}/{p}.json').read_text())
                assert disk['revision'] == 1 and disk['mounting'] == patch['mounting']
            print('PASS: Rust HTTP contracts, revision/conflict/persistence, log gap, WS envelopes/rates/1008')
        except BaseException:
            for log in logs:
                log.flush(); log.seek(0); print(log.read()[-4000:])
            raise
        finally:
            for process in processes:
                process.terminate()
            for process in processes:
                try:
                    process.wait(5)
                except subprocess.TimeoutExpired:
                    process.kill(); process.wait()
            for log in logs:
                log.close()


if __name__ == '__main__':
    main()
