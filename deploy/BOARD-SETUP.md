# 新设备配置与通路验收

入口技能：`microduck-new-board-deploy`。各阶段单独报告结果，不能把镜像写入、服务ready、UART回环和真实舵机控制混称成功。

| 阶段 | 入口与完成依据 |
|---|---|
| 刷纯净系统 | 本机 `D:/Users/wl/Downloads/MicroDuck-Flash/prepare-clean-wifi.py` / `write-clean-card.py`：官方Armbian26.2.1源SHA、目标USB盘身份/容量/非系统盘、用户清空授权、镜像及写后整段读回SHA。当前模板限定磁盘3、MXT序列150101v01、32GB，其他读卡器/容量必须重新核对脚本，不能删断言硬刷。 |
| Wi-Fi与SSH | 准備脚本JSON经stdin传ssid/password/ssh_password/public_key；凭据不入CLI/公开记录。netplan DHCP/权限600，已有系统的aic8800_fdrv_sdio模块显式加载。首次启动3～5分钟，取得真实IP并SSH验证；不能仅复制netplan就宣称联网。 |
| 服务部署 | [NEW-BOARD.md](NEW-BOARD.md)与 `tools/deploy-new-board.py`。独立身份、空标定、UART、音频/I²C自定义dtbo、权限、服务自启、重启bootId变化与广播检查。最新包来源/SHA明确。 |
| UART2外部回环 | 断电拔掉HAT/TTL，仅Pin8-Pin10短接后通电。执行下方正式工具，115200及1Mbps各3次48字节，6/6 exactMatch；服务生命周期恢复。 |
| HAT回包 | 断电取下回环跳线、接回HAT，TTL保持拔除，确认独立舵机供电。先采API和内核serial计数，必要时暂停平台独立只读56/15批读/单读。有效校验/ID及缺帧统计，不能把0字节当解析丢帧。 |
| IMU与标定 | UART9有效gyro/accel/quaternion、实际方向对照。新机独立标定；用户要求沿用老安装方向时仅完整段保留式修改安装矩阵/yaw，保留新参考/位置/关节。无需从旧机复制整份标定。 |
| App及实机动作 | 广播注册不等于手机绑定成功；用户实际绑定。运动另需用户托稳/授权，遵循03最新规则，不作为新板部署自动测试。 |

回环工具：

```powershell
python tools/check-uart-loopback.py --host <IP> --confirmed-isolated
```

`--confirmed-isolated`只能在用户实际确认HAT/TTL拔除、Pin8/10跳线后使用，不能随部署自动传。脚本拒绝运行中的模型/动作；仅测试字节，无位置/使能写入。报告保存在附件/HAT/<日期>/loopback-*。测试结束后**断电取下跳线**再接HAT。

外部失败后的路径：先核对实际针脚/导通和pinmux；必要时UART控制器内部回环（TIOCM_LOOP，必须finally清除）。内部通过不能证明排针通路正常。GPIO电平测试须先unbind UART并在HAT/TTL移除时执行，finally输入态/unexport/rebind/恢复服务；不是默认验收步骤。GPIO自身能0/1而另一端恒0，查物理针脚/跳线/映射，不能只凭内部回环就认定驱动或HAT没问题。

系统不联网：先确认是否真正进入Linux。SD离线无新增挂载/启动日志只支持早期启动问题方向，不证明Wi-Fi坏。只读TTL先GND→Pin6、RXD→Pin8，3.3V、1500000/8N1/无流控、先开串口再上电。HAT接回前拔掉TTL；交互TX→Pin10需用户确认3.3V并排除HAT共驱动。乱码不是有效错误日志，不按乱码猜根因或盲目改波特率。不同板RAM可不同，不能用旧板日志代替新板。

默认走已实机成功的纯净系统→联网→部署路线。64GB完整镜像不能直接写32GB，容量转换要离线副本验证；此前重建分区/克隆未能启动的镜像不能当已验证模板。本机刷卡脚本/镜像保存在Downloads，尚非可在任意电脑直接使用的便携安装器。
