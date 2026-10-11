import json,hashlib
from pathlib import Path
folder=Path(__file__).resolve().parent;root=folder.parents[5]
capture=json.loads((folder/'readonly-capture.json').read_text(encoding='utf-8'))
ctrl=capture['snapshot']['system']['data']['servoControl']
rows=capture['snapshot']['joints']['data']['servos']
assert capture['limits']['source']=='kissqy-hd1910-saved-mechanical-v1'
snapshot_summaries=[]
for name in ['snapshots-first.json','snapshots-second.json','snapshots.json']:
    data=json.loads((folder/name).read_text(encoding='utf-8'))
    counts={}
    for s in data:
        c=s.get('system',{}).get('data',{}).get('servoControl',{})
        key=str((c.get('state'),c.get('activeSkill')));counts[key]=counts.get(key,0)+1
    snapshot_summaries.append({'file':name,'samples':len(data),'counts':counts})
matrix=[
 {'item':'模型/推理映射','donor':'a481d9…; HOME+raw, contract scale1, no EMA','ours':'same ONNX/contract; ARM fixture passed','verdict':'一致（倍率仅指模型基础合同，外围电压补偿另列）'},
 {'item':'机械目标限位','donor':'saved mechanical clamp then single-turn encoding','ours':'mechanical-v1 deployed; 152 saved target cases ARM verified','verdict':'已修复'},
 {'item':'控制执行时序','donor':'synchronous 50Hz tick; read -> model -> write','ours':'100Hz target write/read, asynchronous inference target50Hz; captured45.24Hz inference/59.43Hz feedback, result age7.30ms','verdict':'确定不一致；数据来自坐站/行走混合会话，不冒称新起身实测'},
 {'item':'上力时增益','donor':'hd1910.arm applies native temporary model P/D before enable','ours':'getup arm_current(MANUAL_GAINS32/40), then model P5/D20','verdict':'确定不一致'},
 {'item':'电压动作补偿','donor':'default voltage_adapt=true, nominal7.4; runtime_scale*=7.4/clamp(filtered battery,6,9.5)','ours':'fixed action scale1, no voltage correction','verdict':'与供体默认实现不同；供体本机当前开关未知，非唯一原因证明'},
 {'item':'加速度寄存器','donor':'native arm preserves register41','ours':'profiles_with_gains writes register41=0','verdict':'执行不同；供体当前寄存器值未知，若原值0效果相同'},
 {'item':'P/D/扭矩保存覆盖','donor':'saved servo-test can override; management defaults3/0/681','ours':'observed model joints5/20/I0/torque1000','verdict':'本机确定；供体实机保存覆盖未知，不能称确定应换3/0'},
 {'item':'其它动作中的起身请求','donor':'LB recovery can take over ground-pick/seat/named skills when fallen','ours':'queue_skill accepts only walking or seated->stand; App actionBusy disables getup in other skill','verdict':'确定不一致，可能导致点击未启动，而不是actor撑不起'},
 {'item':'卸力来源的起身结束','donor':'home_recovery end disables policy and returns HOME via bringup','ours':'after6 effective seconds always transitions to xgoduck walking; fall guard can then unload','verdict':'确定不一致，适用从卸力启动的路径；未捕获新起身不能认定本次发生'},
 {'item':'零位/方向和IMU','donor':'per-board saved calibration; yaw/neck/headpitch/headroll directions include+1','ours':'board directions empty => all-1; zeros local; MS901M calibrated mounting','verdict':'数值不同是事实，装配与轴向实物对应尚未核实；不能直接导入另一机zero/direction'},
]
paths=[root/'backend-rust/src'/n for n in ['policy.rs','control.rs','telemetry.rs','ducklink_motion.rs']]
paths += [root.parent/'mobile/android/app/src/main/java/io/microduck/control/MainActivity.java']
donor=folder.parent/'getup-compare'
paths += [donor/n for n in ['donor-robotd.rs','donor-control.rs','donor-feetech.rs','donor-hd1910.rs','donor-params.rs','donor-robotd.toml']]
audit={'date':'2026-10-11','userFeedback':'mechanical alignment still cannot recover; user intends retry','hardwareMotionAdded':False,'newGetupSessionCaptured':False,'snapshotSummaries':snapshot_summaries,'traceSummary':capture['traceSummary'],'matrix':matrix,'observedRates':ctrl.get('cycleCosts'),'sourceHashes':{str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in paths}}
(folder/'runtime-differences.json').write_text(json.dumps(audit,ensure_ascii=False,indent=2),encoding='utf-8')
lines=['# 机械限位修复后的 kissqy 起身执行差异核查', '',
'2026-10-11，用户反馈仍撑不起。本次只读获取源码/归档供体配置/本机快照及三轮45秒采样；无运动、训练、参数改动或部署。', '',
'**证据边界：** 当前已mechanical-v1。最新保存会话只有行走与坐站，没有 xgoduck_getup；瞬时快照为 sitstand_stand。三轮采样为378+381+380帧，均停留跌倒后failed/卸力。不能把这次站起或行走故障说成新倒地起身模型失败。用户说明两种按钮都点过，准备再试，仍需保留真正新起身会话。', '',
'|项目|kissqy|本机|结论|','|---|---|---|---|']
for r in matrix:lines.append('|'+ '|'.join(r[k].replace('|','/') for k in ['item','donor','ours','verdict'])+'|')
lines += ['', '优先级：先确认实际请求确实启动xgoduck_getup并完整保存；然后隔离运行增益上力与50Hz同步时序差异。电压补偿先核对供体实际开关和本机电压，不把默认值当另一台机器的现场值。零位和方向必须与本机物理轴/机械零姿对照，不能直接复制供体编码零位。', '',
'源码依据：本机 policy.rs 的 arm_current调用、profiles_with_gains、InferenceWorker/10ms FrameClock、skill.complete/切回xgoduck；telemetry.rs queue_skill；mobile MainActivity actionBusy。供体 donor-hd1910.rs arm/diagnostics、donor-robotd.rs LB recovery/scale_mult/home_recovery、donor-params.rs defaults/control.hz、donor-feetech.rs set_gain。完整源hash及本机原始快照保存。', '']
(folder/'执行差异对照.md').write_text('\n'.join(lines),encoding='utf-8')
note='''> **2026-10-11 机械修复后起身继续失败/外围差异核查（用户反馈+只读，最新覆盖）：** 用户说仍撑不起，两按钮均点过准备再试。当前mechanical-v1限位生效；最新保存会话为walk+坐站，无xgoduck_getup，瞬时坐站stand；两轮45秒378+381快照全failed卸力，不能冒称已捕获新起身或将行走跌倒故障套用起身。已确认剩余执行差异：供体50Hz同步read/model/write vs本机100Hz目标发令读取+异步50Hz推理（混合会话实际推理45.24/读取59.43，不是新起身频率）；起身上力供体先model P/D vs本机先P32D40保持再P5D20；供体默认电压倍率7.4/clamp(filtered V,6,9.5) vs本机固定1（供体现机开关未知）；供体保留41 vs本机置0；供体倒地LB能接管其它技能，本机queue/App限制；卸力来源供体结束回HOME，本机6有效秒切walk。模型/基础HOME映射/机械限位已核验，不将默认管理P3D0/torque681当供体实机值，本机model5/20/I0/1000已读。两机zero/direction/IMU数据不同，实物轴对应未核实，不能直接复制。详见附件/平台/2026-10-11/getup-after-mechanical/执行差异对照.md及runtime-differences.json/快照；等用户新起身记录再隔离因果，未训练或修改部署/参数，停止训练仍有效。'''
for path in [root/'AGENTS.md',root/'docs/实测记录/03-舵机实测.md',root/'docs/实测记录/04-网络与观测平台.md',root/'docs/实测记录/README.md']:
    text=path.read_text(encoding='utf-8-sig')
    if '2026-10-11 机械修复后起身继续失败/外围差异核查' not in text:path.write_text(note+'\n\n'+text,encoding='utf-8')
print('Saved comparison with explicit evidence boundaries; no hardware changes.')
