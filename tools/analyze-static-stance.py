import json,math,statistics
import sys
from pathlib import Path
root=Path(__file__).resolve().parents[1]
directory=root/'docs/实测记录/附件/HAT/2026-10-04/static-stance-supported'
report=json.loads((directory/('observation-after-reference.json' if '--after-reference' in sys.argv else 'observation.json')).read_text(encoding='utf-8'))
def rotation(q):
    x,y,z,w=q;n=math.sqrt(x*x+y*y+z*z+w*w);x,y,z,w=[v/n for v in q]
    return [[1-2*(y*y+z*z),2*(x*y-z*w),2*(x*z+y*w)],
            [2*(x*y+z*w),1-2*(x*x+z*z),2*(y*z-x*w)],
            [2*(x*z-y*w),2*(y*z+x*w),1-2*(x*x+y*y)]]
def transpose(m):return [list(v) for v in zip(*m)]
def multiply(a,b):return [[sum(a[i][k]*b[k][j] for k in range(3)) for j in range(3)] for i in range(3)]
rows=[]
for snapshot in report['samples']:
    cal=snapshot['calibration']['imu'];mounting=rotation(cal['mountingQuaternion'])
    relative=multiply(transpose(rotation(cal['quaternion'])),rotation(snapshot['imu.orientation']['data']['quaternion']))
    body=multiply(multiply(multiply(rotation(cal.get('targetQuaternion',[0,0,0,1])),transpose(mounting)),relative),mounting)
    gravity=[-body[2][i] for i in range(3)]
    rows.append({'rollDeg':math.degrees(math.atan2(-gravity[1],-gravity[2])),
                 'pitchDeg':math.degrees(math.atan2(gravity[0],math.hypot(gravity[1],gravity[2]))),
                 'gravity':gravity})
summary={'imuBody':{key:{'mean':statistics.mean(row[key] for row in rows),'min':min(row[key] for row in rows),'max':max(row[key] for row in rows)} for key in ['rollDeg','pitchDeg']},
         'projectedGravityMean':[statistics.mean(row['gravity'][n] for row in rows) for n in range(3)],
         'maxEnabledGoalErrorDeg':max(joint['maxGoalErrorDeg'] for id,joint in report['summary']['joints'].items() if id!='34'),
         'userObservation':'User reports static stance basically motionless while supported; not a free-standing balance acceptance'}
(directory/('analysis-after-reference.json' if '--after-reference' in sys.argv else 'analysis.json')).write_text(json.dumps(summary,indent=2),encoding='utf-8')
print(json.dumps(summary))
