from pathlib import Path
import hashlib
import json
import shutil

folder=Path(__file__).resolve().parent
root=folder.parents[5]
manifest=json.loads((folder/'deployment.json').read_text(encoding='utf-8'))
assert manifest['result']=='installed'
assert manifest['modelsBefore']==manifest['modelsAfter']
assert manifest['frontendBefore']==manifest['frontendAfter']
assert manifest['servoRegistersBefore']==manifest['servoRegistersAfter']
assert manifest['gainsUnchanged']
assert manifest['policyParity']['policy-saved-getup-targets.json']['savedTargetCasesVerified']==152
for name in ['install.py','policy-saved-getup-targets.json']:
    shutil.copy2(root/'deploy/staging/kissqy-mechanical-20261011'/name,folder/name)
shutil.copy2(root/'backend-rust/tests/fixtures/kissqy-mechanical-limits.json',folder/'kissqy-mechanical-limits.json')
note='''> **2026-10-11 kissqy机械限位与执行已部署（用户明确授权，最新覆盖）：** release20261011-kissqy-mechanical-v1已主板5722安装，仅更新Rust二进制。15关节声明/手动控制/模型目标统一供体保存机械范围：右yaw−30～25、左yaw−25～30、双roll±22、双髋pitch/膝±90、双踝±70、neck−90～70、headpitch±90、headyaw±170、headroll±20、mouth−5～30。模型先裁机械角度，再用本机reference/direction及4096编码换算，round与供体一致；不跨圈、不用编码饱和改目标，编码越0～4095或有效EEPROM边界时拒绝整批发令。手动超范围请求仍拒绝；嘴模型接口接受−5～30，App预设开/闭仍30/0。raw prev_action/反馈/模型归一化、各技能执行时长与独立增益保持；旧训练合同/3D模型XML不冒称机械范围、不重写权重。103Rust检查通过/1忽略，ARM构建及主板五技能纯推理fixture通过；另外保存实机起身152帧×14目标按供体机械裁剪公式独立生成expected编码，ARM逐值核对全部通过，无UART访问。模拟候选15项HTTP限位核验后，空闲全卸力状态切换，标定物理值、15舵机寄存器、BLE身份、所有模型/网页与持久增益保持，20只读快照零缺，服务active；tofd/rsyslog仍masked。旧kissqy-pick-getup-v1完整保留可回退。没有训练或自动运动；这是发令一致性和只读部署验收，不代表起身已实物成功。证据附件/平台/2026-10-11/kissqy-mechanical/{deployment.json,source-hashes.json,kissqy-mechanical-tests.log,kissqy-mechanical-arm.log,policy-saved-getup-targets.json}；上一角度核查的未执行限位结论属修复前。'''
for path in [root/'AGENTS.md',root/'docs/实测记录/03-舵机实测.md',root/'docs/实测记录/04-网络与观测平台.md',root/'docs/实测记录/README.md']:
    contents=path.read_text(encoding='utf-8-sig')
    if '2026-10-11 kissqy机械限位与执行已部署' not in contents:
        path.write_text(note+'\n\n'+contents,encoding='utf-8')
short='''> **2026-10-11 最新执行覆盖：** 已部署kissqy-mechanical-v1，全部15关节机械范围采用kissqy保存标定。模型先裁机械角度、再按本机零位/方向转4096编码，无跨圈/编码饱和；单圈编码或EEPROM有效边界不满足则拒绝。模型权重、raw历史、增益及其它设置保持。旧训练XML/v3限位合同属于训练历史，和当前机械发令 guard 分开核验；旧段落的“仅v3裁角度/原供体编码饱和”已被本条覆盖。103检查、ARM五技能及152帧目标公式核对通过，只读部署非起身实机成功，详见实测03/04及2026-10-11/kissqy-mechanical证据。
'''
for path in [root/'backend-rust/XGODUCK-SKILLS.md',root/'deploy/RUST.md']:
    contents=path.read_text(encoding='utf-8')
    if '2026-10-11 最新执行覆盖' not in contents:
        first,rest=contents.split('\n',1)
        path.write_text(first+'\n\n'+short+'\n'+rest.lstrip('\n'),encoding='utf-8')
(folder/'evidence-sha256.json').write_text(json.dumps({p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in folder.iterdir() if p.is_file() and p.name!='evidence-sha256.json'},indent=2),encoding='utf-8')
print('Deployment validated; six records updated. No motion performed.')
