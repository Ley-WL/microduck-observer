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

`bootstrap-package.json`记录当前已在87验证的引导包位置和SHA。包保存在本项目 `deploy/staging/bootstrap/payload.tar.gz`，约24MB，不含密码/身份/标定；此本地包不入Git，保留它才能离线重复部署。这是原SD备份时 `runtime-20261013-085643` 的**冻结版本**，不会自动冒充最新源码构建。新版本须另制并验证相同结构的服务包，显式指定 `--bundle <路径> --sha256 <值>`。包包含 `release/{bin,models,frontend,lib}` 和白名单内的 `system/` 服务模板；不接受软硬链接或任意系统路径。

远程sudo密码从隐藏输入读取，或显式 `--password-file` 读取本地Password文件，只经stdin传输，不写到包、证据、CLI参数或技能。每台SSH主机密钥保存在 `deploy/staging/new-board-known-hosts`，密钥变更必须人工核验，不自动绕过。

实际安装案例：192.168.31.87 / DuckLink-4152e0，见实测记录2026-10-13/new-board87。本次通用脚本验证为本地测试、包解压检查和87只读检查；未用通用脚本重复覆盖该设备。
