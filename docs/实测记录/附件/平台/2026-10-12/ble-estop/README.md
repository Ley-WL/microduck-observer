# 急停后断连与残留连接（2026-10-12）

> **2026-10-12 急停误断连修复（App0.9.42/code61已OTA）：** 主板estop seq208收到后1473/1482ms才两次卸力确认，随后1516ms收到release；App旧整个exchange期限1500ms，卸力结束后剩余不足完整回复，存在明确误超时路径。服务PID1996/NRestarts0，仅BlueZ订阅结束日志，无协议拒绝，不能宣称完整空中断连根因。手机失败期间BlueZ仍Connected=yes；确认主板空闲15卸力后只断开残留一加LE连接，用户确认能重连，绑定身份保持。App急停仍立即发令，回复预算4500ms；普通drive1500ms不变，exchange失败同步关闭GATT阻止清理阶段在失效会话发release，连接失败保留GATT原因。构建/lint0错误27警告、API30全回归含66BLE/6急停检查（1.8秒延迟确认/失败会话立即关闭）通过，原APK签名、公网清单验签/APK逐字节SHA长度/下载页通过。无主板部署/模型变更/训练/代发实机动作；新App急停真实手机闭环仍待验收。证据mobile/releases/ble-estop-0.9.42及observer附件/平台/2026-10-12/ble-estop。


logs-before.json/timing-analysis.json保存时间轴；before-recovery.json确认空闲15卸力；disconnect-stale-phone.log保存一加Connected yes→no。用户确认能重新连接。现场手机错误原文泛化、未获取Android GATT status，不能宣称排除所有断连原因。
