"""Supported robot: exercise the deployed HTTP angle path, not an independent serial implementation."""
import collections,json,threading,time,urllib.request,urllib.error
import sys
from pathlib import Path

base='http://127.0.0.1:8877/api/v1/'
output=Path('/home/radxa/angle-feedback-acceptance-io-cycle.json' if '--io-cycle' in sys.argv else '/home/radxa/angle-feedback-acceptance.json')
def get():return json.load(urllib.request.urlopen(base+'snapshot',timeout=4))
def post(action,body):
    request=urllib.request.Request(base+'servos/'+action,data=json.dumps(body).encode(),headers={'Content-Type':'application/json'},method='POST')
    try:
        response=urllib.request.urlopen(request,timeout=8)
        return {'status':response.status,'result':json.load(response)}
    except urllib.error.HTTPError as error:
        return {'status':error.code,'result':json.load(error)}

report={'commands':[],'samples':[],'test':'platform single23 +/-1 degree, 3 rounds x 10 requests'}
stop=threading.Event()
def collect():
    last=None
    while not stop.is_set():
        try:
            snap=get();row=snap['joints']
            if row['seq']!=last:
                last=row['seq'];report['samples'].append(row)
        except Exception as error:report.setdefault('collectorErrors',[]).append(str(error))
        stop.wait(.02)

started=False
try:
    before=get();report['before']=before
    assert before['system']['data']['servoControl']['state'] not in ('policy','moving','preflight','disabling')
    rows=before['joints']['data']['servos']
    assert len(rows)==15 and all(r['online'] and r['fault']==0 and 4<=r['voltage']<=8.4 and r['torque']==0 for r in rows)
    servo=next(r for r in rows if r['id']==23)
    calibration=before['calibration'];revision=calibration['revision']
    direction=calibration['joints']['directions'].get('23',-1)
    center=(servo['position']-calibration['joints']['references']['23'])*direction*360/4096
    report['centerAngleDeg']=center
    thread=threading.Thread(target=collect,daemon=True);thread.start()
    started=True
    for repetition in range(3):
        begin=time.monotonic()
        for n in range(10):
            time.sleep(max(0,begin+n*.2-time.monotonic()))
            angle=center+(1 if n%2==0 else -1)
            at=time.monotonic();response=post('angle',{'id':23,'angleDeg':angle,'revision':revision})
            report['commands'].append({'round':repetition+1,'angleDeg':angle,'mono':at,'durationMs':(time.monotonic()-at)*1000,**response})
            assert response['status']==200, str(response)
            assert response['result'].get('feedbackConfirmed') is True, 'Feedback unconfirmed: stop repeated motion'
            snap=get()
            assert snap['calibration']['revision']==revision
            assert all(r['fault']==0 and 4<=r['voltage']<=8.4 for r in snap['joints']['data']['servos'] if r['online'])
        print(json.dumps({'round':repetition+1,'commands':10,'confirmed':True}),flush=True)
    report['restorePosition']=post('angle',{'id':23,'angleDeg':center,'revision':revision})
    assert report['restorePosition']['status']==200
    time.sleep(.3)
except Exception as error:
    report['error']=str(error) or repr(error)
finally:
    if started:
        report['disable']=post('disable',{})
        time.sleep(.3)
        stop.set();thread.join(timeout=5)
    report['after']=get()
    missing=collections.Counter();bad=0;positions=[]
    for sample in report['samples']:
        data=sample['data'];missing.update(r['id'] for r in data['servos'] if not r['online'])
        bad+=data['diagnostics'].get('checksumErrors',0)
        positions.extend(r['position'] for r in data['servos'] if r['id']==23 and r['online'])
    times=[command['mono'] for command in report['commands']]
    report['summary']={'commands':len(times),'confirmed':sum(c['result'].get('feedbackConfirmed') is True for c in report['commands']),
                       'requestHz':(len(times)-1)/(times[-1]-times[0]) if len(times)>1 else None,
                       'sampledFrames':len(report['samples']),'missingById':dict(missing),'checksumErrorsInSampledFrames':bad,
                       'position23Range':[min(positions),max(positions)] if positions else None,
                       'allDisabled':all(r['online'] and r['torque']==0 for r in report['after']['joints']['data']['servos']),
                       'preflightReadLengths':dict(collections.Counter(c['result'].get('preflightReadLength') for c in report['commands'])),
                       'configurationCacheHits':sum(c['result'].get('configurationCached') is True for c in report['commands']),
                       'error':report.get('error')}
    output.write_text(json.dumps(report))
    print(json.dumps(report['summary']),flush=True)
if 'error' in report:raise SystemExit(1)
