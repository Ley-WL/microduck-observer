# DuckLink 统一 Rust 后端

`microduck-observer` 同一进程运行 BlueZ GATT、DuckLink/1 AES-GCM 认证、意图控制和 Web API。BLE 从内存读取遥测，动作复用进程内控制入口和唯一 UART owner，不再由 Python HTTP 网关轮询/转发。预备姿势将任务交给同一 owner，异步收结果，BLE 急停/心跳不会等待姿势完成。

兼容旧 Service/RX/TX UUID、MTU≥185、8192 字节分片、HMAC 派生、AES-GCM 方向密钥及递增序号。沿用 `/var/lib/ducklink/identity.json` 的 robotId/setupCode/owner，升级不生成替代身份。一个 BLE peer、一份控制租约；认证前不能执行命令，另一手机不能驱逐当前连接。Web/BLE 仍共享运动任务锁，BLE 不接管网页模型会话。

连接默认零指令待机，无需解锁；主动急停卸力，点击恢复仅解除状态，不自动使能/启动。250ms 指令过期归零，500ms 无有效命令停止本控制端模型并保持；断开停止本人模型且回到待机，重连不恢复旧行走。IMU/舵机反馈暂停清方向，恢复需重新按住。共享标定、舵机限位/故障与工作电压检查保持。

启用配置 `MICRODUCK_BLE_ENABLED=1`，身份路径 `MICRODUCK_BLE_IDENTITY`。`GET /api/v1/health` 的 bluetooth 字段报告 disabled/ready/unavailable。BlueZ 注册失败只重试蓝牙，不退出观测和串口服务。模拟验证必须设 BLE_ENABLED=0，以免碰真实蓝牙。

迁移现有主板使用 `tools/deploy-ducklink-rust.py`，先检测并拒绝运行中的模型/动作，候选模拟检查通过后备份服务配置、沿用身份、更换二进制、停用 ducklink-ble，验证 BLE 注册、标定与舵机寄存器保持，异常回滚原服务。旧 Python 源保留作历史/回滚，不同时启用两个蓝牙服务。

Wi-Fi 配网使用 Rust root 权限辅助服务 ducklink-wifi（无蓝牙/运动），主后端经本地 Unix socket 调用；通过 SO_PEERCRED 校验 radxa/legacy ducklink/root，非公网控制接口。受限电源辅助也为 Rust；二者源码在 src/bin，20261011-service-rust-v1 安装于 /usr/local/lib/microduck-services。保留独立权限进程，整个控制后端仍以 radxa 运行。

实物运动及手机 BLE 联调与单元测试/服务注册分开验收。


## 2026-10-10 主板信息和电源

认证状态含`observer.system`：`bootId/source/resources/resourceAgeMs/serviceUptimeSeconds/powerControl`。resources含CPU/内存/温度、board硬件与OS/内核/版本/release和boardUptimeSeconds；每2秒采样，缺失值null。App按年龄标记过期。认证加密请求`{"type":"power","action":"reboot","confirm":true,"bootId":"当前system.bootId"}`；action也可poweroff。只允许真实主板/当前会话/确认/无活动任务，经统一`system/{action}`路由复用电源辅助；接受返回accepted，失败不自动重试。主板关机不切断舵机电源。已部署board-power-v1与App0.9.19 OTA，未实物电源操作测试。

## 2026-10-10 速度范围

speed接受0–1（App10–100%），前进模型指令forward × speed × 0.4m/s；50%=0.2，100%=0.4。共享drive_twist前后输入±0.4，反向2倍补偿封顶−0.4；转向输入限±0.5、补偿后±1rad/s，侧移±0.1m/s。超范围拒绝，250ms租约不变。网页轮盘和walk启动速度同步最高0.4，默认值保持。

## 2026-10-11 kissqy同步控制核查

核对基准固定为kissqy提交94328f5b43c8ac825c90f0e564e7d1b926cd770d及各模型deployment-contract。行走/起身/坐站/拾取/翻滚均同上游权重；兼容文件名xgoduck不代表仍用旧XgoDuck模型。所有kissqy入口现在同步50Hz read→observe→infer→write；旧本机训练模型入口仍独立保留，不把它们称作kissqy模型。

起身/坐站从当前位置使用模型增益上力；HOME用模型增益和两秒线性过渡。起身直接启动的六秒窗口结束回HOME保持；运行中接管起身则到期交回行走。没有达到直立不会无限重复同一模型，也不会将窗口结束当成成功。原始动作历史独立于电压倍率，native速度延迟随采样推进一次。电压补偿采用固定提交源码默认true，而非已读到供体现机开关；此选项及用户持久增益是运行配置，不能靠模型哈希宣称两台实机设置一样。

本机设备适配仍保留已校准外置IMU、编码零点/方向、EEPROM、工作电压/故障/数据年龄和用户要求的行走跌倒卸力；供体的全套robotd外设、音频/游戏手柄/低电量关机等不属于本次策略移植。新发布只读验收，不是实机起身成功验收。完整证据见docs/实测记录/附件/平台/2026-10-11/kissqy-native。
