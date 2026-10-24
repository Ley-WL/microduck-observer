# 1.6秒翻滚与站稳提前切行走

用户要求：翻滚窗口2→1.6秒，翻滚后倒地起身采用坐站相同的交接条件。

- 1.6秒后进入主动收腿，原IMU躺倒交接门保持。
- 与坐站共用 `SkillProfile::standing_ready`：倾角<10°、角速度模<0.5rad/s、10个腿关节各自HOME偏差<0.30rad、速度<1rad/s，持续0.5秒即交回行走。
- 移除这条翻滚恢复链原有的至少3秒等待；独立起身入口保持原行为。失效反馈清零稳定计时，急停及超时保护保持。
- 权重、增益、限位未改。

## 名义仿真配对比较

同seed221227/221231，各32环境，共64例；两版均64/64完成，末1秒低运动通过。新版本回行走3.04～5.10秒，中位数3.44秒；旧版5.24～6.46秒，中位数5.36秒。起身阶段耗时中位数3.00→1.59秒。模型和HD1910/Observer环境同前次；本轮继续使用相同种子，属于配对回归，不是新的独立成功率估计。实机效果待测试。

[结果](https://github.com/Ley-WL/microduck-rl/blob/main/docs/evidence/roll16-stable-getup-20261022/results.json) · [脚本](https://github.com/Ley-WL/microduck-rl/blob/main/scripts/server/evaluate_roll_getup.py)

285 Rust测试通过（1忽略）、ARM编译通过，UTF-8无BOM/LF检查通过。已部署233：runtime-20261022-144926，回退144051。20秒只读301/301关节及IMU帧完整，20生产快照卸力；模型、增益和标定保持。未发动作，实测待用户。二进制SHA256：58b2441c3bc509df0b49d6b3a16586abc57f905fc6e4b3db7ebd871754ff4b5f。

[部署证据](实测记录/附件/平台/2026-10-22/roll16-stable-getup/deployment.json)
