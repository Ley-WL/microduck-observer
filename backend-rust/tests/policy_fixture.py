"""Generate numerical reference using the unchanged Python policy runtime."""
import argparse
import json
import math
from pathlib import Path
import sys
import time

parser = argparse.ArgumentParser()
parser.add_argument('reference', type=Path)
parser.add_argument('output', type=Path)
args = parser.parse_args()
sys.path.insert(0, str(args.reference.resolve()))
from policy_control import StandPolicy, JOINT_IDS

model = StandPolicy(args.reference / 'models/hd1910-head-v5.onnx')
now = time.monotonic()
cal = {'joints': {'references': {str(i): 2048 for i in JOINT_IDS}, 'directions': {}},
       'imu': {'initialized': True, 'quaternion': [0, math.sin(.1), 0, math.cos(.1)],
               'mountingQuaternion': [0, 0, math.sqrt(.5), math.sqrt(.5)],
               'targetQuaternion': [0, 0, 0, 1]}}
sensors = {
    'imu.orientation': {'valid': True, 'source': 'hardware', 'bootId': 'fixture',
                        'received': now, 'data': {'quaternion': [.1, -.2, .05, .9733961166965892]}},
    'imu.raw': {'valid': True, 'source': 'hardware', 'bootId': 'fixture',
                'received': now, 'data': {'gyro': [.01, -.03, .02]}}}
feedback = {i: {'position': round(2048 - (float(model.home[n]) + .01) * 4096 / (2 * math.pi)),
                 'velocityRaw': 0, 'fault': 0, 'voltage': 5.9, 'torque': 0, 'received': now}
            for n, i in enumerate(JOINT_IDS)}
obs = model.observe(sensors, feedback, cal, now=now)
action, _ = model.infer(obs)
args.output.write_text(json.dumps({'calibration': cal, 'sensors': sensors,
                                    'feedback': feedback, 'observation': obs.tolist(),
                                    'action': action.tolist()}), encoding='utf-8')
print(f'Python fixture saved: {len(obs)} observations / {len(action)} actions')
