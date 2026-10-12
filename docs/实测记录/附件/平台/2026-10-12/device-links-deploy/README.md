# 2026-10-12 网络只读服务部署

用户要求“更新服务吧”，通过5722映射安装runtime-20261012-084506，旧runtime-20261011-213014可回滚。二进制SHA256：`22b7aac263fa9d619f9d39ecd3ce3045583a3c94b1c9a3e77cf96e1e518f3c5a`，对应已验证的64e55ec代码（132 Rust测试/1外部fixture忽略、ARM构建）。

Wi-Fi status不再停止模型，认证BLE提供wifiStatusReadOnly；其它Wi-Fi操作行为保持。只替换后端，模型、网页、标定、增益和BLE身份保持，tofd/rsyslog仍masked。安装前15卸力；候选998/999完整反馈，一帧缺23/31，通过99%门；20生产快照卸力。详见旁目录deployment.json。

后续只读核对service active、新release和15舵机无fault；用户已操作“嘎一下”，状态holding，仅嘴#34使能，不能把安装时卸力结论套到后续。没有代发运动或停止。Unix socket仅查询status，当前Wi-Fi IP10.10.161.13，Web地址http://10.10.161.13:8877/。App0.9.38重连可使用新能力，无需新OTA。本次代码和只读部署验证不等于活动模型下手机验收，也未确认旧站起/起身故障解决。

before.json/after.json为原始快照，network-status.json为权限辅助只读查询，verification.json为验证摘要。早前device-links/prepared.json“未部署”是准备时历史状态，保持原记录。
