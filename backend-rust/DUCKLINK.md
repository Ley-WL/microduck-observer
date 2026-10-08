# DuckLink 统一 Rust 后端

`microduck-observer` 同一进程运行 BlueZ GATT、DuckLink/1 AES-GCM 认证、意图控制和 Web API。BLE 从内存读取遥测，动作复用进程内控制入口和唯一 UART owner，不再由 Python HTTP 网关轮询/转发。预备姿势将任务交给同一 owner，异步收结果，BLE 急停/心跳不会等待姿势完成。

兼容旧 Service/RX/TX UUID、MTU≥185、8192 字节分片、HMAC 派生、AES-GCM 方向密钥及递增序号。沿用 `/var/lib/ducklink/identity.json` 的 robotId/setupCode/owner，升级不生成替代身份。一个 BLE peer、一份控制租约；认证前不能执行命令，另一手机不能驱逐当前连接。Web/BLE 仍共享运动任务锁，BLE 不接管网页模型会话。

连接默认零指令待机，无需解锁；主动急停卸力，点击恢复仅解除状态，不自动使能/启动。250ms 指令过期归零，500ms 无有效命令停止本控制端模型并保持；断开停止本人模型且回到待机，重连不恢复旧行走。IMU/舵机反馈暂停清方向，恢复需重新按住。共享标定、舵机限位/故障与工作电压检查保持。

启用配置 `MICRODUCK_BLE_ENABLED=1`，身份路径 `MICRODUCK_BLE_IDENTITY`。`GET /api/v1/health` 的 bluetooth 字段报告 disabled/ready/unavailable。BlueZ 注册失败只重试蓝牙，不退出观测和串口服务。模拟验证必须设 BLE_ENABLED=0，以免碰真实蓝牙。

迁移现有主板使用 `tools/deploy-ducklink-rust.py`，先检测并拒绝运行中的模型/动作，候选模拟检查通过后备份服务配置、沿用身份、更换二进制、停用 ducklink-ble，验证 BLE 注册、标定与舵机寄存器保持，异常回滚原服务。旧 Python 源保留作历史/回滚，不同时启用两个蓝牙服务。

Wi-Fi 配网仅保留 root 权限辅助服务 ducklink-wifi（无蓝牙/运动），Rust 经本地 Unix socket 调用；通过 SO_PEERCRED 校验 radxa/legacy ducklink/root，非公网控制接口。主板已用独立电源辅助服务，此处同样避免给整个控制后端 root 权限。

实物运动及手机 BLE 联调与单元测试/服务注册分开验收。
