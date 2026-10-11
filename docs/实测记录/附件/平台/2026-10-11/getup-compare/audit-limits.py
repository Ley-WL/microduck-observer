"""Read-only angle-limit comparison using saved contract, code and real trace."""
import hashlib
import json
import math
from pathlib import Path

folder = Path(__file__).resolve().parent
root = folder.parents[5]
contract_path = root / 'deploy/staging/kissqy-pick-getup-20261010/stand-contract.json'
contract = json.loads(contract_path.read_text(encoding='utf-8'))
trace = json.loads((folder / 'policy-session-latest.json').read_text(encoding='utf-8'))
meta = json.loads((root / 'models/xgoduck_getup.metadata.json').read_text(encoding='utf-8'))
assert meta['runtimeKind'] == 'kissqy_stand'
assert meta.get('skillPhysicsRevision') != 'sit-self-contact-bounded-target-v3'
assert contract['hardware_calibration']['data'] == contract['training_hardware_calibration']['data']
static = {10: (-math.degrees(.45), math.degrees(.22)), 20: (-math.degrees(.22), math.degrees(.45)),
          11: (-22, 22), 21: (-22, 22), 30: (-135, 60), 31: (-105, 105),
          32: (-170, 170), 33: (-25, 25), 34: (0, 30)}
static.update({i: (-90, 90) for i in (12, 13, 14, 22, 23, 24)})
names = {10:'右髋转向',11:'右髋侧摆',12:'右髋俯仰',13:'右膝',14:'右踝',
         20:'左髋转向',21:'左髋侧摆',22:'左髋俯仰',23:'左膝',24:'左踝',
         30:'颈俯仰',31:'头俯仰',32:'头旋转',33:'头侧摆',34:'嘴'}
xml = {j['name']: [math.degrees(v) for v in j['range']] for j in contract['model']['joints']}
frames = [f for f in trace['frames'] if f['type'] == 'inference']
assert len(frames) == 152
rows = []
for j in sorted(contract['hardware_calibration']['data']['joints'], key=lambda j:j['id']):
    id = j['id']
    mechanical = [math.degrees(j[k]) for k in ('min_rad', 'max_rad')]
    values = [f['sentTargetDegrees'][trace['jointIds'].index(id)] for f in frames] if id in trace['jointIds'] else []
    def outside(bounds):
        # 0.05 degree tolerance covers nearest encoder-step rounding (half step=0.04395 deg).
        return sum(v < bounds[0]-.05 or v > bounds[1]+.05 for v in values)
    rows.append({'id':id, 'joint':j['name'], 'name':names[id], 'platformDeclaredDegrees':static[id],
                 'donorTrainingXmlDegrees':xml.get(j['name']), 'donorSavedMechanicalDegrees':mechanical,
                 'declaredEqualsDonorMechanical': all(abs(a-b)<1e-6 for a,b in zip(static[id], mechanical)),
                 'sentTargetRangeDegrees':[min(values),max(values)] if values else None,
                 'framesOutsidePlatformDeclared':outside(static[id]) if values else None,
                 'framesOutsideDonorSavedMechanical':outside(mechanical) if values else None})
sources = [contract_path, root/'backend-rust/src/policy.rs', root/'backend-rust/src/telemetry.rs',
           root/'models/xgoduck_getup.metadata.json',folder/'donor-feetech.rs', folder/'policy-session-latest.json', Path(__file__)]
result = {'date':'2026-10-11','scope':'kissqy fallen recovery; archived donor calibration, current local code and saved real-board session',
          'runtimeKind':meta['runtimeKind'], 'modelAngleClippingEnabled':False,
          'runtimeClipping':'encoder 0..4095 intersect valid EEPROM bounds; NOT declared per-joint angular bounds',
          'donorClipping':'saved mechanical min_rad/max_rad before encoder conversion; invalid single-turn target rejected',
          'boardEepromBounds':'not captured in this evidence; do not infer default bounds for all joints',
          'donorCalibrationSource':contract['hardware_calibration']['source'],
          'donorCalibrationSourceSha256':contract['hardware_calibration']['source_sha256'],
          'frames':len(frames),'outsideToleranceDegrees':.05,'hardwareMotionAdded':False,'rows':rows,
          'sourceHashes':{str(p.relative_to(root)):hashlib.sha256(p.read_bytes()).hexdigest() for p in sources}}
(folder/'angle-limits-audit.json').write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8')
def span(v):
    return '—' if v is None else f'{v[0]:.2f}～{v[1]:.2f}'
lines = ['# kissqy 倒地起身角度限位核查（2026-10-11）', '',
         '不一致：限位数值有差异，模型发令的裁剪机制也有差异。模型权重相同不等于整套实机执行一致。', '',
         'kissqy 保存的标定来自 stand-contract.json 的 hardware_calibration，和 training_hardware_calibration.data 逐值相同；是归档值，不是本次读取供体实机。训练 XML、机械限位分别列出，不能混为一张表。', '',
         '|关节/ID|平台声明角度 °|kissqy 训练 XML °|kissqy 保存机械限位 °|本次已发目标范围 °|超平台/超供体帧数|',
         '|---|---|---|---|---|---|']
for r in rows:
    counts = '—' if r['sentTargetRangeDegrees'] is None else f"{r['framesOutsidePlatformDeclared']}/{r['framesOutsideDonorSavedMechanical']}"
    lines.append(f"|{r['name']} #{r['id']}|{span(r['platformDeclaredDegrees'])}|{span(r['donorTrainingXmlDegrees'])}|{span(r['donorSavedMechanicalDegrees'])}|{span(r['sentTargetRangeDegrees'])}|{counts}|")
lines += ['', '## 实际发令机制', '',
          '- 供体 donor-feetech.rs:794 的 target_raw 先将关节目标裁到标定 min_rad/max_rad，再换算编码；编码不在 0～4095 时拒绝，不跨圈。',
          '- 平台 policy.rs:160 的 kissqy_stand 未启用 bounded_targets；policy.rs:770 的 skill_target 跳过角度裁剪，policy.rs:784 的 donor_target 只裁到单圈编码与有效 EEPROM 边界。telemetry.rs:20 的声明角度表没有在此起身路径执行。',
          '- 152 帧来自已保存的实际起身会话，不是新运动。超范围统计容差 0.05°，覆盖编码四舍五入半步误差。嘴不在 14 维起身输出中，未统计其运动目标。',
          '- 本证据未捕获主板每个舵机 EEPROM 边界，不能宣称所有 EEPROM 都是 0～4095。已发送目标越过声明角度，足以证明这条路径没有落实该角度表。',
          '- 这是一项明确执行差异，可影响起身姿态，但尚不能证明它是撑不起来的唯一原因；不能以此保证简单套限位就一定起身。',
          '- 仅核查并维护记录，未训练、改服务/模型、发运动或改标定/EEPROM/增益。', '']
(folder/'角度限位对照.md').write_text('\n'.join(lines),encoding='utf-8')
print(json.dumps(rows,ensure_ascii=False,indent=2))
