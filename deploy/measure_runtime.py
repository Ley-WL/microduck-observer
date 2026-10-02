"""Read-only WS/CPU benchmark on Linux. Does not call any action endpoint."""
import argparse
import asyncio
from collections import Counter
import json
import os
from pathlib import Path
import statistics
import time
from urllib.request import urlopen

import websockets

parser = argparse.ArgumentParser()
parser.add_argument('--url', default='http://127.0.0.1:8877')
parser.add_argument('--pid', type=int, required=True)
parser.add_argument('--seconds', type=float, default=20)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()


def processes(pid):
    children = Path(f'/proc/{pid}/task/{pid}/children')
    result = [pid]
    for child in children.read_text().split() if children.exists() else []:
        result.extend(processes(int(child)))
    return result


def ticks(pids):
    total = 0
    for pid in pids:
        try:
            data = Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()
            total += int(data[11]) + int(data[12])
        except FileNotFoundError:
            pass
    return total


def board():
    result = {}
    for key, path, factor in [('temperatureC', '/sys/class/thermal/thermal_zone0/temp', .001),
                              ('cpuFrequencyMhz', '/sys/devices/system/cpu/cpufreq/policy0/scaling_cur_freq', .001)]:
        try:
            result[key] = int(Path(path).read_text()) * factor
        except (OSError, ValueError):
            pass
    return result


def stats(values):
    if not values:
        return {}
    values = sorted(values)
    return {'count': len(values), 'mean': statistics.mean(values),
            'p50': statistics.median(values), 'p95': values[int((len(values)-1)*.95)],
            'max': values[-1]}


async def main():
    with urlopen(args.url+'/api/v1/snapshot', timeout=3) as response:
        before = json.load(response)
    pids = processes(args.pid)
    start_board = board()
    start_ticks = ticks(pids)
    started = time.monotonic()
    counts = Counter()
    scans, ages = [], []
    complete, missing, frames = 0, Counter(), 0
    torque_before = {r['id']: r['torque'] for r in before['joints']['data']['servos'] if r['online']}
    async with websockets.connect(args.url.replace('http', 'ws', 1)+'/api/v1/stream') as ws:
        await ws.send(json.dumps({'type': 'subscribe', 'topics': {'joints': 50, 'imu.raw': 50,
                                                               'imu.orientation': 50, 'system': 1}}))
        while time.monotonic()-started < args.seconds:
            sample = json.loads(await asyncio.wait_for(ws.recv(), 4))
            if sample['type'] != 'sample':
                continue
            counts[sample['topic']] += 1
            if sample['topic'] == 'joints':
                frames += 1
                offline = [r['id'] for r in sample['data']['servos'] if not r['online']]
                missing.update(offline)
                complete += not offline
                hz = sample['data'].get('diagnostics', {}).get('observedScanHz')
                if hz is not None:
                    scans.append(hz)
                ages.append(sample['ageMs']+max((r.get('ageMs', 0) for r in sample['data']['servos']), default=0))
    elapsed = time.monotonic()-started
    cpu = (ticks(pids)-start_ticks)/os.sysconf('SC_CLK_TCK')/elapsed*100
    with urlopen(args.url+'/api/v1/snapshot', timeout=3) as response:
        after = json.load(response)
    result = {'elapsedSeconds': elapsed, 'pids': pids, 'cpuPercentOneCore': cpu,
              'boardBefore': start_board, 'boardAfter': board(),
              'received': dict(counts), 'receivedHz': {k: v/elapsed for k, v in counts.items()},
              'observedSerialScanHz': stats(scans), 'jointOldestAgeMs': stats(ages),
              'sampledJointFrames': frames, 'completeSampledJointFrames': complete,
              'missingSampledRowsById': dict(missing), 'torqueBefore': torque_before,
              'torqueAfter': {r['id']: r['torque'] for r in after['joints']['data']['servos'] if r['online']}}
    args.output.write_text(json.dumps(result, indent=2), encoding='utf-8')
    print(json.dumps(result, ensure_ascii=False))


asyncio.run(main())
