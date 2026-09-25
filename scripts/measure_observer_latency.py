"""Measure only GET snapshots; never access hardware directly."""
import urllib.request,time,json,statistics,pathlib,argparse
p=argparse.ArgumentParser(description="Read-only observer latency measurement")
p.add_argument("--endpoint",default="http://127.0.0.1:8877")
p.add_argument("--seconds",type=float,default=30)
p.add_argument("--output",type=pathlib.Path,required=True)
args=p.parse_args()
if not 0 < args.seconds <= 3600: p.error("seconds must be in (0,3600]")
r=[];ages=[];imu=[];seq={};scans=[];read_ms=[];err=[];missing=0;t=time.monotonic()
while time.monotonic()-t<args.seconds:
 a=time.monotonic()
 try:
  s=json.load(urllib.request.urlopen(args.endpoint.rstrip('/')+'/api/v1/snapshot',timeout=1));r.append((time.monotonic()-a)*1000);j=s['joints'];ages.append(max(x['ageMs'] for x in j['data']['servos'])+j['ageMs']);imu.append(s['imu.raw']['ageMs'])
  if j['seq'] not in seq:
   if j['data'].get('scanMs') is not None:scans.append(j['data']['scanMs'])
   read_ms.extend(x['readMs'] for x in j['data']['servos'] if x.get('readMs') is not None)
   seq[j['seq']]=j['sampleMonoMs'];missing+=sum(not x['online'] for x in j['data']['servos'])
 except Exception as e:err.append(type(e).__name__)
 time.sleep(max(0,.01-(time.monotonic()-a)))
def stats(x):
 y=sorted(x);return dict(n=len(y),p50=round(statistics.median(y),2),p95=round(y[int(.95*(len(y)-1))],2),max=round(max(y),2)) if y else {}
result=dict(request_ms=stats(r),joint_oldest_ms=stats(ages),imu_ms=stats(imu),scan_ms=stats(scans),servo_read_ms=stats(read_ms),unique_joint_frames=len(seq),duration=time.monotonic()-t,errors=err,missing_servo_rows=missing,source_frame_hz=(max(seq)-min(seq))/((seq[max(seq)]-seq[min(seq)])/1000) if len(seq)>1 else 0)
print(json.dumps(result,indent=2));args.output.write_text(json.dumps(result,indent=2))
