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

服务面向可信局域网，未实现身份认证，不应直接暴露到公网。实机发布传感器坐标姿态，未完成安装轴向校准，也没有舵机控制。当前浏览器刷新保留标定；服务重启后需重新标定 IMU。

## 可选：URT-2 舵机反馈

先关闭其他占用串口的软件。使用 `ls -l /dev/serial/by-id/` 核对实际适配器路径；下方 SERIAL_PATH 必须替换为你的设备路径。运行 `sudo systemctl edit microduck-observer` 添加：

```ini
[Service]
Environment=MICRODUCK_SERVO_PORT=SERIAL_PATH
Environment=MICRODUCK_SERVO_IDS=11,12,13,14,21,22,23,24
SupplementaryGroups=dialout
DeviceAllow=char-ttyACM rw
```

若适配器为 ttyUSB 则将设备类别改为 char-ttyUSB。SupplementaryGroups 与原 service 的 i2c 组累加。然后执行 `sudo systemctl restart microduck-observer`。无设备、无应答或拔线会在页面显示异常；重新连接后自动重试。该配置仅开启读取，不改变扭矩或机械位置。服务重启会使页面初始 IMU 标定失效，需重新标定。

全部 15 颗已接线时，将上面的 ID 配置替换为：

```ini
Environment=MICRODUCK_SERVO_IDS=10,11,12,13,14,20,21,22,23,24,30,31,32,33,34
```

2026-09-22 实机只读验证：15 个 ID 在约 8 秒、112 组反馈中均在线且故障位为 0；完整总线推送约 13.8 Hz。20 Hz 是目标上限，实际速率受串行读取耗时和网络影响。此结果仅验证反馈通信，不代表机械运动已验证。
