# 坐下预备与行走执行核对（2026-10-11）

用户要求暂停训练，预备改坐下、App平衡改站起，检查kissqy行走差异及后退无效。供体固定94328f5b43c8ac825c90f0e564e7d1b926cd770d。没有自动实机动作、训练或新仿真。

## 已改行为

- App/网页「坐下预备」启动现有sitstand_sit，从新鲜实读当前位置对齐使能，直接推理维持坐姿，不先走直立HOME。倒地入口检查保留，不是倒地起身替代训练。
- App「站起」发standup，按主板sitSettleSeconds（当前4有效秒）开放，旧硬编码2秒已修；站起1有效秒后切现有行走策略，同driveSession、保留raw action/传感器历史，不插入HOME或卸力使能。
- 旧stand兼容为站起，旧预备HOME oneshot删除。App显示实际commandTwist，精简BLE回复保留该字段。
- 行走指令补齐供体robotd cmd_alpha=0.2/50Hz平滑；技能姿态/相位编码不经过该平滑。超时立即归零仍为本机保护；动作切换和长反馈恢复清行走指令历史。
- 网页速度条显示0.40但wheelTwist隐藏上限0.20已修。命令范围按供体训练由vx±.4/vy±.1/yaw±.5改为±.4/±.3/±1。App满速和网页轮盘转向可到±1，没有后退放大或符号补偿。

## 与kissqy对照

| 项目 | 结果 |
|---|---|
| 权重 | 同1cbf8730…，61输入/14输出，normalizer烘焙ONNX；未改权重 |
| 观测 | 同gyro/gravity/q-HOME/速度/raw prev_action/13维command；原始校准机身gyro、寄存器速度一帧延迟 |
| 目标 | HOME+.9raw，头.5/腿.7新权重、首帧直通；与已冻结donor-execution.json一致 |
| 时序/电压 | 同native读→观测→推理→写50Hz、7.4V电压适配，已有专项回归保持 |
| 指令 | 本次补齐平滑及训练范围。App下推负forward、BLE speed×.4、Rust不取绝对值；测试负号贯穿链路。训练范围支持负vx不证明实机后退表现 |
| PD差异 | 本机持久P5D0（gains.json新鲜回读）；供体管理默认P3D0，非其真实板保存配置。本机用户配置不擅自覆盖 |
| 输出差异 | 本机EEPROM输出上限1000保持；供体管理默认681未导入，不能等同其板回读 |
| 安全/交互差异 | 本机跌倒停止卸力、超时立即清速度、停止保持、单控制会话保留；供体daemon自动limp/recovery等未全复制 |
| 其它command | App当前只twist，头/身体槽0；供体另支持独立头/身体输入 |

本次最新session只有start事件，不能对应用户后退失败（latest-session.json）。尚无有效负速度运动反馈可判模型、增益或机械跟随唯一根因；未宣称后退/转向/起身实机成功。

## 验证与部署

Rust生产bins130检查/1外部fixture忽略；网页59检查/构建；Android构建lint/API30整套回归（含摇杆下推、4秒门、BLE62/后台19）通过。原APK签名保持。

空闲15卸力下runtime-20261011-204748部署、旧203144可回滚，候选999/999完整反馈、20生产只读快照及HTTP资产核对通过。模型/标定/IMU参考/EEPROM/增益/BLE身份保持、tofd/rsyslog masked，没有启动动作。详见verification.json及相邻runtime204748/deployment.json。

App0.9.35/code54已OTA；APK6909106字节/SHA8d9fe21456112ab093b884d76161f3f64e461795908696827e6e62b6f7b80df1。公网App公钥验签/APK逐字节长度SHA/下载页与原APK签名通过，见平级mobile/releases/sit-prepare-0.9.35/verification.json。软件和只读部署不等于实机运动验收。
