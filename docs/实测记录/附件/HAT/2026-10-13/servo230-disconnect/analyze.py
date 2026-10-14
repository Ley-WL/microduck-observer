import json,hashlib,datetime
from pathlib import Path

root=Path(__file__).resolve().parent
failure=json.loads((root/'policy-failure-latest.json').read_text(encoding='utf-8'))
read=failure['failedRead'];confirm=read['faultConfirmation']
row=confirm['feedback']['30']
rx=bytes.fromhex(confirm['read']['rxHex'])
assert rx[2]==30 and rx[4]==8 and sum(rx[2:])%256==255
assert read['missingIds']==[] and read['checksumErrors']==0
assert confirm['read']['missingIds']==[] and confirm['read']['checksumErrors']==0
stats=failure['stats']
result=dict(
    eventTime=datetime.datetime.fromtimestamp(failure['eventTimeMs']/1000,datetime.timezone(datetime.timedelta(hours=8))).isoformat(),
    failure=failure['error'],stage=failure['stage'],activeSkill=stats['activeSkill'],
    calibratedRevision=failure['calibrationRevision'],
    lastRunGains=dict(p=stats['kpRun'],d=stats['kdRun']),
    originalPacket=dict(receivedIds=read['receivedIds'],rxBytes=read['rxBytes'],checksumErrors=read['checksumErrors'],missingIds=read['missingIds']),
    confirmedPacket=dict(id=30,fault=8,checksumValid=True,readMs=confirm['read']['elapsedMs'],rawHex=confirm['read']['rxHex']),
    neckAtFault=dict(positionRaw=row['position'],targetRaw=row['target'],gapTicks=row['target']-row['position'],
                     gapDegrees=(row['target']-row['position'])*360/4096,velocityRaw=row['velocityRaw'],
                     loadRaw=row['load'],currentRaw=row['currentRaw'],voltage=row['voltage'],temperatureC=row['temperature'],
                     rawModelTargetDegrees=stats['rawTargetDegrees'][5],sentTargetDegrees=stats['sentTargetDegrees'][5]),
    cycleCosts=stats['cycleCosts'],unload=failure['unload'],
    evidenceHashes={p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in root.glob('*.json') if p.name!='analysis.json'},
    hardwareMotionAdded=False,
    inference='Latest saved action stopped on a confirmed hardware fault from neck30, not serial loss. Stalled loaded neck observed; contact/assembly/target compatibility not isolated.',
)
if (root/'passive30.json').exists():
    passive=json.loads((root/'passive30.json').read_text(encoding='utf-8'))
    result['currentIdleMeasurement']=passive
    result['currentIdleIncompleteFraction']=1-passive['completeSampledJointFrames']/passive['sampledJointFrames']
result['lastLiveAllMissingSnapshot']=dict(rxBytes=0,checksumErrors=0,receivedIds=[],missingIds=[10,11,12,13,14,20,21,22,23,24,30,31,32,33,34],
    elapsedMs=31.99497,replyBudgetMs=30,discardedBeforeRead=0,pendingHex='',
    controlState='failed',controlAction='zero',controlError='#10 反馈丢失；预检查未通过，未发送运动指令，原使能状态不变',
    source='Live HTTP snapshot observed during this audit; not saved as complete original snapshot')
result['currentBoundary']='Many all-group no-response intervals while unloaded; real feedback absence, but power/DATA/UART scheduling source not isolated. Later SSH and HTTP timed out; not equivalent to servo-link loss.'
(root/'analysis.json').write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8')
print(json.dumps({k:v for k,v in result.items() if k not in ('currentIdleMeasurement','evidenceHashes','cycleCosts')},ensure_ascii=False,indent=2))
