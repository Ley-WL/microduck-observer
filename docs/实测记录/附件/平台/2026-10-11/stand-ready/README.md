# 坐下后站起等待纠错（2026-10-11）

用户实际已落座但App站起仍锁定。此前将本机元数据sitSettleSeconds=4误称供体普通站起等待，且在0.9.35将旧App2秒延长到4秒，是错误归因/业务门控。

固定kissqy94328f5 donor-control.rs::sit_toggle：Sit::Sitting直接设Rising，拒绝仅Rising途中重复；busy明确Sitting不是busy。SkillTuning::sitstand_ramp_s默认2秒，shutdown_sit_secs=2×ramp=4秒用于关机前坐下，不是普通站起最低等待。已有供体源码文件getup-compare/donor-control.rs及donor-params.rs保持，comparison.json列依据与纠错。

Rust donor坐姿第一次有效policy帧即standAllowed=true，普通站起取消固定计时；队列仍验证policy/会话/序号/反馈非暂停/无pending动作，重训模型和其它旧profile原等待不变。坐姿展示settle估计恢复2秒，业务允许切站起与坐姿已稳定不是同一概念，不称通过姿态识别。保留原模型artifact元数据4这个遗留字段做兼容校验，不再作为donor站起门；没有改ONNX/训练/标定/增益/EEPROM/IMU或BLE身份。

BLE顶层传standAllowed；网页和App按后端布尔判断，旧后端未提供字段时App保留旧时间回退。尚未启动/反馈暂停/待切换/重复站起仍拒绝。站起1有效秒后回行走、之前拾取嘴组合保持。

132Rust/1外部忽略、网页59/构建、Android构建lint/API30整套回归（含0.02秒donor允许、后端拒绝、旧服务4秒回退）通过。用户回复好了后仅只读部署runtime210935，旧210030可回滚，999/999完整候选帧/20生产快照全卸力；模型/标定增益/BLE身份保持，网页资产校验、tofd/rsyslog masked。App0.9.36/code55已OTA，原签名、公网App公钥验签/APK逐字节长度SHA/下载页通过；见平级mobile/releases/stand-ready-0.9.36/verification.json。未代发动作，按钮和切换实机效果仍需用户验收。
