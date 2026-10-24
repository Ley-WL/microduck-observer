# Observer J5 ToF自动权限准备

ToF通过HAT J5的`/dev/i2c-3`访问。设备节点的Unix权限和systemd的`DevicePolicy=closed`白名单必须同时放行；用户属于i2c组并不足以解决全部权限问题。

`deploy/tof_prepare.py --install`安装root拥有的准备程序、`microduck-tof-prepare.service`、Observer的`tof-j5.conf`和定向udev规则。新板安装上传该程序；运行时更新包也携带并安装它。既有设备只有原二进制更新、没有安装此系统组件时，不会自动获得该功能。

每次启动/重启Observer前，systemd通过Wants/After启动一次root准备服务。准备程序仅核验I2C3字符设备89:3并将其设为root:i2c、0660，不跟随符号链接，不读取或写入传感器数据，也不访问舵机串口。Observer和ToF采集仍以radxa运行；drop-in仅放行I2C3，不放开所有设备。

udev规则负责节点创建或重新创建时的权限；启动准备负责修正已经存在的错误权限。准备服务没有RemainAfterExit，因此后续Observer重启会再次执行。初始化可有限等待i2c-dev节点，设备缺失时保存诊断并结束，不阻止Observer启动，不自动重启主板。ToF启用偏好不改变。

诊断位于`/run/microduck-tof-permissions.json`，包含ready、error、执行时间、pid及固定总线路径。服务日志通过`journalctl -u microduck-tof-prepare`查看。缺传感器或I2C错误不能靠修改权限自动修好，应分别检查J5连接和总线状态。

本地验证：5项权限准备测试覆盖普通文件、符号链接、错误字符设备拒绝，目标文件描述符授权，以及缺设备的非阻断诊断；另有4项音频准备与7项安装器测试通过，runtime包及内嵌安装脚本编译/内容核验通过。

2026-10-23：141先前静态I2C3权限修复后，40个不同ToF遥测样本完整、约15Hz。自动准备服务的新代码安装时设备已被用户断电，SSH未连接成功；新服务的启动/权限恢复实机验证尚未完成，不能沿用之前的静态修复结果作为自动修复验收。
