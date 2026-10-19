"""Read-only trace and upstream-contract audit; no robot commands."""
import ast
import json
import re
from pathlib import Path
import xml.etree.ElementTree as ET

import numpy as np
import onnxruntime as ort

ROOT = Path(__file__).resolve().parents[2]
OUT = Path(__file__).resolve().parent
LIMITS = {20: (-25, 30), 21: (-22, 22), 22: (-90, 90), 23: (-90, 90),
          24: (-70, 70), 30: (-90, 70), 31: (-90, 90), 32: (-170, 170),
          33: (-20, 20), 10: (-30, 25), 11: (-22, 22), 12: (-90, 90),
          13: (-90, 90), 14: (-70, 70)}


def read(path: Path) -> dict:
    return json.loads(path.read_text(encoding='utf-8'))


def audit(path: Path, session: ort.InferenceSession) -> dict:
    data = read(path)
    frames = [f for f in data['frames'] if f.get('kind') == 'flamingo_hold' and f.get('type') == 'inference']
    assert frames and all(f['policySha256'].startswith('df77929c') for f in frames)
    ids = data['jointIds']
    obs = np.array([f['observation'] for f in frames])
    raw = np.array([f['rawActionRadians'] for f in frames])
    pre = np.array([f['preClipTargetDegrees'] for f in frames])
    sent = np.array([f['sentTargetDegrees'] for f in frames])
    times = np.array([f['mono'] for f in frames])
    elapsed = times - times[0]
    contiguous = np.diff(times) < .1
    tilt = np.degrees(np.arccos(np.clip(-obs[:, 5] / np.linalg.norm(obs[:, 3:6], axis=1), -1, 1)))
    cal = data['calibration']['joints']
    reference = np.array([cal['references'][str(i)] for i in ids])
    direction = np.array([cal['directions'].get(str(i), -1) for i in ids])
    position = np.array([[f['feedbackBeforeWrite'][str(i)]['position'] for i in ids] for f in frames])
    speed = np.array([[f['feedbackBeforeWrite'][str(i)]['velocityRaw'] for i in ids] for f in frames])
    actual = (position-reference)*360/4096*direction
    home = np.array(frames[0]['homeRadians'])
    clipped = abs(pre-sent) > .1
    bounds = np.array([LIMITS[i] for i in ids])
    expected = np.clip(pre, bounds[:, 0], bounds[:, 1])
    goals = np.rint(reference + np.radians(expected)*4096/(2*np.pi)*direction)
    decoded = (goals-reference)*360/4096*direction
    first_clip = int(np.where(clipped.any(axis=1))[0][0])
    chronology = []
    for n in sorted(set([0, first_clip, 5, 10, 15, 25, 40, 50])):
        chronology.append(dict(frame=n,seconds=float(elapsed[n]),tilt=float(tilt[n]),
            clippedIds=[ids[k] for k in np.where(clipped[n])[0]],
            clippedTargets=[dict(id=ids[k],requested=float(pre[n,k]),sent=float(sent[n,k])) for k in np.where(clipped[n])[0]],
            ankleTarget=float(sent[n,13]),ankleActual=float(actual[n,13])))
    replay_error = 0.
    speed_counterfactual = []
    for n, row in enumerate(obs):
        action = session.run(None, {'obs': row[None].astype(np.float32)})[0][0]
        replay_error = max(replay_error, float(abs(action-raw[n]).max()))
        if elapsed[n] <= 1:
            fresh = row.copy()
            fresh[20:34] = speed[n]*50*2*np.pi/4096*direction
            alternative = session.run(None, {'obs': fresh[None].astype(np.float32)})[0][0]
            speed_counterfactual.append(float(np.degrees(abs(action-alternative)).max()))
    interval = elapsed <= 1
    age = [r['ageMs'] for f in frames[:51] for r in f['feedbackBeforeWrite'].values()]
    return dict(eventTimeMs=data['eventTimeMs'],result=data['result'],frames=len(frames),
        chronology=chronology,firstClipSeconds=float(elapsed[first_clip]),
        firstClipTilt=float(tilt[first_clip]),
        observationAngleMaxErrorRad=float(abs(obs[:,6:20]-(np.radians(actual)-home)).max()),
        targetFormulaMaxErrorDeg=float(abs(pre-np.degrees((raw.astype(np.float32)+home.astype(np.float32)).astype(np.float64))).max()),
        targetEncoderMaxErrorDeg=float(abs(sent-decoded).max()),
        rawActionHistoryMaxError=float(abs(obs[1:,34:48][contiguous]-raw[:-1][contiguous]).max()),
        previousFeedbackVelocityMaxError=float(abs(obs[1:,20:34][contiguous]-(speed[:-1]*50*2*np.pi/4096*direction)[contiguous]).max()),
        onnxMaxError=replay_error,
        firstSecondFeedbackAgeP50P95MaxMs=np.percentile(age,[50,95,100]).tolist(),
        freshSpeedOneStepCounterfactualMaxJointChangeP50P95MaxDeg=np.percentile(speed_counterfactual,[50,95,100]).tolist(),
        firstSecondJoints=[dict(id=i,requestedMinMax= [float(pre[interval,k].min()),float(pre[interval,k].max())],
            clippingPercent=float(100*clipped[interval,k].mean()),
            previousTargetTrackingErrorP95Deg=float(np.percentile(abs(sent[:-1,k]-actual[1:,k])[contiguous & interval[1:]],95))) for k,i in enumerate(ids)])


def main() -> None:
    meta = read(ROOT/'models/flamingo_official.metadata.json')
    tree = ET.parse(OUT/'upstream-robot_allcollisions.xml')
    joint_nodes = {j.attrib['name']: j for j in tree.iter('joint') if 'name' in j.attrib}
    ranges = []
    for i,name in zip(LIMITS, meta['jointOrder']):
        limits = np.degrees(np.fromstring(joint_nodes[name].attrib['range'],sep=' '))
        ranges.append(dict(id=i,name=name,trainingDeg=limits.tolist(),runtimeDeg=LIMITS[i],same=bool(np.allclose(limits,LIMITS[i],atol=1e-6))))
    source = ast.parse((OUT/'upstream-microduck_constants.py').read_text(encoding='utf-8'))
    call = next(n.value for n in source.body if isinstance(n,ast.Assign) and any(isinstance(t,ast.Name) and t.id=='HOME_FRAME' for t in n.targets))
    homes = ast.literal_eval(next(k.value for k in call.keywords if k.arg=='joint_pos'))
    training_home = [next(v for pattern,v in homes.items() if re.fullmatch(pattern,name)) for name in meta['jointOrder']]
    assert np.allclose(training_home,meta['homeRadians'],atol=1e-8)
    session = ort.InferenceSession(str(ROOT/'models/flamingo_official.onnx'),providers=['CPUExecutionProvider'])
    report = dict(upstreamCommit='0bf9897',jointRanges=ranges,homeMatches=True,
        historical=audit(ROOT/'.local/flamingo-jitter/session.json',session),
        latest=audit(OUT/'session.json',session),
        limitation='Counterfactual replaces one input on recorded states only; not a physical or closed-loop improvement test. Calibration algebra does not validate physical assembly zero/direction.')
    (OUT/'audit.json').write_text(json.dumps(report,indent=2,ensure_ascii=False),encoding='utf-8')
    print(json.dumps(report,indent=2,ensure_ascii=True))


if __name__ == '__main__':
    main()
