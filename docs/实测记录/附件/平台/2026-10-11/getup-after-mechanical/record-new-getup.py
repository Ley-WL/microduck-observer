import json,hashlib
from pathlib import Path
import numpy as np

folder=Path(__file__).resolve().parent;root=folder.parents[5]
a=json.loads((folder/'new-getup-analysis.json').read_text(encoding='utf-8'))
assert a['frames']==378 and a['onnxReplayMaxErrorRadians']<1e-6
assert not any(r['targetOutsideMechanicalFrames'] for r in a['perJoint'])
data=np.load(folder/'new-getup-aligned.npz');time=data['time']
segments={}
for name,mask in [('first2Seconds',time<2),('last2Seconds',time>time[-1]-2)]:
    segments[name]={'trackingP95Degrees':np.percentile(np.abs(data['targets'][mask]-data['actual'][mask]),95,axis=0).tolist(),
                    'targetFrameChangeP95Degrees':np.percentile(np.abs(np.diff(data['targets'][mask],axis=0)),95,axis=0).tolist(),
                    'maxAbsLoadRaw':np.abs(data['loads'][mask]).max(axis=0).tolist(),
                    'tiltRangeDegrees':[float(data['tilt'][mask].min()),float(data['tilt'][mask].max())]}
(folder/'new-getup-segments.json').write_text(json.dumps(segments,indent=2),encoding='utf-8')
lines=['# 最新真实倒地起身：机械限位修复后（2026-10-11）', '',
'用户确认App显示起身执行中，并完成一次倒地起身→停止。只读watch已保存独立getup-1791685669674.json，之后其它动作不会覆盖该附件；本次不发实机运动。', '',
'- 正确模型a481d9…，378推理帧，7.760秒墙钟/7.560有效秒，全程xgoduck_getup，结束disabled/卸力；没有其它模型切入。',
'- 投影重力倾角87.08°→最好80.87°（0.30秒）→末83.27°；6秒时83.23°。全程没有直立。',
'- 机械限位全部生效，14关节零目标越界；ONNX相同观测重放误差2.38e−7rad，模型权重或输出转换错误没有新证据。',
'- 本次实际推理48.46Hz、反馈57.18Hz；推理间隔median20.00ms/p95 29.92ms/max31.47ms，异步结果age median10.39ms/p95 11.04ms。前轮45.24/59.43是混合会话，不能冒称本次频率或主要失败原因。',
'- 电压6.8～8.0V，零missing/校验错误/舵机fault；仅少量瞬态load到1000，不支持全程堵转/欠压停机。负载raw不是物理力矩。',
'- 首2秒目标帧跳大，头yaw/pitch/neck p95约27.53/24.41/21.31°，右膝17.29°；头yaw跟随p95 50.98°、头pitch36.83°、右膝31.31°。',
'- 最后2秒倾角稳定83.22～83.28°，目标帧跳p95各关节≤0.26°；12/14关节跟随p95≤1.5°，neck6.94°、headroll2.20°；load abs最大neck109、其余≤43。模型已经收敛到倒地稳定姿态，多数关节能跟上末段目标，不能把“撑不起”全归因P太小/舵机力量不够。', '',
'## 新确认的结束流程差异与纠错', '',
'policy.rs 的 next 在 complete之后仍检查倾角：若不是sitstand_stand且tilt>55°，选择xgoduck_getup；随后过滤next==model.kind，使当前起身继续运行而不重置，也不结束。本次6有效秒后继续78帧，直到用户停止。因此前轮“6秒后总切回walk”描述不准确，本条按代码和新实测纠正。kissqy的卸力来源home_recovery在技能窗口结束后关闭policy并进入HOME流程；不是这条持续推理路径。', '',
'剩余上力增益32/40→5/20、异步时序、默认电压補偿和动作接管差异仍存在；此次不根据默认供体参数盲改P/扭矩。末段稳定倒地优先核对模型坐标系与本机IMU轴向、机械零位及接触/装配对应，再做有依据的执行对齐。不同板的zero/direction或安装四元数不能直接复制，当前尚无独立物理轴验证。', '',
'本次只读采集/离线分析，无训练、部署、参数修改或自动运动；仿真/同输入重放不是实际起身通过。', '']
(folder/'最新起身分析.md').write_text('\n'.join(lines),encoding='utf-8')
note='''> **2026-10-11 机械修复后真实倒地起身已捕获（最新覆盖/纠错）：** 用户完成倒地起身→停止，独立trace getup-1791685669674.json保存；378帧，a481d9模型，7.760墙钟/7.560有效秒，全程getup，结束卸力，无模型打架。倾角87.08→最好80.87（0.3秒）→83.27，6秒83.23未直立；全部机械目标无越界/同观测ORT误差2.38e-7rad，6.8～8V、零missing/checksum/servofault。本次推理48.46/反馈57.18Hz、间隔median20/p9529.92ms、异步结果age median10.39ms；前轮45.24/59.43属于混合会话非本次。首2秒快速目标跳变/跟随偏差，最后2秒target帧跳p95≤.26°，12/14跟随p95≤1.5°（neck6.94/headroll2.20），负载raw仅neck最高109其余≤43，却仍83°倒地，不能仅归因力量/P不足或持续堵转。新确认代码complete后倾角>55仍选择当前getup，next==kind过滤导致持续推理，本次超6有效秒继续78帧直至用户停止；前轮“6秒总切walk”不准确，本条纠正。供体卸力来源home_recovery窗口结束返回HOME，执行仍不同。优先核对IMU轴/机械零姿/接触装配对应并对齐运行增益上力、时序和结束流程；不直接复制供体zero/direction或默认P3D0/681。证据附件/平台/2026-10-11/getup-after-mechanical/最新起身分析.md、new-getup-analysis/segments/aligned及原trace。仅只读采集和离线分析，无训练/部署/自动运动或参数修改；停止训练仍有效。'''
for path in [root/'AGENTS.md',root/'docs/实测记录/03-舵机实测.md',root/'docs/实测记录/04-网络与观测平台.md',root/'docs/实测记录/README.md']:
    text=path.read_text(encoding='utf-8-sig')
    if '2026-10-11 机械修复后真实倒地起身已捕获' not in text:path.write_text(note+'\n\n'+text,encoding='utf-8')
(folder/'new-getup-evidence-sha256.json').write_text(json.dumps({p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in folder.iterdir() if p.is_file() and p.name!='new-getup-evidence-sha256.json'},indent=2),encoding='utf-8')
print('Saved real getup evidence and corrected end-flow claim; no hardware changes.')
