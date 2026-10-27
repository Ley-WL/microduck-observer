> 2026-10-27：新板工具上传当前audio_prepare.py，安装器在校验冻结包后使用该准备逻辑；音频每次启动精确核对Line输出、右DAC路由及旁路，修复静音并回读，已有设备保留全部播放音量。硬件I²C错误仍报告ready=false，不用重试或自动调大音量掩盖。226已验证单侧静音自动修复及重复执行无写入。

> 2026-10-17 自动准备包：包含已校验的6.1.115 HAT codec模块及`hardware/audio/audio_prepare.py`。新板/普通runtime安装器均安装root拥有的音频准备服务，在Observer前执行有限等待；失败写`/run/microduck-audio.json`而不重启机器人。ALSA权限使用`char-alsa`允许晚注册设备；首次初始化默认PCM0dB/Line0dB，以后保留音量，仅纠正DAC/Bypass路由。模块kernel/vermagic/SHA不符拒绝安装，不在线下载任意驱动。冻结包已更新但新板整板重启验收待做。
>
> 服务首次读到旧标定缺少mountingQuaternion时，自动补项目默认安装方向、yaw及缺失位置，保留已有位置/关节零点/IMU归零与target，revision加1并保留previous备份；已有明确安装四元数不覆盖。普通runtime验证仅放行这项精确迁移，不能忽略其它标定变化。手柄自动发现/重连已有，明确输入接管运行中的兼容模型采用现有控制权保护；未取得会话的X/技能请求现在给明确提示，不再静默忽略，不自动启动或退出保护。

> **2026-10-16 默认IMU安装配置（用户授权，项目默认）：** 从8.133.210.70:5723只读取得当前安装四元数、mounting.yaw与positionMm，保存deploy/imu-installation.default.json。新板安装器及Rust首次创建标定读取同一默认值，已有calibration.json不覆盖；新设备initialized=false，quaternion=null、bootId/time空，必须自行归零，不克隆来源机的关节零点或设备身份。完整来源IMU段留存附件/IMU/2026-10-16/default-installation/source.json，仅作备份，不作新板参考。187 Rust通过/1忽略，Linux安装脚本7测试通过；未部署或改来源机配置，未实机动作验收。

# 新主板一键部署

通用模型默认 PD 由 `deploy/model-gains.default.json` 统一为 P5/D0。新板工具上传该模板，启动前创建 `/var/lib/microduck-observer/model-gains.json`；已有保存文件不覆盖，启动后核验 API 与保存值一致。Rust 缺配置时使用编译内嵌的同一模板。平台保存设置优先，下次模型启动生效；嘴部独立参数不属于此配置。新安装脚本可为冻结安装包初始化默认值，不代表包内二进制已重新构建。

适用于已刷好 Armbian 26.2.1 / Radxa ZERO 3W、已联网并允许电脑SSH公钥登录的**新设备**。不负责刷卡或Wi-Fi首次配置。现有设备更新使用 `tools/deploy-runtime.py`，不可用此流程覆盖身份和标定。

Windows双击 `tools/deploy-new-board.cmd`，输入新主板IP及sudo密码。也可运行：

```powershell
python tools/deploy-new-board.py --host 192.168.31.87
```

自动完成：目标检查、校验并传输服务包、安装Rust主服务/模型/网页/ORT及Wi-Fi/电源助手、补齐i2c和ducklink权限/系统账号、生成独立hostname/robotId/BLE名称、启用开机服务、修正rk3568 UART2/9及音频overlay、重启和验证。只生成空的新设备标定，不套用原机器人零点；无动作/使能/EEPROM写入。

脚本仅接受ARM64、已知DTB和所需overlay文件；检测已有current、身份或标定即拒绝。重新运行不会覆盖已部署设备。失败保留阶段和 `/var/lib/microduck-bootstrap/<release>`；不自动删除身份、无限重试或盲目重装。网络地址变化/180秒启动验收超时必须核对新IP和日志。重启后的UART、服务和蓝牙广播通过不等于App绑定/实机控制验收。

只读检查（不会安装、修改或重启）：

```powershell
python tools/deploy-new-board.py --host 192.168.31.87 --check
```

`bootstrap-package.json` 固定当前本地安装包 `deploy/staging/bootstrap/payload-gamepad-auto-20261017.tar.gz` 和 SHA256。2026-10-17 包在原 2026-10-14 包基础上更新 ARM64 主服务和手柄 input 只读权限，其余模型、网页、库及辅助程序逐字节保持。194 项 Rust 测试、6 项安装器测试、归档白名单/内容校验与87板只读手柄自动识别通过；新版包尚未执行完整新板安装/重启验收，不沿用旧包的验收结论。包不含密码/身份/标定，不入Git，须保留以离线重复部署。可显式指定 `--bundle <路径> --sha256 <值>`；包仅接受 `release/` 和白名单 `system/` 文件，不接受软硬链接。

新板安装自动加入 input 组并安装 `SupplementaryGroups=input`、`DeviceAllow=char-input r`。服务启动即可等待兼容手柄，连接后自动识别按键和摇杆，断连/设备编号变化后重扫，无需手写 eventN、名称或轴配置。自动识别不自动配对、不启动模型；首次控制许可仍为只读诊断，网页保存后跨重启恢复。

远程sudo密码从隐藏输入读取，或显式 `--password-file` 读取本地Password文件，只经stdin传输，不写到包、证据、CLI参数或技能。每台SSH主机密钥保存在 `deploy/staging/new-board-known-hosts`，密钥变更必须人工核验，不自动绕过。

实际通用脚本安装案例：192.168.31.87 / DuckLink-1c15e7，见实测记录2026-10-14/bootstrap-20261014-224027。重启后bootId、UART2/9、服务和广播验证通过。纯净系统缺自定义I²C/音频overlay时，当前包已携带这两份经过230使用的dtbo，安装器白名单检查后安装。
