# DuckLink 统一 Rust 后端

`microduck-observer` 同一进程运行 BlueZ GATT、DuckLink/1 AES-GCM 认证、意图控制和 Web API。BLE 从内存读取遥测，动作复用进程内控制入口和唯一 UART owner，不再由 Python HTTP 网关轮询/转发。预备姿势将任务交给同一 owner，异步收结果，BLE 急停/心跳不会等待姿势完成。

兼容旧 Service/RX/TX UUID、MTU≥185、8192 字节分片、HMAC 派生、AES-GCM 方向密钥及递增序号。沿用 `/var/lib/ducklink/identity.json` 的 robotId/setupCode/owner，升级不生成替代身份。一个 BLE App peer，可与蓝牙 HID 手柄同时连接；App 认证不占运动控制权，只有显式运动操作才获取与手柄共享的输入租约；认证前不能执行命令，另一手机不能驱逐当前连接。Web/BLE 仍共享运动任务锁，BLE 不接管网页模型会话。

连接默认零指令待机，无需解锁；主动急停卸力，点击恢复仅解除状态，不自动使能/启动。250ms 指令过期归零，500ms 无有效命令停止本控制端模型并保持；断开停止本人模型且回到待机，重连不恢复旧行走。IMU/舵机反馈暂停清方向，恢复需重新按住。共享标定、舵机限位/故障与工作电压检查保持。

启用配置 `MICRODUCK_BLE_ENABLED=1`，身份路径 `MICRODUCK_BLE_IDENTITY`。`GET /api/v1/health` 的 bluetooth 字段报告 disabled/ready/unavailable。BlueZ 注册失败只重试蓝牙，不退出观测和串口服务。模拟验证必须设 BLE_ENABLED=0，以免碰真实蓝牙。

BLE与运动已并入Rust。当前主后端打包、只读候选验证和切换入口见 `deploy/RUST.md`；辅助服务使用 `tools/deploy-service-rust.py`。旧版本专用迁移脚本已删除，不同时启用两个蓝牙服务。

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


## 2026-10-12 头部轮盘

BLE command 可携带 headPitch/headYaw（有限数字−1至1，缺省0）。沿用同一个认证会话、请求seq和完整应答，与forward/turn独立。主板状态根节点 headControl=true 表示支持接口，headReady表示本BLE行走会话处于kissqy同步行走模型且无待执行动作/反馈暂停/急停。其它模型不接受非零头部量。

正pitch为抬头（机械#31负方向），正yaw为左转（#32正方向）。偏转范围±25°/±35°，在现有模型目标上合成，机械边界与硬件编码检查保持。唯一UART owner每20ms至多改变1.2°偏转，松手/超过250ms未更新回零偏转。技能切换、反馈暂停、停止/断开清请求，不能复播旧输入。手机旧固件缺少能力标记时锁定轮盘并提示更新；后台心跳携带零头部量。实际方向与稳定性待实机验收。

2026-10-12 quack同包携带当前headPitch/headYaw，在本BLE会话native行走就绪时刷新头部意图；身体twist仍归零。嘴部busy失败不重置有效头部输入，保护状态/非本会话/技能切换仍清头部，缺省0及250ms过期规则不变。App保留轮盘pointer并允许另一指点击嘎一下；版本0.9.50配套runtime-20261012-220325，实机并行动作待验收。

2026-10-14：状态新增 `controlOwned` / `controlBusy`。空闲 drive 心跳不抢占控制，非控制端 reset/idle/断连/超时不清另一端头部输入；双方急停均走全局卸力。停止保持释放运动权而保留连接，另一端须显式发起动作，旧方向不自动接管。物理蓝牙双连接仍待主板实测。

## 2026-10-17 多手机绑定、单App连接（未部署）

`identity.json` 保留旧 `owner: {client,key}`，新增 `owners: {client: keyHex}` 保存其他手机；旧密钥优先且不被固定初始化码覆盖，加载不重写身份文件。成功加密认证才原子保存新手机，解绑仅移除当前手机。hello.bound表示当前client是否已有绑定；协议版本、派生算法、App存储不变。新手机沿用既有自动初始化认证方式。

同一时刻只接受一个App peer。其他地址写入拒绝，不驱逐当前手机；当前通知通道存活时拒绝新订阅替换，订阅结束后才允许新通道。仍需原手机在App里断开，退到后台的保活不等于断开。HID手柄/运动控制租约规则保持。

降级旧服务只识别legacy owner，不支持新增owners；维护脚本若需要清空全部手机，必须明确同时清owner与owners。此变更未部署，真实双手机轮换和BlueZ行为待验收。
