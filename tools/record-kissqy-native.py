"""Record verified installation and its boundaries without rewriting history."""
from pathlib import Path
import hashlib,json

root=Path(__file__).resolve().parents[1]
e=root/'docs/实测记录/附件/平台/2026-10-11/kissqy-native'
d=json.loads((e/'deployment.json').read_text(encoding='utf-8'))
assert d['result']=='installed' and d['release'].endswith('20261011-kissqy-native-v2')
assert d['gainsUnchanged'] and all(r['torque']==0 for r in d['servoRegistersAfter'].values())
assert all(v['ok'] and not v['hardwareAccess'] for v in d['policyParity'].values())
summary='''**2026-10-11 kissqy核心策略全面核查与同步运行部署（最新覆盖）：** 用户要求模型及处理参照kissqy，并明确IMU轴向/机械零位已校准、3D与实物运动一致；采纳该反馈，不再把未校准当主要原因，不修改标定。固定上游94328f5b43c8ac825c90f0e564e7d1b926cd770d五模型逐字节哈希核验；发现翻滚仍为旧XgoDuck 8adb，已替换供体roulade 8fdaf12/1秒/无额外EMA，getup a481、walk1cbf、sitstand1c949、pick2b69权重保持。新kissqy运行器同步50Hz读→组观测→推理→写，一帧一次推理/发令，61维顺序/归一化图/raw prev_action/原生速度一采样延迟保持，切换不重复推进速度历史；起身直接model P5D20上力、嘴P10D20，保留41，不再先32/40；HOME两秒直线；电压按源码默认true/7.4V、15舵机均值每秒EMA新0.1，闲置/动作跨会话连续，再7.4/clamp(V,6,9.5)只作用动作；去掉后退/yaw隐藏2倍倍率；嘴闭−5/开30，不随pick自动开合。起身可接管坐/站/pick/roulade；直接启动的起身300个20ms有效tick结束回HOME保持，不再倾倒时无限自选getup；策略内起身结束交回walk，仍倒地由本机已有跌倒规则卸力，不伪称成功。release20261011-kissqy-native-v2已5722安装，网页更新；107Rust检查/1忽略、62网页检查/构建、7份ARM纯推理fixture（obs误差0/output≤1.2e-7、152×14机械公式目标）、378旧实机同输入重放raw误差2.38e-7通过。20只读快照零缺，15卸力；标定物理值/15寄存器/持久增益/BLE身份及其它模型保持，tofd/rsyslog仍masked，机械-v1完整可回退。App0.9.23/code42已OTA验签/长度/哈希/原APK签名与模拟器14坐站起身门控检查通过。保留本机外置IMU传输/本机reference-direction、EEPROM边界、真实fault/4～8.4V/反馈与IMU暂停和跌倒卸力处理、用户持久增益；供体现机保存的servo-test/电压开关/torque cap不可从源码推断，未声称其现机每个配置一致，也非整套robotd所有外设功能复制。尚未新实机运动验收，不能承诺能起身；没有训练或自动运动。首次v1因Windows哈希路径分隔符在服务切换前拒绝，修正为POSIX后v2安装；期间两次上传SSH超时未重复动作。证据附件/平台/2026-10-11/kissqy-native/{audit,deployment,source-hashes}.json及上游五组模型合同、构建日志。'''
for relative in ['AGENTS.md','docs/实测记录/03-舵机实测.md','docs/实测记录/04-网络与观测平台.md','docs/实测记录/README.md']:
    p=root/relative;s=p.read_text(encoding='utf-8')
    if '**2026-10-11 kissqy核心策略全面核查与同步运行部署' not in s:p.write_text('> '+summary+'\n\n'+s,encoding='utf-8')
detail='''
## 2026-10-11 kissqy同步控制核查

核对基准固定为kissqy提交94328f5b43c8ac825c90f0e564e7d1b926cd770d及各模型deployment-contract。行走/起身/坐站/拾取/翻滚均同上游权重；兼容文件名xgoduck不代表仍用旧XgoDuck模型。所有kissqy入口现在同步50Hz read→observe→infer→write；旧本机训练模型入口仍独立保留，不把它们称作kissqy模型。

起身/坐站从当前位置使用模型增益上力；HOME用模型增益和两秒线性过渡。起身直接启动的六秒窗口结束回HOME保持；运行中接管起身则到期交回行走。没有达到直立不会无限重复同一模型，也不会将窗口结束当成成功。原始动作历史独立于电压倍率，native速度延迟随采样推进一次。电压补偿采用固定提交源码默认true，而非已读到供体现机开关；此选项及用户持久增益是运行配置，不能靠模型哈希宣称两台实机设置一样。

本机设备适配仍保留已校准外置IMU、编码零点/方向、EEPROM、工作电压/故障/数据年龄和用户要求的行走跌倒卸力；供体的全套robotd外设、音频/游戏手柄/低电量关机等不属于本次策略移植。新发布只读验收，不是实机起身成功验收。完整证据见docs/实测记录/附件/平台/2026-10-11/kissqy-native。
'''
for relative in ['backend-rust/XGODUCK-SKILLS.md','backend-rust/DUCKLINK.md']:
    p=root/relative;s=p.read_text(encoding='utf-8')
    if '## 2026-10-11 kissqy同步控制核查' not in s:p.write_text(s+detail,encoding='utf-8')
mobile=root.parent/'mobile'
ota=json.loads((mobile/'releases/kissqy-native-0.9.23/verification.json').read_text(encoding='utf-8'))
assert ota['publicApkHashAndLengthVerified'] and ota['manifestSignatureVerified']
note='''
## 2026-10-11 kissqy起身接管与OTA 0.9.23

倒地起身按钮在坐下、站起、拾取和翻滚执行时可用，保留连接/维护/待发送/反馈保持/重复起身门控；后端匹配会话及递增sequence接受接管。配套主板kissqy-native-v2同步50Hz控制，去掉隐藏后退和转向倍率，嘴闭合−5°。保持原应用ID和签名，0.9.23/code42构建/lint通过；模拟器实际自定义Instrumentation回报14坐站起身、34设备布局、5停止、9主板信息/电源以及OTA/BLE/摇杆/标定等检查通过（Gradle显示0个JUnit测试不能当这些检查的计数，另直接运行runner验证）。公网清单签名/本地一致性/APK长度6788317/SHA256 646e47a2b6bc9396841130e418023f05b177011b92581e9f8ad55e7bbe46047e/下载页/原APK签名核对通过。真实手机和新实机动作未验证；没有自动运动/训练。证据releases/kissqy-native-0.9.23/verification.json。
'''
for relative in ['OTA-DEPLOYMENT.md','TEST-RESULTS.md']:
    p=mobile/relative;s=p.read_text(encoding='utf-8')
    if '## 2026-10-11 kissqy起身接管与OTA 0.9.23' not in s:p.write_text(s+note,encoding='utf-8')
paths=list(e.glob('*'))+[root/'tools/record-kissqy-native.py']
(e/'final-evidence-hashes.json').write_text(json.dumps({p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in paths if p.is_file() and p.name!='final-evidence-hashes.json'},indent=2),encoding='utf-8')
print(json.dumps({'recordsUpdated':8,'release':d['release'],'physicalMotion':False,'training':False}))
