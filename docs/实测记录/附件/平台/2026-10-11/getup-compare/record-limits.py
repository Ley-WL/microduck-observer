from pathlib import Path
import json

folder = Path(__file__).resolve().parent
root = folder.parents[5]
audit = json.loads((folder/'angle-limits-audit.json').read_text(encoding='utf-8'))
assert audit['frames'] == 152 and not audit['modelAngleClippingEnabled']
assert sum(not r['declaredEqualsDonorMechanical'] for r in audit['rows']) == 8
note = '''> **2026-10-11 kissqy起身角度限位核查（代码/归档标定/既有实机trace，最新覆盖）：** 角度限位没有完全一致，不能将相同模型权重解释为相同执行。平台声明表与供体保存机械限位15关节中8项不同：右yaw−25.78～12.61 vs−30～25、左yaw−12.61～25.78 vs−25～30；双踝±90 vs±70；neck−135～60 vs−90～70；headpitch±105 vs±90、headroll±25 vs±20；mouth0～30 vs−5～30（嘴非14维起身输出）。供体训练XML亦不是机械表：双踝±90、neck−90～60、headroll±25。更关键：当前kissqy_stand未启用bounded_targets，policy.rs只裁编码0～4095与有效EEPROM边界；供体feetech target_raw先裁保存min_rad/max_rad，再转编码并拒绝编码越界。既有152帧已发目标左hiproll−82.62～−26.72，全部越±22；右yaw最大46.32、左hippitch最小−118.04、右膝最大93.43、neck最大115.40、headyaw最小−179.47、headroll最大28.74，明确实际发令差异。供体标定是stand-contract中归档源b56de589…，不是当前供体实机回读；本证据未包含本板全部EEPROM边界，不推断统一0～4095。统计含0.05°编码舍入容差；不证明这是撑不起的唯一原因或套限位保证成功。详见附件/平台/2026-10-11/getup-compare/角度限位对照.md与angle-limits-audit.json/审计脚本/源hash。仅离线只读核查和记录维护，未发运动、训练、改代码执行/部署/标定EEPROM增益；停止训练要求仍有效。'''
for path in [root/'AGENTS.md', root/'docs/实测记录/03-舵机实测.md',
             root/'docs/实测记录/04-网络与观测平台.md', root/'docs/实测记录/README.md']:
    contents = path.read_text(encoding='utf-8-sig')
    if '2026-10-11 kissqy起身角度限位核查' not in contents:
        path.write_text(note+'\n\n'+contents,encoding='utf-8')
print('Updated four observer records; no execution changes.')
