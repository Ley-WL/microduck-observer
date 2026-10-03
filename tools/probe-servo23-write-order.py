"""Explicitly authorized small-motion #23 comparison. Run with sudo."""
import json, os, signal, subprocess, sys, time, urllib.request
from pathlib import Path
sys.path.insert(0, '/home/radxa/microduck-observer/releases/20261002-servo-scan-recovery/debug-server')
from servos import ReadOnlyBus
from pose_calibration import read_register, write_register

def sync(bus, address, data):
    p = bytes([254, len(data)+5, 0x83, address, len(data), 23]) + data
    bus.serial.write(b'\xff\xff' + p + bytes([~sum(p) & 255]))

pid = int(subprocess.check_output(['systemctl', 'show', 'microduck-observer', '-p', 'MainPID', '--value']))
with urllib.request.urlopen('http://127.0.0.1:8877/api/v1/snapshot') as f:
    snapshot = json.load(f)
assert snapshot['system']['data']['servoControl']['state'] == 'commanded'
output = {'before': snapshot['system']['data']['servoControl'], 'phases': []}
bus = None
paused = False
try:
    assert os.geteuid() == 0, 'Run with sudo: the native serial owner must release its exclusive lock'
    subprocess.run(['systemctl', 'stop', 'microduck-observer'], check=True)
    paused = True
    assert not Path(f'/proc/{pid}').exists()
    bus = ReadOnlyBus('/dev/ttyS2')
    baseline = bus.read_feedback(23)
    cfg = read_register(bus, 23, 0, 40)
    assert list(cfg[:2]) == [3, 46] and cfg[33] == 4
    assert baseline['torque'] == 1 and baseline['fault'] == 0 and 4 <= baseline['voltage'] <= 8.4
    start = baseline['position']
    low, high = int.from_bytes(cfg[9:11], 'little'), int.from_bytes(cfg[11:13], 'little')
    assert high <= low or low <= start-11 <= start <= high
    output['baseline'] = baseline
    output['config'] = list(cfg)
    def observe(name):
        rows = []
        until = time.monotonic() + .7
        while time.monotonic() < until:
            row = bus.read_feedback(23)
            rows.append(row)
            assert row['fault'] == 0 and row['torque'] == 1 and 4 <= row['voltage'] <= 8.4
            time.sleep(.02)
        output['phases'].append({'name': name, 'samples': rows})
    sync(bus, 42, (start-11).to_bytes(2, 'little'))
    observe('sync_position_only_minus_11')
    sync(bus, 40, b'\x01')
    observe('repeat_enable_after_position')
    write_register(bus, 23, 42, start.to_bytes(2, 'little'))
    observe('individual_position_only_return')
    sync(bus, 42, (start-11).to_bytes(2, 'little'))
    sync(bus, 40, b'\x01')
    observe('original_position_then_enable')
    sync(bus, 42, start.to_bytes(2, 'little'))
    observe('sync_position_only_return')
finally:
    if bus is not None:
        bus.close()
    if paused:
        subprocess.run(['systemctl', 'start', 'microduck-observer'], check=True)
    path = Path('/home/radxa/servo23-write-order-20261002.json')
    path.write_text(json.dumps(output, ensure_ascii=False))
    print(path)
    for phase in output['phases']:
        row = phase['samples'][-1]
        print(phase['name'], {k: row[k] for k in ['position', 'goalPositionRaw', 'target', 'currentRaw']})
