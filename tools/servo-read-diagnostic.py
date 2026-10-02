"""Read-only bus diagnosis. Stop observer first; never write servo registers."""
import argparse
from collections import Counter
import json
from pathlib import Path
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parents[1]/'debug-server'))
from servos import ALL_IDS, ReadOnlyBus
from pose_calibration import read_register


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--port',required=True)
    parser.add_argument('--seconds',type=float,default=6)
    args=parser.parse_args()
    if not 1<=args.seconds<=30: parser.error('seconds must be 1–30')
    bus=ReadOnlyBus(args.port)
    output=dict(mode='read-only',started=time.time(),config={},phases=[])
    orders=[list(ALL_IDS),list(reversed(ALL_IDS)),[20,21,22,23,24,30,31,32,33,10,11,12,13,14,34]]
    try:
        initial=bus.read_feedback_many(ALL_IDS)
        output['initial']=initial
        output['initialRead']=bus.last_read
        for sid in ALL_IDS:
            if sid not in initial: continue
            try:
                cfg=list(read_register(bus,sid,0,40))
                output['config'][sid]=dict(raw=cfg,firmware=cfg[:2],id=cfg[5],baudRaw=cfg[6],
                    secondaryId=cfg[7],responseLevel=cfg[8],mode=cfg[33])
            except Exception as exc: output['config'][sid]=dict(error=str(exc))
        for order in orders:
            deadline=time.monotonic()+args.seconds/3
            missing=Counter();checksums=0;latencies=[];failures=[];frames=0;latest={}
            tick=time.monotonic()
            while time.monotonic()<deadline:
                latest=bus.read_feedback_many(order);read=bus.last_read;frames+=1
                missing.update(read['missingIds']);checksums+=read['checksumErrors'];latencies.append(read['elapsedMs'])
                if read['missingIds'] and len(failures)<8: failures.append(read)
                tick=max(tick+.02,time.monotonic())
                time.sleep(max(0,tick-time.monotonic()))
            output['phases'].append(dict(ids=order,frames=frames,missingCounts=dict(missing),
                checksumErrors=checksums,scanMeanMs=sum(latencies)/len(latencies),
                scanMaxMs=max(latencies),failureSamples=failures,lastFeedback=latest))
        # Individual READ separates a broadcast ordering issue from no reply.
        output['individual']={}
        for sid in ALL_IDS:
            try: output['individual'][sid]=bus.read_feedback(sid)
            except Exception as exc: output['individual'][sid]=dict(error=str(exc))
    finally:
        bus.close()
    print(json.dumps(output,ensure_ascii=False))


if __name__=='__main__': main()
