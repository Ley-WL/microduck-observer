# kissqy 起身切换审计（2026-10-11）

用户要求不一致直接修改。比较固定供体94328f5的robotd.rs LB入口与HOME退出、control.rs start_skill/step/reset_for_home、params.rs 60°/200ms默认。comparison.json保存源hash与分支对照。

普通停机起身→HOME、行走中起身→行走及6秒窗口原本一致，不能把会话入口固定return_home描述为已证明的错误去向。修复直立策略起身保留history/坐姿、倒地才允许抢占pick/roll、重复LB不重启窗口、卸力入口确认倒地。保留本机传感器一次采样时序，复制gyro滤波状态避免使用bank内旧状态；其它模型、增益、HOME、标定和EEPROM不改。

倒地确认只用于起身请求仲裁，不更改原行走跌倒停止保护。平台新增native_homed：仅完整native HOME到位后置true，卸力清false，重启默认false；直立LB要求此状态与实时torque-on同时成立。中断起身后用户要求的停止保持不等同重新完成HOME；停止保持/急停行为继续按用户原要求。不得称全部服务状态机已与供体逐项完全相同。

用户上一条300帧起身没有中途切换，不能把本次切换修复当作低头根因已经证实。软件检查、ARM构建和只读部署不是起身实机验收。

## 验证与部署

127检查通过，1个外部推理fixture忽略；ARM构建成功。runtime-20261011-201917只更新二进制，binaryOnly包无frontend；远端旧/新frontend与models逐文件diff一致，模型标定增益BLE身份保留。候选1000只读帧999完整（其中一帧缺31/32），20生产快照仍卸力；服务active，未发动作。verification.json保留二进制和最终源码SHA。此前runtime183231可回滚。

新增共享native_homed记录native HOME完成；显式disabled清除，重启默认未homed，直立LB不能仅凭已上力就绕过HOME入口。独立行走跌倒停止与用户停止保持属于平台既有要求，保留，不冒称全服务所有状态切换与供体完全相同。低头实物是否改善未验收。
