# Radxa ZERO 3W 部署

示例固定使用 Linux 用户 radxa、/dev/i2c-4 和 BNO085 地址 0x4B。其他设备请先调整 service、安装脚本及配置，并核对实际连线。需要 Python 3.11+、python3-venv、I2C 设备节点及 fuser（通常来自 psmisc）。

## 电脑端构建

```sh
cd frontend
npm ci
npm run build
cd ..
python deploy/package.py
scp deploy/staging/observer.tar.gz radxa@<主控IP>:~/observer.tar.gz
```

SSH 连接需核对主机密钥，不要关闭主机密钥验证。

## 主控端首次安装

```sh
mkdir -p /home/radxa/microduck-observer/releases/v0.3.0
tar -xzf ~/observer.tar.gz -C /home/radxa/microduck-observer/releases/v0.3.0
python3 -m venv /home/radxa/microduck-observer/venv
/home/radxa/microduck-observer/venv/bin/python -m pip install -r /home/radxa/microduck-observer/releases/v0.3.0/debug-server/requirements.txt
sudo bash /home/radxa/microduck-observer/releases/v0.3.0/deploy/install-service.sh /home/radxa/microduck-observer/releases/v0.3.0
```

访问 http://<主控IP>:8877/ 。前端、API 和 WebSocket 由同一服务提供。安装脚本配置设备权限及 systemd 开机启动。服务只有一个 worker；运行其他 IMU 诊断程序前先停止本服务。

## 检查和维护

```sh
systemctl status microduck-observer
sudo journalctl -u microduck-observer -n 100 --no-pager
sudo systemctl restart microduck-observer
sudo systemctl stop microduck-observer
```

电脑安装 websockets 后运行 `python deploy/check_live.py http://<主控IP>:8877 --seconds 12` 验证数据流。

更新时解压到新的 release 目录，安装对应依赖，再执行安装脚本。previous-release.txt 保存上一版本路径；回滚时停止服务，恢复对应依赖和 current 链接，再启动。

服务面向可信局域网，未实现身份认证，不应直接暴露到公网。实机发布传感器坐标姿态，未完成安装轴向校准，也没有舵机运动控制；姿势标定页可显式执行硬件中位校准。标定保存于主控，刷新或更换浏览器自动恢复；服务重启后恢复 IMU 参考显示，核对方向后可确认沿用；不一致时重新标定。

## 可选：URT-2 舵机反馈

先关闭其他占用串口的软件。使用 `ls -l /dev/serial/by-id/` 核对实际适配器路径；下方 SERIAL_PATH 必须替换为你的设备路径。运行 `sudo systemctl edit microduck-observer` 添加：

```ini
[Service]
Environment=MICRODUCK_SERVO_PORT=SERIAL_PATH
Environment=MICRODUCK_SERVO_IDS=11,12,13,14,21,22,23,24
SupplementaryGroups=dialout
DeviceAllow=char-ttyACM rw
```

若适配器为 ttyUSB 则将设备类别改为 char-ttyUSB。SupplementaryGroups 与原 service 的 i2c 组累加。然后执行 `sudo systemctl restart microduck-observer`。无设备、无应答或拔线会在页面显示异常；重新连接后自动重试。该配置仅开启读取，不改变扭矩或机械位置。服务重启会恢复已有 IMU 参考显示；核对方向后可确认沿用，无需强制重标。

全部 15 颗已接线时，将上面的 ID 配置替换为：

```ini
Environment=MICRODUCK_SERVO_IDS=10,11,12,13,14,20,21,22,23,24,30,31,32,33,34
```

2026-09-22 实机只读验证：15 个 ID 在约 8 秒、112 组反馈中均在线且故障位为 0；完整总线推送约 13.8 Hz。20 Hz 是目标上限，实际速率受串行读取耗时和网络影响。此结果仅验证反馈通信，不代表机械运动已验证。

共享标定持久目录由 service 的 `StateDirectory=microduck-observer` 创建，文件路径为 `/var/lib/microduck-observer/calibration.json`。从旧版升级须同步更新 service，再执行 daemon-reload 和重启；仅替换前端不会启用后端持久化接口。

## 摄像头实时画面（IMX219 / mediad）

“画面”页复用主控现有 `mediad` 的 WebRTC 服务，不新增摄像头采集进程。浏览器连接观测后端同一主机的 `8443` 信令端口，视频走 WebRTC；IMU、舵机遥测继续使用原有 WebSocket。适用于当前 HTTP 局域网部署，不支持通过纯 HTTP 反向代理直接转发视频或跨公网 NAT。

主控先安装并启用原机器人媒体服务和 Rockchip 摄像头驱动。在 `/etc/robot/robotd.toml` 的 `[media]` 中设置：

```toml
source = "camera"
quality = "1080p30"
```

备份配置后只重启 `mediad`。保持 `rkaiq_3A` 运行；曝光、白平衡由板上 IQ 配置维护，观测平台不会覆盖它们。媒体服务仍占用摄像头，独立 V4L2 抓帧测试前须停止它。

进入画面页自动连接，可停止、重连或全屏；离开页面关闭 WebRTC 和信令连接。接收帧率/码率来自浏览器实际入站统计，断流时显示停滞状态。客户端不发送任何机器人控制 RPC，也不请求电脑摄像头或麦克风权限。媒体服务当前有独立的控制能力，本平台不会改变它的访问策略。

2026-09-25 实机验证：IMX219 1920×1080，浏览器约30fps；使用此前实测的固定曝光/白平衡，自动曝光/白平衡尚未恢复。停止、重新连接及切页释放会话通过浏览器验证。

2026-09-23 更新：独立进程同步只读采集已实测约 49.36 Hz，最新测量与字段说明见 README 的采集优化验证。上述 13.8 Hz 为旧版本历史结果。

## ToF 距离热图（VL53L5CX / tofd）

硬件模式默认订阅 `/run/tofd/tof.sock` 的 `tof.stream`，不新增 I²C 采集进程。使用 `MICRODUCK_TOF_SOCKET` 可指定路径，设为空字符串可禁用。运行用户需要 socket 所属 `robot` 组权限；本次主控 radxa 已属于该组。不要为了接入热图重启 tofd 或改写传感器参数。

浏览器“传感器”页经平台原有 WebSocket 接收 `tof` 主题，显示 8×8 距离、状态、有效区域统计和实际频率。只将 status=5 计为有效；原始分区顺序未经安装方向标定。断流3秒后后端重新订阅，页面超过1.5秒标记过期。全局暂停冻结热图，恢复后显示最新帧。模拟模式默认不生成 ToF 假数据。

2026-09-29 已部署 `releases/20260929-tof`，只切换平台 release 并重启 microduck-observer，现有 systemd 配置和 tofd 保留。旧 release 路径保存在主控 `previous-release-tof-20260929.txt`；标定文件已备份为 `/var/lib/microduck-observer/calibration.pre-tof-20260929.json`。回退可将 current 重新链接到该旧 release 再重启平台。现有安装脚本的 I²C 独占检查不适用于已经运行 tofd 的更新场景，此次更新没有调用它。

## MS901M 串口IMU（UART4）

当前实机使用MS901M。启用Armbian的uart4-m1后，物理Pin16为主控RX（接IMU TX）、Pin18为主控TX（接IMU RX）。Pin8/10是U-Boot启动串口，不用于该主动输出模块。引脚接线与电平核对见实测文档。

设置观测服务drop-in：
```ini
[Service]
Environment=MICRODUCK_IMU_DRIVER=ms901m
Environment=MICRODUCK_IMU_PORT=/dev/ttyS4
SupplementaryGroups=dialout
DeviceAllow=/dev/ttyS4 rw
```

默认仍支持bno085驱动。MS901M以115200/8N1接收；仅发送量程寄存器读取请求，不写配置。模块RX回路需接通才能查询量程；未收到合法量程应答时仍可发布四元数，但不输出猜测倍率的加速度/角速度。health显示rangeState、ranges、checksumErrors和报告计数。替换传感器后须备份并清除旧IMU参考/安装校准，不能照搬旧BNO085参数；保留舵机标定。

2026-09-29部署到20260929-ms901m，备份和回退步骤见实测记录02-IMU实测.md。本次更新未调用原安装脚本（它的I²C独占检查针对BNO085，不适用于当前UART4/ToF并行运行）。


## HD1910 v5 模型站立维持（2026-10-01）

发布包包含debug-server/models/hd1910-head-v5.onnx和元数据；requirements含numpy、onnxruntime。服务启动仅采集，平台“姿态与IMU→模型控制→实机模型v5”可显式开始站立维持、停止并保持、全部失能和只读检查。启动先过渡到模型HOME，头颈俯仰各20°，然后50Hz目标闭环；实际频率由成功总线发令统计。当前仅站立策略，移动及其他技能仍为操作预览。

GET /api/v1/policy返回模型元数据与状态；POST /api/v1/policy/start和/shadow使用JSON revision；POST /stop结束模型并保持。全部失能沿用POST /api/v1/servos/disable。模型状态经既有system WebSocket同步。只读检查给出当前姿态预测目标，不会执行HOME过渡或舵机写入；currentPoseTargetsCompatible=false时不能称该姿态已适配实机。

模型不从倒地任意姿态起立；请托稳直立后启动。v5尚未完成实机平衡验证；不会因为模型已上传就标记hardwareValidated=true。部署证据与回滚路径见实测记录。
