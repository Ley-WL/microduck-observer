from pathlib import Path
import json
folder=Path(__file__).resolve().parent;root=folder.parents[5]
analysis=json.loads((folder/'analysis.json').read_text(encoding='utf-8'))
assert analysis['frames']==152 and analysis['onnxRawReplayMaxErrorRad']<1e-6
note='''> **2026-10-11 kissqy倒地起身失败对比（用户反馈+只读实机证据，最新覆盖）：** 用户明确kissqy/Microduck-kissqy能动但撑不起来。板release仍kissqy-pick-getup-v1；GitHub提交94328的stand.onnx、本地与主板xgoduck_getup.onnx完全同SHAa481d9f211f3…，HOME+raw倍率1/无EMA、raw history和速度反馈规则保持，本次P5/D20及torqueLimit1000。供体native默认训练P/D20与保存servo-test覆盖/R17默认3/0/681必须区分，不冒称已读供体实机设置。最新起身152帧3.04有效秒，倾角86.88°→最好77.21°→111.21°，终态disabled/卸力，无自动切换/模型报错/缺帧/舵机fault，电压6.2～8.0V；同观测ONNX重放误差1.19e-7rad。头yaw/左髋roll/头pitch等目标位置偏差p95约74.85/47.00/43.49°，多关节负载1000，#32七帧裁剪且末目标−179.47°，训练头yaw±170°；这直接显示实机跟随偏离，不是唯一机械卡死/PD根因证明。另存policy-fall是更早的行走xgoduck倾角84°，不能套到本次起身；起身跌倒检测关闭、没有到6秒窗口，不能算完整6秒失败或保证延长即成功。原ec665仿真结论属历史不适用a481。详见附件/平台/2026-10-11/getup-compare/对比结论.md及原trace/fault/analysis/固定供体源码与模型。未训练/部署/发运动/改标定EEPROM/增益/保护或重启；用户停止训练要求继续有效，自动优化检查此前已删除。后续须区分接触/负载/传感器及训练物理，不能直接加P或让顶住时硬撑。'''
for file in [root/'AGENTS.md',root/'docs/实测记录/03-舵机实测.md',root/'docs/实测记录/04-网络与观测平台.md',root/'docs/实测记录/README.md']:
    contents=file.read_text(encoding='utf-8-sig')
    if '2026-10-11 kissqy倒地起身失败对比' not in contents:file.write_text(note+'\n\n'+contents,encoding='utf-8')
print('Updated observer instructions, servo/platform topics and evidence index; no hardware changes.')
